use super::{AiEvent, EngineKind, SessionBinding, StartRequest};
use crate::automation::{ToolCall, ToolClient};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::time::{Duration, Instant};

struct Shared {
    cancelled: Arc<AtomicBool>,
    finished: AtomicBool,
    approvals: Mutex<HashMap<String, SyncSender<bool>>>,
    interrupt: Mutex<Option<(SyncSender<Value>, Value)>>,
    steering: Mutex<super::steering::Steering>,
}
#[derive(Clone)]
pub struct SessionControl {
    shared: Arc<Shared>,
}
impl SessionControl {
    pub fn steer(
        &self,
        text: String,
        images: &[super::ImageAttachment],
    ) -> Result<Receiver<Result<(), String>>, String> {
        if self.is_finished() || self.shared.cancelled.load(Ordering::Acquire) {
            return Err("任务已经结束，未发送引导".into());
        }
        super::steering::send(&self.shared.steering, text, images)
    }
    pub fn cancel(&self) {
        self.shared.cancelled.store(true, Ordering::Release);
        if let Ok(interrupt) = self.shared.interrupt.lock()
            && let Some((writer, frame)) = &*interrupt
        {
            let _ = writer.try_send(frame.clone());
        }
        if let Ok(mut approvals) = self.shared.approvals.lock() {
            approvals.clear();
        }
    }
    pub fn cancelled(&self) -> Arc<AtomicBool> {
        self.shared.cancelled.clone()
    }
    pub fn is_finished(&self) -> bool {
        self.shared.finished.load(Ordering::Acquire)
    }
    pub fn approve(&self, id: impl Into<String>, allowed: bool) -> Result<(), String> {
        if self.shared.cancelled.load(Ordering::Acquire)
            || self.shared.finished.load(Ordering::Acquire)
        {
            return Err("AI session is no longer active".into());
        }
        let sender = self
            .shared
            .approvals
            .lock()
            .map_err(|_| "Approval channel unavailable")?
            .remove(&id.into())
            .ok_or("Approval is no longer pending")?;
        sender
            .try_send(allowed)
            .map_err(|_| "Approval is no longer pending".into())
    }
}
pub struct RunningSession {
    pub events: Receiver<AiEvent>,
    pub control: SessionControl,
}
impl Drop for RunningSession {
    fn drop(&mut self) {
        if !self.control.shared.finished.load(Ordering::Acquire) {
            self.control.cancel();
        }
    }
}

pub struct AiRuntime;
#[derive(Clone, Copy)]
enum Purpose {
    Design,
    NativeDesign,
    Palette { web_search: bool },
}

impl AiRuntime {
    pub fn start(request: StartRequest) -> Result<RunningSession, String> {
        Self::start_job(request, Purpose::Design)
    }

    pub fn start_with_subagents(request: StartRequest) -> Result<RunningSession, String> {
        if !super::is_local(request.config.kind) {
            return Err("原生子代理需要本地智能体".into());
        }
        Self::start_job(request, Purpose::NativeDesign)
    }

    pub fn start_palette(
        mut request: StartRequest,
        web_search: bool,
    ) -> Result<RunningSession, String> {
        if !super::is_local(request.config.kind) {
            return Err("请选择本地智能体生成配色".into());
        }
        request.resume = None;
        request.mcp = None;
        Self::start_job(request, Purpose::Palette { web_search })
    }

