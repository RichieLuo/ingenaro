//! Versioned, per-turn JSON Lines transport for a user-selected engine executable.
use super::process::{Process, plugin_command};
use super::runtime::Job;
use super::{AiEvent, ToolCall};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

pub const AI_PLUGIN_PROTOCOL: &str = "ai-design-engine";
pub const AI_PLUGIN_VERSION: u32 = 1;
const MAX_TOOL_CALLS: usize = 128;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Frame {
    Hello {
        protocol: String,
        version: u32,
    },
    Text {
        text: String,
    },
    ToolCall {
        id: String,
        #[serde(flatten)]
        call: ToolCall,
    },
    Done,
    Error {
        message: String,
    },
}

struct Workspace(PathBuf);

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) fn run(job: &Job) -> Result<(), String> {
    let path = job
        .request
        .config
        .executable
        .as_deref()
        .ok_or("请选择 自定义引擎可执行文件")?;
    let mut command = plugin_command(path)?;
    let workspace = job
        .request
        .working_dir
        .join(format!("plugin-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&workspace).map_err(|_| "无法创建 自定义引擎本轮工作目录")?;
    let workspace = Workspace(workspace);
    let process = Process::spawn(&mut command, &workspace.0)?;
    process
        .send(json!({"type":"hello","protocol":AI_PLUGIN_PROTOCOL,"version":AI_PLUGIN_VERSION}))?;
    let hello_deadline = Instant::now() + Duration::from_secs(5);
    let hello = receive(job, &process, Some(hello_deadline))?;
    if !matches!(hello, Frame::Hello { protocol, version }
        if protocol == AI_PLUGIN_PROTOCOL && version == AI_PLUGIN_VERSION)
    {
        return Err("自定义引擎协议或版本不兼容".into());
    }
    let images: Vec<Value> = job
        .request
        .images
        .iter()
        .map(|image| json!({"name":image.name(),"data_url":image.data_url()}))
        .collect();
    let tools = job.request.tools.tool_definitions();
    process.send(json!({"type":"start","prompt":job.prompt(),"images":images,"tools":tools}))?;
    job.emit(AiEvent::State {
        state: "running".into(),
    });
    let mut calls = HashSet::new();
    loop {
        match receive(job, &process, None)? {
            Frame::Text { text } => job.emit(AiEvent::TextDelta { text }),
            Frame::ToolCall { id, call } => {
                if calls.len() >= MAX_TOOL_CALLS {
                    return Err("自定义引擎超过本轮工具调用上限".into());
                }
                if id.is_empty()
                    || id.len() > 128
                    || !call.arguments.is_object()
                    || !calls.insert(id.clone())
                {
                    return Err("自定义引擎发送了无效的工具调用".into());
                }
                let (result, is_error) = job.call_tool(id.clone(), call.name, call.arguments)?;
                process.send(
                    json!({"type":"tool_result","id":id,"result":result,"is_error":is_error}),
                )?;
            }
            Frame::Done => return Ok(()),
            Frame::Error { message } => return Err(message.chars().take(500).collect()),
            Frame::Hello { .. } => return Err("自定义引擎重复发送了握手消息".into()),
        }
    }
}

fn receive(job: &Job, process: &Process, deadline: Option<Instant>) -> Result<Frame, String> {
    loop {
        job.check()?;
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err("自定义引擎握手超时".into());
        }
        match process.lines.recv_timeout(Duration::from_millis(50)) {
            Ok(Ok(frame)) => {
                return serde_json::from_value(frame).map_err(|_| "自定义引擎消息格式无效".into());
            }
            Ok(Err(_)) | Err(RecvTimeoutError::Disconnected) => {
                return Err("自定义引擎在任务结束前断开或发送了无效数据".into());
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}
