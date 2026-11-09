use super::process::{Process, cli_command};
use super::runtime::Job;
use super::{AiEvent, PublicText};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

mod subagents;

pub(super) const MCP_INVENTORY_ID: u64 = 4;

pub(super) fn run(job: &Job) -> Result<(), String> {
    let mut command = cli_command(&job.request.config)?;
    command.args(["app-server", "--listen", "stdio://"]);
    // Per-process overrides only; never change the user's global CLI config.
    command.args([
        "-c",
        "sandbox_mode=\"read-only\"",
        "-c",
        "features.shell_tool=false",
        "-c",
        if job.palette_search() {
            "web_search=\"live\""
        } else {
            "web_search=\"disabled\""
        },
        "-c",
        "features.apps=false",
        "-c",
        "features.plugins=false",
        "-c",
        "features.hooks=false",
        "-c",
        "features.codex_hooks=false",
        "-c",
        if job.native_subagents() {
            "features.multi_agent=true"
        } else {
            "features.multi_agent=false"
        },
        "-c",
        "features.js_repl=false",
    ]);
    let native = job.native_profile();
    if let Some(native) = native {
        command.args([
            "-c",
            "features.multi_agent_v2=false",
            "-c",
            "agents.max_threads=6",
            "-c",
            "agents.max_depth=1",
        ]);
        command.env(
            native.codex_mcp_token_env,
            &job.request.mcp.as_ref().unwrap().token,
        );
    }
    let process = Process::spawn(&mut command, &job.request.working_dir)?;
    process.send(json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":job.request.host.client_id,"title":job.request.host.name,"version":job.request.host.version},"capabilities":{"experimentalApi":true}}}))?;
    let mut thread = String::new();
    let mut text_blocks: HashMap<String, PublicText> = HashMap::new();
    let mut reasoning = ReasoningStream::default();
    let mut turn_started = false;
    let mut server_overrides = serde_json::Map::new();
    let mut inventory_pages = 0;
    let mut agents = subagents::Observer::default();
    loop {
        job.check()?;
        let frame = match process.lines.recv_timeout(Duration::from_millis(50)) {
            Ok(frame) => frame?,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(_) => return Err(
                "Codex CLI closed before completing the turn; check CLI login and compatibility"
                    .into(),
            ),
        };
        if frame["id"] == super::steering::REQUEST_ID {
            job.steer_ack(frame.get("error").is_none() && frame.get("result").is_some());
            continue;
        }
        if frame.get("error").is_some() {
            return Err(
                "Codex rejected the request; check CLI login, model and installed version".into(),
            );
        }
        if frame["id"] == 1 && frame.get("result").is_some() {
            process.send(json!({"method":"initialized","params":{}}))?;
            // Metadata only: never read config/read, which can contain provider credentials.
            process.send(json!({"id":MCP_INVENTORY_ID,"method":"mcpServerStatus/list","params":{"limit":100,"detail":"toolsAndAuthOnly"}}))?;
        } else if frame["id"] == MCP_INVENTORY_ID && frame.get("result").is_some() {
            inventory_pages += 1;
            if inventory_pages > 16 {
                return Err("Too many inherited MCP servers; cannot isolate Codex safely".into());
            }
            for server in frame["result"]["data"]
                .as_array()
                .ok_or("Invalid Codex MCP inventory")?
            {
                let name = server["name"]
                    .as_str()
                    .ok_or("Invalid inherited MCP identity")?;
                if name.len() > 128
                    || !name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                {
                    return Err("Cannot safely disable inherited Codex MCP server".into());
                }
                if native.is_none_or(|profile| name != profile.codex_mcp_name) {
                    server_overrides.insert(format!("mcp_servers.{name}.enabled"), json!(false));
                }
            }
            if let Some(cursor) = frame["result"]["nextCursor"].as_str() {
                process.send(json!({"id":MCP_INVENTORY_ID,"method":"mcpServerStatus/list","params":{"limit":100,"detail":"toolsAndAuthOnly","cursor":cursor}}))?;
                continue;
            }
            let mut params = thread_params(job);
            if let Some(native) = native {
                let mcp = job.request.mcp.as_ref().unwrap();
                // The editor handles consent for its authenticated, turn-scoped tools.
                server_overrides.insert(
                    format!("mcp_servers.{}", native.codex_mcp_name),
                    json!({
                        "url": mcp.endpoint,
                        "bearer_token_env_var": native.codex_mcp_token_env,
                        "enabled": true,
                        "default_tools_approval_mode": "approve",
                    }),
                );
            }
            params["config"] = Value::Object(server_overrides.clone());
            let method = if let Some(resume) = &job.request.resume {
                params["threadId"] = json!(resume.session_id);
                // The native session already owns history (including image data).
                // Only its identity is needed here; do not echo stored turns back.
                params["excludeTurns"] = json!(true);
                "thread/resume"
            } else {
                params["dynamicTools"] = if job.design_tools() {
                    dynamic_tools(job.request.tools.tool_definitions())
                } else {
                    json!([])
                };
                "thread/start"
            };
            process.send(json!({"id":2,"method":method,"params":params}))?;
        } else if frame["id"] == 2 && frame.get("result").is_some() {
            thread = frame["result"]["thread"]["id"]
                .as_str()
                .ok_or("Codex returned no session ID")?
                .to_owned();
            job.bind(thread.clone());
            job.interrupt(
                process.writer.clone(),
                json!({"id":90,"method":"turn/interrupt","params":{"threadId":thread,"turnId":""}}),
            );
            // Keep summary-only providers visible when there is no reasoning text.
            let mut params = json!({"threadId":thread,"input":turn_input(job),"approvalPolicy":"untrusted","sandboxPolicy":{"type":"readOnly"},"summary":"auto"});
            if !job.request.config.model.is_empty() {
                params["model"] = json!(job.request.config.model);
            }
            if let Some(effort) = job.request.config.reasoning_effort {
                params["effort"] =
                    serde_json::to_value(effort).map_err(|_| "Invalid reasoning effort")?;
            }
            process.send(json!({"id":3,"method":"turn/start","params":params}))?;
        } else if frame["id"] == 3 && frame.get("result").is_some() {
            if let Some(turn) = frame["result"]["turn"]["id"].as_str() {
                job.interrupt(process.writer.clone(), json!({"id":90,"method":"turn/interrupt","params":{"threadId":thread,"turnId":turn}}));
                job.steer_target(process.writer.clone(), thread.clone(), turn.to_owned());
            }
            turn_started = true;
        }
        let method = frame["method"].as_str().unwrap_or("");
        let params = &frame["params"];
        let other_thread = params["threadId"].as_str().is_some_and(|id| id != thread);
        if job.native_subagents() && !thread.is_empty() {
            agents.observe(job, &thread, method, params);
        }
        if params["threadId"].as_str() == Some(thread.as_str()) {
            for event in reasoning.receive(method, params) {
                job.emit(event);
            }
        }
        match method {
            "item/started" | "item/completed"
                if !other_thread && params["item"]["type"] == "mcpToolCall" =>
            {
                let item = &params["item"];
                if let (Some(id), Some(name)) = (item["id"].as_str(), item["tool"].as_str()) {
                    job.emit(AiEvent::ToolCall {
                        id: id.into(),
                        name: name.into(),
                        arguments: Value::Null,
                    });
                    if method == "item/completed" {
                        job.emit(AiEvent::ToolResult {
                            id: id.into(),
                            result: Value::Null,
                            is_error: item["status"] == "failed"
                                || item["result"]["isError"] == true,
                        });
                    }
                }
            }
            "turn/started" if !other_thread => {
                turn_started = true;
                if let Some(turn) = params["turn"]["id"].as_str() {
                    job.interrupt(process.writer.clone(), json!({"id":90,"method":"turn/interrupt","params":{"threadId":thread,"turnId":turn}}));
                    job.steer_target(process.writer.clone(), thread.clone(), turn.to_owned());
                }
                job.emit(AiEvent::State {
                    state: "running".into(),
                });
            }
            "item/agentMessage/delta" if !other_thread => {
                if let Some(text) = params["delta"].as_str() {
                    // The empty key keeps compatibility with older unkeyed deltas.
                    let id = params["itemId"].as_str().unwrap_or("");
                    text_blocks.entry(id.into()).or_default().record_delta(text);
                    job.emit(AiEvent::TextDelta { text: text.into() });
                }
            }
            "item/completed" if !other_thread => {
                let item = &params["item"];
                if job.palette_search()
                    && params["threadId"].as_str() == Some(thread.as_str())
                    && item["type"] == "webSearch"
                {
                    let action = &item["action"];
                    let query = item["query"]
                        .as_str()
                        .filter(|text| !text.trim().is_empty());
                    let url = matches!(action["type"].as_str(), Some("openPage" | "findInPage"))
                        .then(|| action["url"].as_str())
                        .flatten();
                    job.emit(AiEvent::PaletteSearch {
                        query: query.map(str::to_owned),
                        url: url.map(str::to_owned),
                    });
                }
                if item["type"] == "agentMessage"
                    && let Some(text) = item["text"].as_str()
                {
                    let id = item["id"].as_str().unwrap_or("");
                    let unkeyed = if id.is_empty() || text_blocks.contains_key(id) {
                        None
                    } else {
                        text_blocks.remove("")
                    };
                    let block = text_blocks
                        .entry(id.into())
                        .or_insert_with(|| unkeyed.unwrap_or_default());
                    if let Some(text) = block.snapshot(text) {
                        job.emit(AiEvent::TextDelta { text: text.into() });
                    }
                }
            }
            "item/tool/call" => {
                if frame.get("id").is_none() {
                    return Err("Codex tool call had no request ID".into());
                }
                if params["threadId"].as_str() != Some(&thread) {
                    if job.native_subagents() {
                        let result = dynamic_result(
                            json!({"error":"Use the scoped MCP tools in the assigned subagent"}),
                            true,
                            job.request.host.image_result_field,
                        );
                        process.send(json!({"id":frame["id"],"result":result}))?;
                        continue;
                    }
                    return Err("Codex tool call had an invalid session binding".into());
                }
                let name = params["tool"]
                    .as_str()
                    .ok_or("Codex tool call had no name")?
                    .to_owned();
                let id = params["callId"]
                    .as_str()
                    .ok_or("Codex tool call had no call ID")?
                    .to_owned();
                let (result, is_error) = job.call_tool(id, name, params["arguments"].clone())?;
                process.send(json!({"id":frame["id"],"result":dynamic_result(result,is_error,job.request.host.image_result_field)}))?;
            }
            "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
                // Project edits go through the host tools.
                process.send(json!({"id":frame["id"],"result":{"decision":"decline"}}))?;
                job.emit(AiEvent::State {
                    state: "Filesystem/command request denied; use host tools".into(),
                });
            }
            "item/permissions/requestApproval" => {
                process
                    .send(json!({"id":frame["id"],"result":{"permissions":{},"scope":"turn"}}))?;
            }
            "item/tool/requestUserInput" => {
                process.send(json!({"id":frame["id"],"result":{"answers":{}}}))?;
            }
            "turn/completed" if turn_started && !other_thread => {
                if params["turn"]["status"] == "failed"
                    || params["turn"].get("error").is_some_and(|e| !e.is_null())
                {
                    return Err(
                        "Codex turn failed; check CLI login, model or service availability".into(),
                    );
                }
                return Ok(());
            }
            "error" if !other_thread => {
                return Err(
                    "Codex reported an error; check CLI login or service availability".into(),
                );
            }
            "" => {}
            _ => {
                if frame.get("id").is_some() {
                    process.send(json!({"id":frame["id"],"error":{"code":-32601,"message":"Unsupported host client request"}}))?;
                }
            }
        }
    }
}
/// Codex 0.147 app-server UserInput: `image.url` accepts data URLs, while
/// `localImage.path` would require writing temporary files. Keep snapshots in memory.
fn turn_input(job: &Job) -> Value {
    let mut input = vec![json!({"type":"text","text":job.prompt(),"text_elements":[]})];
    input.extend(
        job.request
            .images
            .iter()
            .map(|image| json!({"type":"image","url":image.data_url()})),
    );
    Value::Array(input)
}