    fn start_job(mut request: StartRequest, purpose: Purpose) -> Result<RunningSession, String> {
        request.config.validate()?;
        if request.scope.is_empty() || request.scope.len() > 4096 {
            return Err("Invalid project scope".into());
        }
        if !request.working_dir.is_dir() {
            return Err("AI working directory does not exist".into());
        }
        if request.prompt.len() + request.context.len() > 2 * 1024 * 1024 {
            return Err("AI prompt and context are too large".into());
        }
        super::validate_image_attachments(&request.images).map_err(|error| error.to_string())?;
        if let Some(resume) = &request.resume
            && (resume.engine_id != request.config.id
                || resume.scope != request.scope
                || resume.session_id.is_empty()
                || resume.session_id.len() > 256)
        {
            return Err("Session belongs to another project or engine".into());
        }
        // Codex persists the registered dynamic tool schemas on the thread.
        // Claude reads its MCP tool definitions on each launch.
        if request.config.kind == EngineKind::Codex
            && request
                .resume
                .as_ref()
                .is_some_and(|resume| resume.tool_revision != super::TOOL_PROTOCOL_REVISION)
        {
            request.resume = None;
        }
        if matches!(purpose, Purpose::NativeDesign)
            || (request.config.kind == EngineKind::Claude && matches!(purpose, Purpose::Design))
        {
            let mcp = request
                .mcp
                .as_ref()
                .ok_or("Design agents require an RPShop loopback MCP bridge")?;
            validate_mcp(&mcp.endpoint, &mcp.token)?;
        }
        request.tools = request
            .tools
            .scoped(request.scope.clone(), request.cancelled.clone());
        if super::is_local(request.config.kind) {
            request.tools = request.tools.until_cancelled();
        }
        let (events_tx, events) = mpsc::sync_channel(256);
        let shared = Arc::new(Shared {
            cancelled: request.cancelled.clone(),
            finished: AtomicBool::new(false),
            approvals: Mutex::new(HashMap::new()),
            interrupt: Mutex::new(None),
            steering: Mutex::new(Default::default()),
        });
        let control = SessionControl {
            shared: shared.clone(),
        };
        std::thread::Builder::new()
            .name("rpshop-ai-runtime".into())
            .spawn(move || {
                let deadline = match purpose {
                    Purpose::Palette { .. } => Some(Instant::now() + Duration::from_secs(180)),
                    Purpose::Design | Purpose::NativeDesign => {
                        (!super::is_local(request.config.kind)
                            && request.config.timeout_seconds > 0)
                            .then(|| {
                                Instant::now() + Duration::from_secs(request.config.timeout_seconds)
                            })
                    }
                };
                let job = Job {
                    request,
                    purpose,
                    events: events_tx,
                    shared,
                    deadline,
                    redactor: Mutex::new(StreamRedactor::new()),
                    reasoning_redactor: Mutex::new(StreamRedactor::new()),
                };
                job.emit(AiEvent::State {
                    state: "starting".into(),
                });
                let result = job.check().and_then(|()| match job.request.config.kind {
                    EngineKind::Codex => super::codex::run(&job),
                    EngineKind::Claude => super::claude::run(&job),
                    EngineKind::OpenAiCompatible => super::api::run(&job),
                });
                if let Err(message) = result
                    && !job.is_cancelled()
                {
                    job.emit(AiEvent::Error { message });
                }
                // Invalidate the shared automation token even on natural completion.
                // A tool request arriving after the turn has ended may not mutate.
                let cancelled = job.is_cancelled();
                job.flush_text();
                if let Ok(mut state) = job.shared.steering.lock() {
                    state.target = None;
                    state.finish(Err("本轮已结束，未确认的引导已保留为草稿".into()));
                }
                job.shared.finished.store(true, Ordering::Release);
                job.shared.cancelled.store(true, Ordering::Release);
                if let Ok(mut approvals) = job.shared.approvals.lock() {
                    approvals.clear();
                }
                // Terminal delivery must not be dropped when the bounded queue is full.
                // Dropping RunningSession disconnects the receiver and unblocks this send.
                let _ = job.events.send(AiEvent::Complete { cancelled });
            })
            .map_err(|_| "Cannot start AI runtime thread")?;
        Ok(RunningSession { events, control })
    }
}
fn validate_mcp(endpoint: &str, token: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(endpoint).map_err(|_| "Invalid MCP endpoint")?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "::1"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || token.is_empty()
        || token.len() > 16384
        || token.contains(['\r', '\n'])
    {
        return Err("MCP bridge must use an authenticated numeric loopback endpoint".into());
    }
    Ok(())
}

