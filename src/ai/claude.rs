use super::process::{Process, cli_command};
use super::runtime::Job;
use super::{AiEvent, PublicText};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

mod subagents;

pub(super) fn run(job: &Job) -> Result<(), String> {
    let mut command = cli_command(&job.request.config)?;
    command.args([
        "--print",
        "--verbose",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--include-partial-messages",
        "--replay-user-messages",
        "--thinking-display",
        "summarized",
        "--permission-prompt-tool",
        "stdio",
        "--tools",
        if job.native_subagents() { "Agent" } else { "" },
        "--strict-mcp-config",
        "--disable-slash-commands",
        "--setting-sources",
        "user",
    ]);
    // Prevent hooks and arbitrary built-in/file tools; auth remains owned by CLI.
    command.args(["--settings", "{\"disableAllHooks\":true}"]);
    if !job.request.config.model.is_empty() {
        command.arg("--model").arg(&job.request.config.model);
    }
    if let Some(effort) = job.request.config.reasoning_effort {
        command.arg("--effort").arg(effort.as_str());
    }
    if let Some(budget) = job.request.config.max_budget_usd {
        command.arg("--max-budget-usd").arg(budget.to_string());
    }
    if let Some(resume) = &job.request.resume {
        command.arg("--resume").arg(&resume.session_id);
    }
    // Only an ephemeral per-process config. Never write ~/.claude/settings.json.
    if job.is_palette() {
        command.arg("--no-session-persistence");
    }
    if job.native_subagents() {
        command
            .arg("--agents")
            .arg(subagents::definitions(job).to_string());
        command.args(["--agent", "rpshop-coordinator"]);
    }
    let mcp_config = if !job.is_palette() {
        let mcp = job
            .request
            .mcp
            .as_ref()
            .ok_or("Claude requires the RPShop MCP bridge")?;
        json!({"mcpServers":{"rpshop":{"type":"http","url":mcp.endpoint,"headers":{"Authorization":format!("Bearer {}",mcp.token)}}}})
    } else {
        json!({"mcpServers":{}})
    };
    command.arg("--mcp-config").arg(mcp_config.to_string());
    let process = Process::spawn(&mut command, &job.request.working_dir)?;
    job.interrupt(process.writer.clone(), json!({"type":"control_request","request_id":"rpshop-interrupt","request":{"subtype":"interrupt"}}));
    process.send(json!({"type":"control_request","request_id":"rpshop-initialize","request":{"subtype":"initialize","hooks":{}}}))?;
    let mut prompt_sent = false;
    let mut public_output = PublicOutput::default();
    let mut agents = subagents::Observer::default();
    loop {
        job.check()?;
        let frame = match process.lines.recv_timeout(Duration::from_millis(50)) {
            Ok(frame) => frame?,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(_) => return Err(
                "Claude CLI closed before completing the turn; check CLI login and compatibility"
                    .into(),
            ),
        };
        if let Some(session) = frame["session_id"].as_str()
            && frame["type"] == "system"
            && (frame["subtype"] == "init" || frame["subtype"].is_null())
            && frame["parent_tool_use_id"].is_null()
        {
            job.bind(session.into());
            job.steer_target(process.writer.clone(), session.into(), String::new());
        }
        if job.native_subagents() {
            agents.observe(job, &frame);
        }
        match frame["type"].as_str().unwrap_or("") {
            "control_response" if frame["response"]["request_id"] == "rpshop-initialize" => {
                if frame["response"]["subtype"] == "error" {
                    return Err("Claude control initialization failed; check CLI version".into());
                }
                if !prompt_sent {
                    process.send(user_message(job))?;
                    prompt_sent = true;
                }
            }
            "system" => {
                job.emit(AiEvent::State {
                    state: "running".into(),
                });
            }
            "stream_event" => {
                for event in public_output.stream(&frame) {
                    job.emit(event);
                }
            }
            "assistant" => {
                for event in public_output.snapshot(&frame) {
                    job.emit(event);
                }
                if let Some(content) = frame["message"]["content"].as_array() {
                    for block in content {
                        if block["type"] == "tool_use"
                            && let (Some(id), Some(name)) =
                                (block["id"].as_str(), block["name"].as_str())
                        {
                            job.emit(AiEvent::ToolCall {
                                id: id.into(),
                                name: name.into(),
                                arguments: block["input"].clone(),
                            });
                        }
                    }
                }
                if frame.get("error").is_some() {
                    return Err(
                        "Claude reported an assistant error; check CLI login or model".into(),
                    );
                }
            }
            "user" => {
                job.steer_echo(frame["uuid"].as_str());
                if let Some(content) = frame["message"]["content"].as_array() {
                    for block in content {
                        if block["type"] == "tool_result" {
                            let id = block["tool_use_id"].as_str().unwrap_or("").to_owned();
                            let is_error = block["is_error"].as_bool().unwrap_or(false);
                            // Provider-supplied error bodies can contain secrets.
                            let result = if is_error {
                                json!({"error":"MCP tool failed or was denied"})
                            } else {
                                block["content"].clone()
                            };
                            job.emit(AiEvent::ToolResult {
                                id,
                                result,
                                is_error,
                            });
                        }
                    }
                }
            }
            "control_request" => {
                let id = frame["request_id"]
                    .as_str()
                    .ok_or("Claude control request had no ID")?
                    .to_owned();
                let request = &frame["request"];
                if request["subtype"] == "can_use_tool" {
                    let name = request["tool_name"].as_str().unwrap_or("").to_owned();
                    let arguments = request["input"].clone();
                    let allowed = !job.is_palette()
                        && (is_rpshop_tool(&job.request.tools, &name)
                            || job.native_subagents()
                                && subagents::allowed_call(&name, &arguments))
                        && job.permission(id.clone(), name, arguments.clone())?;
                    process.send(permission_response(&id, allowed, arguments))?;
                } else {
                    process.send(json!({"type":"control_response","response":{"subtype":"error","request_id":id,"error":"Unsupported RPShop control request"}}))?;
                }
            }
            "result" => {
                if frame["is_error"].as_bool().unwrap_or(false)
                    || frame["subtype"].as_str().is_some_and(|s| s != "success")
                {
                    return Err(
                        "Claude turn failed; check CLI login, budget or service availability"
                            .into(),
                    );
                }
                return Ok(());
            }
            "error" => {
                return Err(
                    "Claude reported an error; check CLI login or service availability".into(),
                );
            }
            _ => {}
        }
    }
}
/// SDKUserMessage.message is an Anthropic MessageParam. Image blocks use
/// Base64ImageSource, not a filename in prompt text. The CLI owns any native
/// session persistence; RPShop never writes this payload into its project/logs.
fn user_message(job: &Job) -> Value {
    let content = if job.request.images.is_empty() {
        json!(job.prompt())
    } else {
        let mut blocks: Vec<Value> = job
            .request
            .images
            .iter()
            .map(|image| {
                json!({"type":"image","source":{"type":"base64","media_type":image.media_type(),"data":image.base64()}})
            })
            .collect();
        blocks.push(json!({"type":"text","text":job.prompt()}));
        Value::Array(blocks)
    };
    json!({"type":"user","message":{"role":"user","content":content},"parent_tool_use_id":null,"session_id":job.request.resume.as_ref().map(|r|r.session_id.as_str()).unwrap_or("")})
}

