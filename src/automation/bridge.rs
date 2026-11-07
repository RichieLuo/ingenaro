use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use super::ToolCall;
use super::reference_files::{self, LocalReferences, ReferenceFile};
use super::schema::{self, ToolSet};
use serde_json::Value;

pub(super) const CALL_TIMEOUT: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(20);

/// The GUI must check `cancelled` and verify `scope` immediately before any
/// operation/commit. A late reply after cancellation must never commit edits.
pub struct ToolRequest {
    pub scope: String,
    pub call: ToolCall,
    pub cancelled: Arc<AtomicBool>,
    pub reply: mpsc::Sender<Result<Value, String>>,
    task_cancelled: Arc<AtomicBool>,
    host_cancelled: Arc<AtomicBool>,
    deadline: Option<Instant>,
}

impl ToolRequest {
    /// Proves this request belongs to the current built-in task capability, not an external client.
    pub fn same_task(&self, cancelled: &Arc<AtomicBool>) -> bool {
        Arc::ptr_eq(&self.task_cancelled, cancelled)
    }
    /// Check immediately before execution AND immediately before committing.
    /// Includes the scoped task flag synchronously, without a polling delay.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
            || self.task_cancelled.load(Ordering::Acquire)
            || self.host_cancelled.load(Ordering::Acquire)
            || self
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
    }
}

#[derive(Clone)]
pub struct ToolClient {
    sender: mpsc::SyncSender<ToolRequest>,
    scope: String,
    cancelled: Arc<AtomicBool>,
    unbounded: bool,
    tool_set: ToolSet,
    reference_files: Arc<[ReferenceFile]>,
    local_references: LocalReferences,
}

pub struct ToolHost {
    receiver: mpsc::Receiver<ToolRequest>,
}

impl ToolHost {
    #[allow(clippy::new_ret_no_self)] // Public paired host/client constructor contract.
    pub fn new() -> (Self, ToolClient) {
        let (sender, receiver) = mpsc::sync_channel(64);
        (
            Self { receiver },
            ToolClient {
                sender,
                scope: String::new(),
                cancelled: Arc::new(AtomicBool::new(false)),
                unbounded: false,
                tool_set: ToolSet::General,
                reference_files: Arc::default(),
                local_references: LocalReferences::default(),
            },
        )
    }

    /// Nonblocking; cancelled queued requests are returned so the host can
    /// reject them without inspecting or executing their tool payload.
    pub fn try_recv(&self) -> Option<ToolRequest> {
        self.receiver.try_recv().ok()
    }
}

impl ToolClient {
    pub fn scoped(&self, scope: String, cancelled: Arc<AtomicBool>) -> Self {
        Self {
            sender: self.sender.clone(),
            scope,
            cancelled,
            unbounded: self.unbounded,
            tool_set: self.tool_set,
            reference_files: self.reference_files.clone(),
            local_references: self.local_references.clone(),
        }
    }

    pub fn with_parallel_design(mut self) -> Self {
        self.tool_set = ToolSet::Design;
        self
    }

    pub fn with_reference_files(mut self, files: Vec<ReferenceFile>) -> Self {
        self.reference_files = files.into();
        self
    }

    pub fn with_reference_prompt(mut self, prompt: &str) -> Self {
        self.local_references = LocalReferences::new(prompt);
        self
    }

    pub(crate) fn tool_definitions(&self) -> Value {
        schema::definitions(self.tool_set)
    }

    pub(crate) fn supports_tool(&self, name: &str) -> bool {
        self.tool_set.supports(name)
    }

    /// Internal agent tasks wait until the user resumes/cancels; external clients retain bounds.
    pub fn until_cancelled(mut self) -> Self {
        self.unbounded = true;
        self
    }

    pub fn call(&self, call: ToolCall) -> Result<Value, String> {
        self.call_until(call, &Arc::new(AtomicBool::new(false)), CALL_TIMEOUT)
    }

    pub(super) fn call_until(
        &self,
        call: ToolCall,
        stopped: &Arc<AtomicBool>,
        timeout: Duration,
    ) -> Result<Value, String> {
        self.call_interruptible(call, stopped, &Arc::new(AtomicBool::new(false)), timeout)
    }

    pub(super) fn call_interruptible(
        &self,
        call: ToolCall,
        stopped: &Arc<AtomicBool>,
        interrupted: &Arc<AtomicBool>,
        timeout: Duration,
    ) -> Result<Value, String> {
        if self.cancelled.load(Ordering::Acquire)
            || stopped.load(Ordering::Acquire)
            || interrupted.load(Ordering::Acquire)
        {
            return Err("Tool request cancelled".into());
        }
        let start = Instant::now();
        if call.name == reference_files::READ_TOOL {
            if !self.supports_tool(&call.name) {
                return Err("Unknown tool".into());
            }
            let result = reference_files::read(
                &self.reference_files,
                &self.local_references,
                &call.arguments,
            );
            self.check_wait(stopped, interrupted, start, timeout)?;
            return result;
        }
        let cancelled = interrupted.clone();
        // Cancels queued and already received requests on every return path.
        struct Cancel(Arc<AtomicBool>);
        impl Drop for Cancel {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let _cancel = Cancel(cancelled.clone());
        let (reply, receiver) = mpsc::channel();
        let mut request = ToolRequest {
            scope: self.scope.clone(),
            call,
            cancelled: cancelled.clone(),
            reply,
            task_cancelled: self.cancelled.clone(),
            host_cancelled: stopped.clone(),
            deadline: (!self.unbounded).then(|| start + timeout),
        };
        loop {
            self.check_wait(stopped, interrupted, start, timeout)?;
            match self.sender.try_send(request) {
                Ok(()) => break,
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    return Err("Live GUI tool host disconnected".into());
                }
                Err(mpsc::TrySendError::Full(returned)) => {
                    request = returned;
                    std::thread::park_timeout(if self.unbounded {
                        POLL
                    } else {
                        POLL.min(timeout.saturating_sub(start.elapsed()))
                    });
                }
            }
        }
        loop {
            self.check_wait(stopped, interrupted, start, timeout)?;
            match receiver.recv_timeout(if self.unbounded {
                POLL
            } else {
                POLL.min(timeout.saturating_sub(start.elapsed()))
            }) {
                Ok(reply) => {
                    self.check_wait(stopped, interrupted, start, timeout)?;
                    return reply;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("Live GUI host dropped the tool reply".into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }

    fn check_wait(
        &self,
        stopped: &Arc<AtomicBool>,
        interrupted: &AtomicBool,
        start: Instant,
        timeout: Duration,
    ) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire)
            || stopped.load(Ordering::Acquire)
            || interrupted.load(Ordering::Acquire)
        {
            Err("Tool request cancelled".into())
        } else if !self.unbounded && start.elapsed() >= timeout {
            Err("Live GUI tool request timed out".into())
        } else {
            Ok(())
        }
    }
}