pub(super) struct Job {
    pub request: StartRequest,
    purpose: Purpose,
    events: SyncSender<AiEvent>,
    shared: Arc<Shared>,
    pub deadline: Option<Instant>,
    redactor: Mutex<StreamRedactor>,
    reasoning_redactor: Mutex<StreamRedactor>,
}

impl Job {
    pub fn is_palette(&self) -> bool {
        matches!(self.purpose, Purpose::Palette { .. })
    }

    pub fn native_subagents(&self) -> bool {
        matches!(self.purpose, Purpose::NativeDesign)
    }

    pub fn palette_search(&self) -> bool {
        matches!(self.purpose, Purpose::Palette { web_search: true })
    }

    pub fn steer_target(&self, writer: SyncSender<Value>, thread: String, turn: String) {
        if let Ok(mut state) = self.shared.steering.lock() {
            state.target = Some((writer, self.request.config.kind, thread, turn));
        }
    }
    pub fn steer_ack(&self, accepted: bool) {
        if let Ok(mut state) = self.shared.steering.lock() {
            state.finish(if accepted {
                Ok(())
            } else {
                Err("引擎未接受引导，草稿已保留".into())
            });
        }
    }
    pub fn steer_echo(&self, id: Option<&str>) {
        if let Ok(mut state) = self.shared.steering.lock()
            && id.is_some()
            && state.message_id.as_deref() == id
        {
            state.finish(Ok(()));
        }
    }
    pub fn is_cancelled(&self) -> bool {
        self.shared.cancelled.load(Ordering::Acquire)
    }
    pub fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            return Err("AI session cancelled".into());
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err("AI request timed out".into());
        }
        Ok(())
    }
    pub fn emit(&self, event: AiEvent) {
        let secrets = self.secrets();
        let event = match event {
            AiEvent::TextDelta { text } => {
                let text = self
                    .redactor
                    .lock()
                    .map(|mut r| r.push(&text, &secrets))
                    .unwrap_or_default();
                if text.is_empty() {
                    return;
                }
                AiEvent::TextDelta { text }
            }
            AiEvent::ReasoningDelta { text } => {
                let text = self
                    .reasoning_redactor
                    .lock()
                    .map(|mut r| r.push(&text, &secrets))
                    .unwrap_or_default();
                if text.is_empty() {
                    return;
                }
                AiEvent::ReasoningDelta { text }
            }
            other => {
                let Ok(mut value) = serde_json::to_value(other) else {
                    return;
                };
                redact_value(&mut value, &secrets);
                let Ok(event) = serde_json::from_value(value) else {
                    return;
                };
                event
            }
        };
        self.send_event(event);
    }
    fn secrets(&self) -> Vec<&str> {
        let mut secrets = Vec::new();
        if let Some(key) = self.request.key.as_deref().filter(|s| !s.is_empty()) {
            secrets.push(key);
        }
        if let Some(mcp) = &self.request.mcp
            && !mcp.token.is_empty()
        {
            secrets.push(mcp.token.as_str());
        }
        secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
        secrets
    }
    fn flush_text(&self) {
        let text = self
            .redactor
            .lock()
            .map(|mut r| r.finish())
            .unwrap_or_default();
        if !text.is_empty() {
            self.send_event(AiEvent::TextDelta { text });
        }
        let text = self
            .reasoning_redactor
            .lock()
            .map(|mut r| r.finish())
            .unwrap_or_default();
        if !text.is_empty() {
            self.send_event(AiEvent::ReasoningDelta { text });
        }
    }
    fn send_event(&self, mut event: AiEvent) {
        loop {
            match self.events.try_send(event) {
                Ok(()) => return,
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    self.shared.cancelled.store(true, Ordering::Release);
                    return;
                }
                Err(mpsc::TrySendError::Full(value)) => {
                    if self.check().is_err() {
                        return;
                    }
                    event = value;
                    std::thread::park_timeout(Duration::from_millis(10));
                }
            }
        }
    }
    pub fn bind(&self, session_id: String) {
        if self.is_palette() {
            return;
        }
        self.emit(AiEvent::Session {
            binding: SessionBinding {
                session_id,
                engine_id: self.request.config.id.clone(),
                scope: self.request.scope.clone(),
                tool_revision: super::TOOL_PROTOCOL_REVISION,
            },
        });
    }
    pub fn interrupt(&self, writer: SyncSender<Value>, frame: Value) {
        if let Ok(mut value) = self.shared.interrupt.lock() {
            *value = Some((writer, frame));
        }
    }
    pub fn arguments_contain_secret(&self, arguments: &Value) -> bool {
        contains_secret(arguments, &self.secrets())
    }
    pub fn permission(&self, id: String, tool: String, arguments: Value) -> Result<bool, String> {
        self.check()?;
        if self.arguments_contain_secret(&arguments) {
            return Ok(false);
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.shared
            .approvals
            .lock()
            .map_err(|_| "Approval channel unavailable")?
            .insert(id.clone(), tx);
        self.emit(AiEvent::ApprovalRequired {
            id: id.clone(),
            tool,
            arguments,
        });
        let result = loop {
            if let Err(error) = self.check() {
                break Err(error);
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(allowed) => break Ok(allowed),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break Ok(false),
            }
        };
        if let Ok(mut approvals) = self.shared.approvals.lock() {
            approvals.remove(&id);
        }
        result
    }
    pub fn call_tool(
        &self,
        id: String,
        name: String,
        arguments: Value,
    ) -> Result<(Value, bool), String> {
        self.check()?;
        if self.is_palette() {
            return Ok((
                json!({"error":"Palette suggestions cannot access the project"}),
                true,
            ));
        }
        if self.arguments_contain_secret(&arguments) {
            return Ok((
                json!({"error":"Tool arguments contained a private credential and were rejected"}),
                true,
            ));
        }
        if !self.request.tools.supports_tool(&name) {
            return Ok((json!({"error":"Unknown RPShop tool"}), true));
        }
        let report_error = self.native_subagents()
            || matches!(
                name.as_str(),
                crate::automation::DESIGN_PAGES_TOOL | crate::automation::EDIT_PAGE_TOOL
            );
        self.emit(AiEvent::ToolCall {
            id: id.clone(),
            name: name.clone(),
            arguments: arguments.clone(),
        });
        let tools: ToolClient = self.request.tools.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let _ = tx.send(tools.call(ToolCall { name, arguments }));
        });
        let result = loop {
            self.check()?;
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(result) => break result,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break Err("Tool worker stopped".into()),
            }
        };
        self.check()?;
        let (value, is_error) = match result {
            Ok(value) => (value, false),
            Err(error) if report_error => {
                let mut value = json!({"error":error});
                redact_value(&mut value, &self.secrets());
                (value, true)
            }
            Err(_) => (json!({"error":"RPShop tool failed or was denied"}), true),
        };
        if serde_json::to_vec(&value).map_or(true, |v| v.len() > super::process::MAX_FRAME / 2) {
            return Err("RPShop tool result exceeded the size limit".into());
        }
        self.emit(AiEvent::ToolResult {
            id,
            result: value.clone(),
            is_error,
        });
        Ok((value, is_error))
    }

    pub fn prompt(&self) -> String {
        if self.is_palette() {
            return format!(
                "{}\n\n用户描述：\n{}",
                self.request.context, self.request.prompt
            );
        }
        format!(
            "You are RPShop's design assistant. Use only RPShop tools to inspect or modify the open prototype. Do not modify project files directly. Respect explicit tool approvals.\n\nProject context:\n{}\n\nUser request:\n{}",
            self.request.context, self.request.prompt
        )
    }
}

