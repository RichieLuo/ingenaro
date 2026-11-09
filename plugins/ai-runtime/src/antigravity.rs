//! Google Antigravity CLI's Agent Client Protocol over stdio.
use super::process::{Process, cli_command, version_supported};
use super::runtime::Job;
use super::{
    AiEvent, EngineConfig, EngineKind, ImageAttachment, McpEndpoint, ModelCatalog, ModelInfo,
};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

const SETUP_TIMEOUT: Duration = Duration::from_secs(20);
const LOGIN: &str = "请先在终端运行 agy 登录 Google，再重试";

pub(super) fn discover_models(config: &EngineConfig) -> Result<ModelCatalog, String> {
    let client = Client::start(config, &|| Ok(()))?;
    client.initialize("ingenaro-models", env!("CARGO_PKG_VERSION"))?;
    let result = client.request("session/new", client.session_params(None, "ingenaro"))?;
    let model = option(&result, "model");
    let active_model = model["currentValue"].as_str();
    let efforts = options(option(&result, "thinking_level"))
        .filter_map(|value| value["value"].as_str())
        .filter_map(effort_name)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    let models = options(model)
        .filter_map(|value| {
            let id = value["value"].as_str()?;
            if id.is_empty()
                || id.len() > 256
                || id.chars().any(char::is_control)
                || !seen.insert(id)
            {
                return None;
            }
            let is_default = Some(id) == active_model;
            Some(ModelInfo {
                id: id.into(),
                name: label(&value["name"], id, 160),
                description: label(&value["description"], "", 512),
                is_default,
                reasoning_efforts: if is_default {
                    efforts.clone()
                } else {
                    // agy also exposes supported model/effort combinations
                    // through its legacy model selector.
                    result["models"]["availableModels"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|row| {
                            row["modelId"].as_str()?.strip_prefix(id)?.strip_prefix('-')
                        })
                        .filter_map(effort_name)
                        .map(str::to_owned)
                        .collect()
                },
            })
        })
        .take(256)
        .collect::<Vec<_>>();
    client.close(&result["sessionId"]);
    if models.is_empty() {
        return Err("Antigravity CLI 未返回账户的模型目录".into());
    }
    Ok(ModelCatalog {
        models,
        source: "Antigravity CLI 实时模型目录".into(),
        warning: None,
    })
}