/// Codex may provide text, a summary, or both for a reasoning item. Stream text
/// immediately and keep a summary only if the completed item has no text.
#[derive(Default)]
struct ReasoningStream {
    items: HashMap<String, ReasoningItem>,
}

#[derive(Default)]
struct ReasoningItem {
    parts: BTreeMap<u64, PublicText>,
    summaries: BTreeMap<u64, PublicText>,
    completed: bool,
}

impl ReasoningStream {
    fn receive(&mut self, method: &str, params: &Value) -> Vec<AiEvent> {
        match method {
            "item/reasoning/textDelta" | "item/reasoning/summaryTextDelta" => {
                let summary = method == "item/reasoning/summaryTextDelta";
                let index_key = if summary {
                    "summaryIndex"
                } else {
                    "contentIndex"
                };
                let (Some(id), Some(index), Some(text)) = (
                    params["itemId"].as_str(),
                    params[index_key].as_u64().filter(|index| *index < 1024),
                    params["delta"].as_str().filter(|text| !text.is_empty()),
                ) else {
                    return vec![];
                };
                let item = self.items.entry(id.into()).or_default();
                if item.completed {
                    return vec![];
                }
                if summary {
                    if item.parts.is_empty() {
                        item.summaries.entry(index).or_default().record_delta(text);
                    }
                    vec![]
                } else {
                    item.summaries.clear();
                    item.parts.entry(index).or_default().record_delta(text);
                    vec![AiEvent::ReasoningDelta {
                        text: text.into(),
                        summary: false,
                    }]
                }
            }
            "item/completed" if params["item"]["type"] == "reasoning" => {
                let item = &params["item"];
                let Some(id) = item["id"].as_str() else {
                    return vec![];
                };
                let state = self.items.entry(id.into()).or_default();
                if std::mem::replace(&mut state.completed, true) {
                    return vec![];
                }
                let mut deltas = Vec::new();
                for (index, value) in item["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(1024)
                    .enumerate()
                {
                    if let Some(text) = value.as_str().filter(|text| !text.is_empty())
                        && let Some(tail) =
                            state.parts.entry(index as u64).or_default().snapshot(text)
                    {
                        deltas.push(AiEvent::ReasoningDelta {
                            text: tail.into(),
                            summary: false,
                        });
                    }
                }
                if state.parts.is_empty() {
                    for (index, value) in item["summary"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .take(1024)
                        .enumerate()
                    {
                        if let Some(text) = value.as_str().filter(|text| !text.is_empty()) {
                            // The final item may have only a snapshot for this part.
                            let mut part = PublicText::default();
                            part.record_delta(text);
                            state.summaries.insert(index as u64, part);
                        }
                    }
                    let text = std::mem::take(&mut state.summaries)
                        .into_values()
                        .filter_map(|part| part.prefix)
                        .collect::<Vec<_>>()
                        .join("\n\n");
                    if !text.is_empty() {
                        deltas.push(AiEvent::ReasoningDelta {
                            text,
                            summary: true,
                        });
                    }
                }
                state.parts.clear();
                state.summaries.clear();
                deltas
            }
            _ => vec![],
        }
    }
}

fn thread_params(job: &Job) -> Value {
    let mut params = json!({"cwd":job.request.working_dir,"sandbox":"read-only","approvalPolicy":"untrusted","developerInstructions":"Use only the supplied host tools. Never edit project files directly. Respect the editor’s approval rules for document changes."});
    if !job.design_tools() {
        params["ephemeral"] = json!(true);
        params["developerInstructions"] = if job.is_palette() {
            json!(
                "Generate color suggestions only. Use web search for visual references when requested and available. Never edit files or designs. Return only the requested JSON."
            )
        } else {
            json!(&job.request.context)
        };
    } else if let Some(native) = job.native_profile() {
        params["developerInstructions"] = json!(native.codex_instructions);
    }
    if !job.request.config.model.is_empty() {
        params["model"] = json!(job.request.config.model);
    }
    params
}
fn dynamic_tools(tools: Value) -> Value {
    Value::Array(tools.as_array().into_iter().flatten().map(|tool| json!({"type":"function","name":tool["name"],"description":tool["description"],"inputSchema":tool["inputSchema"]})).collect())
}
fn dynamic_result(result: Value, is_error: bool, image_field: &str) -> Value {
    if let Some(url) = result.get(image_field).and_then(Value::as_str) {
        let mut metadata = result.clone();
        metadata.as_object_mut().unwrap().remove(image_field);
        json!({"contentItems":[{"type":"inputText","text":metadata.to_string()},{"type":"inputImage","imageUrl":url}],"success":!is_error})
    } else {
        json!({"contentItems":[{"type":"inputText","text":result.to_string()}],"success":!is_error})
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dynamic_reference_image_is_sent_as_image_content() {
        let result = dynamic_result(
            json!({"name":"screen.png","image_data":"data:image/png;base64,YWJj"}),
            false,
            "image_data",
        );
        assert_eq!(
            result["contentItems"][1],
            json!({"type":"inputImage","imageUrl":"data:image/png;base64,YWJj"})
        );
        assert!(
            !result["contentItems"][0]["text"]
                .as_str()
                .unwrap()
                .contains("YWJj")
        );
    }

    #[test]
    fn schema_0147_dynamic_tools_have_required_type_and_input_schema() {
        let tools = dynamic_tools(
            json!([{"name":"read_document","description":"Read the document","inputSchema":{"type":"object"}}]),
        );
        assert!(!tools.as_array().unwrap().is_empty());
        for tool in tools.as_array().unwrap() {
            assert_eq!(tool["type"], "function");
            assert!(tool["name"].is_string());
            assert!(tool["inputSchema"].is_object());
        }
    }
    #[test]
    fn reasoning_stream_prefers_live_text_and_reconciles_snapshots() {
        let mut reasoning = ReasoningStream::default();
        let summary = json!({"itemId":"r1", "summaryIndex":0, "delta":"A summary."});
        assert!(
            reasoning
                .receive("item/reasoning/summaryTextDelta", &summary)
                .is_empty()
        );
        let delta = json!({"itemId":"r1", "contentIndex":0, "delta":"Inspect "});
        assert_eq!(
            reasoning.receive("item/reasoning/textDelta", &delta),
            [AiEvent::ReasoningDelta {
                text: "Inspect ".into(),
                summary: false
            }]
        );
        let item = json!({"item":{"type":"reasoning","id":"r1","summary":["A summary."],"content":["Inspect pages"]}});
        assert_eq!(
            reasoning.receive("item/completed", &item),
            [AiEvent::ReasoningDelta {
                text: "pages".into(),
                summary: false
            }]
        );
        assert!(reasoning.receive("item/completed", &item).is_empty());
        let fallback = json!({"item":{"type":"reasoning","id":"r2","summary":["Summary-only provider."],"content":[]}});
        assert_eq!(
            reasoning.receive("item/completed", &fallback),
            [AiEvent::ReasoningDelta {
                text: "Summary-only provider.".into(),
                summary: true
            }]
        );
    }
    #[test]
    fn dynamic_response_is_codex_content_items_not_mcp_content() {
        let result = dynamic_result(json!({"ok":true}), false, "image_data");
        assert_eq!(result["contentItems"][0]["type"], "inputText");
        assert_eq!(result["success"], true);
    }
}