/// Hold only a suffix that could become a secret, so split deltas cannot leak it.
struct StreamRedactor {
    pending: String,
}
impl StreamRedactor {
    fn new() -> Self {
        Self {
            pending: String::new(),
        }
    }
    fn push(&mut self, text: &str, secrets: &[&str]) -> String {
        self.pending.push_str(text);
        for secret in secrets {
            self.pending = self.pending.replace(secret, "[REDACTED]");
        }
        let hold = secrets
            .iter()
            .flat_map(|secret| secret.char_indices().skip(1).map(|(n, _)| n))
            .filter(|&n| {
                secrets.iter().any(|s| {
                    n < s.len() && s.is_char_boundary(n) && self.pending.ends_with(&s[..n])
                })
            })
            .max()
            .unwrap_or(0);
        let emit = self.pending.len() - hold;
        let suffix = self.pending.split_off(emit);
        std::mem::replace(&mut self.pending, suffix)
    }
    fn finish(&mut self) -> String {
        // A dangling credential prefix is also private; do not flush it verbatim.
        if self.pending.is_empty() {
            String::new()
        } else {
            self.pending.clear();
            "[REDACTED]".into()
        }
    }
}
fn contains_secret(value: &Value, secrets: &[&str]) -> bool {
    let matches = |text: &str| secrets.iter().any(|s| text.contains(s));
    match value {
        Value::String(text) => matches(text),
        Value::Array(values) => values.iter().any(|v| contains_secret(v, secrets)),
        Value::Object(values) => values
            .iter()
            .any(|(key, value)| matches(key) || contains_secret(value, secrets)),
        _ => false,
    }
}
fn redact_value(value: &mut Value, secrets: &[&str]) {
    match value {
        Value::String(text) => {
            for secret in secrets {
                *text = text.replace(secret, "[REDACTED]");
            }
        }
        Value::Array(values) => {
            for value in values {
                redact_value(value, secrets);
            }
        }
        Value::Object(values) => {
            let original = std::mem::take(values);
            for (mut key, mut value) in original {
                for secret in secrets {
                    key = key.replace(secret, "[REDACTED]");
                }
                redact_value(&mut value, secrets);
                values.insert(key, value);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_in_json_keys_and_values_are_detected_and_redacted() {
        let mut value = json!({"session-secret":{"nested":"session-secret"}});
        assert!(contains_secret(&value, &["session-secret"]));
        redact_value(&mut value, &["session-secret"]);
        assert!(!value.to_string().contains("session-secret"));
        assert_eq!(value["[REDACTED]"]["nested"], "[REDACTED]");
        let mut stream = StreamRedactor::new();
        assert_eq!(stream.push("hello session-", &["session-secret"]), "hello ");
        assert_eq!(
            stream.push("secret done", &["session-secret"]),
            "[REDACTED] done"
        );
    }
    #[test]
    fn mcp_rejects_remote_and_query_tokens() {
        assert!(validate_mcp("http://127.0.0.1:123/mcp", "secret").is_ok());
        assert!(validate_mcp("http://evil.test/mcp", "secret").is_err());
        assert!(validate_mcp("http://127.0.0.1/mcp?token=secret", "secret").is_err());
    }
    #[test]
    fn approval_cannot_arrive_after_cancellation() {
        let shared = Arc::new(Shared {
            cancelled: Arc::new(AtomicBool::new(false)),
            finished: AtomicBool::new(false),
            approvals: Mutex::new(HashMap::new()),
            interrupt: Mutex::new(None),
            steering: Mutex::new(Default::default()),
        });
        let (tx, _rx) = mpsc::sync_channel(1);
        shared.approvals.lock().unwrap().insert("1".into(), tx);
        let control = SessionControl { shared };
        control.cancel();
        assert!(control.cancelled().load(Ordering::Acquire));
        assert!(control.approve("1", true).is_err());
    }
}