pub(super) fn run(job: &Job) -> Result<(), String> {
    let request = &job.request;
    let check = || job.check();
    let client = Client::start(&request.config, &check)?;
    let host = request.host;
    let capabilities = client.initialize(host.client_id, host.version)?;
    if !request.images.is_empty() && capabilities["promptCapabilities"]["image"] != true {
        return Err("当前 Antigravity CLI 不支持图片输入".into());
    }
    let mcp = request.mcp.as_ref().filter(|_| job.design_tools());
    if mcp.is_some() && capabilities["mcpCapabilities"]["http"] != true {
        return Err("当前 Antigravity CLI 不支持会话级 HTTP MCP".into());
    }
    let params = client.session_params(mcp, host.client_id);
    let result = if let Some(resume) = &request.resume
        && job.design_tools()
        && resume.tool_revision == host.tool_revision
        && capabilities["sessionCapabilities"]["resume"].is_object()
    {
        let mut resume_params = params.clone();
        resume_params["sessionId"] = json!(resume.session_id);
        let reply = client.exchange("session/resume", resume_params)?;
        if matches!(
            reply["error"]["code"].as_i64(),
            Some(-32601 | -32602 | -32002)
        ) {
            client.request("session/new", params)?
        } else {
            let mut result = response(reply, "session/resume")?;
            result["sessionId"] = json!(resume.session_id);
            result
        }
    } else {
        client.request("session/new", params)?
    };
    let session = result["sessionId"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control))
        .ok_or("Antigravity CLI 未返回有效的会话 ID")?
        .to_owned();
    let mut config = result;
    if !request.config.model.is_empty() {
        config = client.configure(&session, &config, "model", &request.config.model)?;
    }
    if let Some(effort) = request.config.reasoning_effort {
        let choice = option(&config, "thinking_level");
        let effort = effort.as_str();
        let value = options(choice)
            .filter_map(|row| row["value"].as_str())
            .find(|value| *value == effort || (effort == "xhigh" && *value == "extra"))
            .ok_or("当前 Antigravity 模型不支持所选思考强度")?;
        client.configure(&session, &config, "thinking_level", value)?;
    }
    job.bind(session.clone());
    job.interrupt(
        client.process.writer.clone(),
        json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":session}}),
    );
    client
        .process
        .send(prompt_frame(2, &session, &job.prompt(), &request.images))?;
    job.steer_target(
        client.process.writer.clone(),
        session.clone(),
        String::new(),
    );
    job.emit(AiEvent::State {
        state: "running".into(),
    });
    let mut calls = HashSet::new();
    let mut prompt_finished = false;
    loop {
        let frame = client.recv(None)?;
        match frame["id"].as_u64() {
            Some(2) if frame.get("method").is_none() => {
                prompt_finished = true;
                let steering_pending = job.close_steering_target();
                prompt_result(frame)?;
                if !steering_pending {
                    break;
                }
            }
            Some(super::steering::REQUEST_ID) if frame.get("method").is_none() => {
                let accepted =
                    frame["result"]["stopReason"].is_string() && frame.get("error").is_none();
                job.steer_ack(accepted);
                if prompt_finished {
                    if accepted {
                        prompt_result(frame)?;
                    }
                    break;
                }
            }
            _ if frame["method"] == "session/update" && frame["params"]["sessionId"] == session => {
                observe(job, &frame["params"]["update"], &mut calls);
            }
            _ => client.reply_to_agent(&frame, Some((job, &session)))?,
        }
    }
    client.close(&json!(session));
    Ok(())
}

fn prompt_result(frame: Value) -> Result<(), String> {
    let result = response(frame, "session/prompt")?;
    match result["stopReason"].as_str() {
        Some("end_turn" | "stop_sequence") => Ok(()),
        Some("max_tokens" | "max_turn_requests") => {
            Err("Antigravity 已达到本轮输出或调用上限".into())
        }
        _ => Err("Antigravity 本轮未完成".into()),
    }
}

pub(super) fn prompt_frame(
    id: u64,
    session: &str,
    text: &str,
    images: &[ImageAttachment],
) -> Value {
    let mut prompt = vec![json!({"type":"text","text":text})];
    prompt.extend(
        images.iter().map(
            |image| json!({"type":"image","mimeType":image.media_type(),"data":image.base64()}),
        ),
    );
    json!({"jsonrpc":"2.0","id":id,"method":"session/prompt","params":{"sessionId":session,"prompt":prompt}})
}

struct Client<'a> {
    process: Process,
    workspace: Workspace,
    check: &'a dyn Fn() -> Result<(), String>,
}

impl<'a> Client<'a> {
    fn start(
        config: &EngineConfig,
        check: &'a dyn Fn() -> Result<(), String>,
    ) -> Result<Self, String> {
        let workspace = Workspace::create()?;
        let mut command = cli_command(config)?;
        // HTTP MCP is supported in worker mode. Ignore personal and project
        // customizations so other MCP servers and hooks aren't added.
        command.args([
            "acp",
            "--process-per-session",
            "--include-default-customizations=false",
            "--disable-telemetry",
            "--agent",
            "ingenaro-design",
        ]);
        command.arg("--log-file").arg(workspace.0.join("acp.log"));
        Ok(Self {
            process: Process::spawn(&mut command, &workspace.0)?,
            workspace,
            check,
        })
    }