/// Claude Code exposes user-facing thinking summaries through the SDK stream.
/// Signatures, redacted blocks and subagent transcripts are deliberately ignored.
#[derive(Default)]
struct PublicOutput {
    message_id: Option<String>,
    blocks: HashMap<(bool, u64), PublicText>,
}
impl PublicOutput {
    fn stream(&mut self, frame: &Value) -> Vec<AiEvent> {
        if frame
            .get("parent_tool_use_id")
            .is_some_and(|id| !id.is_null())
        {
            return vec![];
        }
        let event = &frame["event"];
        if event["type"] == "message_start" {
            self.blocks.clear();
            self.message_id = event["message"]["id"].as_str().map(str::to_owned);
            return vec![];
        }
        let Some(index) = event["index"].as_u64().filter(|index| *index < 1024) else {
            return vec![];
        };
        let value = match event["type"].as_str() {
            Some("content_block_start") => match event["content_block"]["type"].as_str() {
                Some("text") => Some((false, &event["content_block"]["text"])),
                Some("thinking") => Some((true, &event["content_block"]["thinking"])),
                _ => None,
            },
            Some("content_block_delta") => match event["delta"]["type"].as_str() {
                Some("text_delta") => Some((false, &event["delta"]["text"])),
                Some("thinking_delta") => Some((true, &event["delta"]["thinking"])),
                _ => None,
            },
            _ => None,
        };
        let Some((reasoning, text)) = value
            .and_then(|(reasoning, value)| value.as_str().map(|text| (reasoning, text)))
            .filter(|(_, text)| !text.is_empty())
        else {
            return vec![];
        };
        self.blocks
            .entry((reasoning, index))
            .or_default()
            .record_delta(text);
        vec![public_event(reasoning, text)]
    }
    fn snapshot(&mut self, frame: &Value) -> Vec<AiEvent> {
        if frame
            .get("parent_tool_use_id")
            .is_some_and(|id| !id.is_null())
        {
            return vec![];
        }
        if let Some(id) = frame["message"]["id"].as_str() {
            if self
                .message_id
                .as_deref()
                .is_some_and(|previous| previous != id)
            {
                self.blocks.clear();
            }
            self.message_id = Some(id.to_owned());
        }
        let mut events = Vec::new();
        for (index, block) in frame["message"]["content"]
            .as_array()
            .into_iter()
            .flatten()
            .take(1024)
            .enumerate()
        {
            let value = match block["type"].as_str() {
                Some("text") => Some((false, &block["text"])),
                Some("thinking") => Some((true, &block["thinking"])),
                _ => None,
            };
            if let Some((reasoning, text)) =
                value.and_then(|(reasoning, value)| value.as_str().map(|text| (reasoning, text)))
                && let Some(tail) = self
                    .blocks
                    .entry((reasoning, index as u64))
                    .or_default()
                    .snapshot(text)
            {
                events.push(public_event(reasoning, tail));
            }
        }
        events
    }
}
fn public_event(reasoning: bool, text: &str) -> AiEvent {
    if reasoning {
        AiEvent::ReasoningDelta { text: text.into() }
    } else {
        AiEvent::TextDelta { text: text.into() }
    }
}