    fn session_params(&self, mcp: Option<&McpEndpoint>, server: &str) -> Value {
        let mut params = json!({"cwd":self.workspace.0,"mcpServers":[]});
        if let Some(mcp) = mcp {
            params["mcpServers"] = json!([{"name":server,"type":"http","url":mcp.endpoint,
                "headers":[{"name":"Authorization","value":format!("Bearer {}",mcp.token)}],
                "_meta":{"antigravity":{"forceAllToolsEager":true}}}]);
            params["_meta"] = json!({"antigravity":{"eagerMcpServers":[server]}});
        }
        params
    }

    fn initialize(&self, name: &str, version: &str) -> Result<Value, String> {
        let result = self.request(
            "initialize",
            json!({
                "protocolVersion":1,"clientInfo":{"name":name,"version":version},
                "clientCapabilities":{}
            }),
        )?;
        if result["protocolVersion"] != 1 || !result["agentCapabilities"].is_object() {
            return Err("Antigravity CLI ACP 协议版本不兼容".into());
        }
        if !result["agentInfo"]["version"]
            .as_str()
            .is_some_and(|version| version_supported(EngineKind::Antigravity, version))
        {
            return Err("请将 Antigravity CLI 更新到 1.5.3 或更新版本".into());
        }
        Ok(result["agentCapabilities"].clone())
    }

    fn recv(&self, deadline: Option<Instant>) -> Result<Value, String> {
        loop {
            (self.check)()?;
            if deadline.is_some_and(|end| Instant::now() >= end) {
                return Err("等待 Antigravity CLI 响应超时".into());
            }
            match self.process.lines.recv_timeout(Duration::from_millis(50)) {
                Ok(frame) => return frame,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err("Antigravity CLI 已退出".into()),
            }
        }
    }

    fn exchange(&self, method: &str, params: Value) -> Result<Value, String> {
        self.exchange_until(method, params, SETUP_TIMEOUT)
    }

    fn exchange_until(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        self.process
            .send(json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))?;
        let deadline = Some(Instant::now() + timeout);
        loop {
            let frame = self.recv(deadline)?;
            if frame["id"] == 1 && frame.get("method").is_none() {
                return Ok(frame);
            }
            self.reply_to_agent(&frame, None)?;
        }
    }

    fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        response(self.exchange(method, params)?, method)
    }

    fn close(&self, session: &Value) {
        if session.is_string() {
            let _ = self.exchange_until(
                "session/close",
                json!({"sessionId":session}),
                Duration::from_millis(600),
            );
        }
    }

    fn configure(
        &self,
        session: &str,
        result: &Value,
        category: &str,
        value: &str,
    ) -> Result<Value, String> {
        let config = option(result, category);
        if config["currentValue"] == value {
            return Ok(result.clone());
        }
        let id = config["id"]
            .as_str()
            .ok_or("Antigravity CLI 未提供所选模型设置")?;
        self.request(
            "session/set_config_option",
            json!({"sessionId":session,"configId":id,"value":value}),
        )
    }

    fn reply_to_agent(&self, frame: &Value, turn: Option<(&Job, &str)>) -> Result<(), String> {
        let (Some(method), Some(id)) = (frame["method"].as_str(), frame.get("id")) else {
            return Ok(());
        };
        let params = &frame["params"];
        if method != "session/request_permission" {
            return self.process.send(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Unsupported client method"}}));
        }
        // Only pass through this turn's editor tools. The MCP server still
        // applies the actual user approval, project scope and write policy.
        let outcome = turn
            .filter(|(job, session)| job.design_tools() && params["sessionId"] == *session)
            .and_then(|(job, _)| host_tool(job, &params["toolCall"]))
            .and_then(|_| params["options"].as_array())
            .and_then(|options| options.iter().find(|option| option["kind"] == "allow_once"))
            .and_then(|option| option["optionId"].as_str())
            .map_or_else(
                || json!({"outcome":"cancelled"}),
                |id| json!({"outcome":"selected","optionId":id}),
            );
        self.process
            .send(json!({"jsonrpc":"2.0","id":id,"result":{"outcome":outcome}}))
    }
}

fn response(mut frame: Value, method: &str) -> Result<Value, String> {
    if frame["error"]["code"] == -32000 {
        return Err(LOGIN.into());
    }
    if frame.get("error").is_some() || frame.get("result").is_none() {
        return Err(format!(
            "Antigravity CLI 无法完成 {method}，请检查 CLI 版本、登录和模型"
        ));
    }
    Ok(frame["result"].take())
}

struct Workspace(PathBuf);

impl Workspace {
    fn create() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!("ingenaro-agy-{}", uuid::Uuid::new_v4()));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&path)
            .map_err(|_| "无法创建 Antigravity 隔离工作目录")?;
        let workspace = Self(path);
        let agents = workspace.0.join(".agents").join("agents");
        std::fs::create_dir_all(&agents)
            .and_then(|()| std::fs::write(agents.join("ingenaro-design.md"), AGENT_PROFILE))
            .map_err(|_| "无法配置 Antigravity 的项目工具权限")?;
        Ok(workspace)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const AGENT_PROFILE: &str = "\
---
name: ingenaro-design
description: Work in the live design host.
mainAgent: true
subagent: false
excludeDefaultComponents: true
inheritCustomizations: false
commandExecutionPolicy: off
tools: []
---
Use only the session's supplied editor tools for design operations.
";

fn effort_name(value: &str) -> Option<&str> {
    match value {
        "extra" => Some("xhigh"),
        "minimal" | "low" | "medium" | "high" | "xhigh" | "max" => Some(value),
        _ => None,
    }
}

fn option<'a>(result: &'a Value, category: &str) -> &'a Value {
    result["configOptions"]
        .as_array()
        .and_then(|options| {
            options.iter().find(|option| {
                option["id"] == category
                    || option["category"] == category
                    || (category == "thinking_level" && option["category"] == "thought_level")
            })
        })
        .unwrap_or(&Value::Null)
}

fn options(option: &Value) -> impl Iterator<Item = &Value> {
    option["options"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|value| {
            value["options"]
                .as_array()
                .map_or(std::slice::from_ref(value), Vec::as_slice)
        })
}

fn label(value: &Value, fallback: &str, max: usize) -> String {
    value
        .as_str()
        .filter(|text| !text.trim().is_empty() && !text.chars().any(char::is_control))
        .unwrap_or(fallback)
        .chars()
        .take(max)
        .collect()
}

fn host_tool(job: &Job, call: &Value) -> Option<String> {
    let server = job.request.host.client_id;
    let prefix = format!("mcp_{server}_");
    let name = call["name"].as_str()?.strip_prefix(&prefix)?;
    job.request
        .tools
        .supports_tool(name)
        .then(|| name.to_owned())
}

fn observe(job: &Job, update: &Value, calls: &mut HashSet<String>) {
    if update["content"]["type"] == "text"
        && let Some(text) = update["content"]["text"].as_str()
    {
        match update["sessionUpdate"].as_str() {
            Some("agent_message_chunk") => job.emit(AiEvent::TextDelta { text: text.into() }),
            Some("agent_thought_chunk") => job.emit(AiEvent::ReasoningDelta {
                text: text.into(),
                summary: false,
            }),
            _ => {}
        }
    }
    if !job.design_tools() {
        return;
    }
    let Some(id) = update["toolCallId"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 256)
    else {
        return;
    };
    if update["sessionUpdate"] == "tool_call"
        && let Some(name) = host_tool(job, update)
        && calls.insert(id.into())
    {
        job.emit(AiEvent::ToolCall {
            id: id.into(),
            name,
            arguments: json!({}),
        });
    }
    let status = update["status"].as_str();
    if matches!(status, Some("completed" | "failed")) && calls.remove(id) {
        job.emit(AiEvent::ToolResult {
            id: id.into(),
            result: Value::Null,
            is_error: status == Some("failed"),
        });
    }
}