fn is_rpshop_tool(tools: &crate::automation::ToolClient, name: &str) -> bool {
    name.strip_prefix("mcp__rpshop__")
        .is_some_and(|name| tools.supports_tool(name))
}
fn permission_response(id: &str, allowed: bool, arguments: Value) -> Value {
    let response = if allowed {
        json!({"behavior":"allow","updatedInput":arguments})
    } else {
        json!({"behavior":"deny","message":"Denied by RPShop; only explicitly approved prototype tools are permitted"})
    };
    json!({"type":"control_response","response":{"subtype":"success","request_id":id,"response":response}})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permission_protocol_matches_bidirectional_sdk_shape() {
        let frame = permission_response("approval-1", true, json!({"page":1}));
        assert_eq!(frame["response"]["request_id"], "approval-1");
        assert_eq!(frame["response"]["response"]["updatedInput"]["page"], 1);
        assert_eq!(
            permission_response("a", false, json!({}))["response"]["response"]["behavior"],
            "deny"
        );
    }
    #[test]
    fn public_summaries_and_answer_blocks_stream_once_with_snapshot_tail_fill() {
        let mut output = PublicOutput::default();
        assert!(
            output
                .stream(&json!({"event":{"type":"message_start","message":{"id":"m1"}}}))
                .is_empty()
        );
        assert_eq!(output.stream(&json!({"event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"Inspect "}}})),
            [AiEvent::ReasoningDelta { text: "Inspect ".into() }]);
        assert_eq!(output.stream(&json!({"event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"pages"}}})),
            [AiEvent::ReasoningDelta { text: "pages".into() }]);
        assert_eq!(output.stream(&json!({"event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Done"}}})),
            [AiEvent::TextDelta { text: "Done".into() }]);
        let snapshot = json!({"message":{"id":"m1","content":[{"type":"thinking","thinking":"Inspect pages first","signature":"private signature"},{"type":"text","text":"Done."}]}});
        assert_eq!(
            output.snapshot(&snapshot),
            [
                AiEvent::ReasoningDelta {
                    text: " first".into()
                },
                AiEvent::TextDelta { text: ".".into() },
            ]
        );
        assert!(output.snapshot(&snapshot).is_empty());
        output.stream(&json!({"event":{"type":"message_start","message":{"id":"m2"}}}));
        assert_eq!(output.snapshot(&json!({"message":{"id":"m2","content":[{"type":"thinking","thinking":"Inspect pages first"}]}})),
            [AiEvent::ReasoningDelta { text: "Inspect pages first".into() }]);
    }
    #[test]
    fn signatures_redactions_and_sidechain_thinking_are_not_public_output() {
        let mut output = PublicOutput::default();
        for event in [
            json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"secret"}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"redacted_thinking","data":"secret"}}),
        ] {
            assert!(output.stream(&json!({"event":event})).is_empty());
        }
        assert!(
            output
                .snapshot(
                    &json!({"message":{"content":[{"type":"redacted_thinking","data":"secret"}]}})
                )
                .is_empty()
        );
        assert!(output.stream(&json!({"parent_tool_use_id":"child","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"child transcript"}}})).is_empty());
        assert!(output.snapshot(&json!({"parent_tool_use_id":"child","message":{"content":[{"type":"thinking","thinking":"child transcript"}]}})).is_empty());
    }
    #[test]
    fn builtin_and_other_mcp_servers_cannot_be_approved() {
        let (_, tools) = crate::automation::ToolHost::new();
        assert!(!is_rpshop_tool(&tools, "Bash"));
        assert!(!is_rpshop_tool(&tools, "mcp__other__update"));
        assert!(!is_rpshop_tool(&tools, "mcp__rpshop__unregistered"));
    }
}
