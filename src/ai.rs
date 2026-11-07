//! App-scoped AI configuration and cancellable, permission-aware engine adapters.
//! Credentials never belong to serializable settings or project documents.
mod api;
mod attachments;
pub use attachments::{
    ImageAttachment, MAX_IMAGE_ATTACHMENTS, MAX_IMAGE_BYTES, MAX_IMAGE_EDGE, MAX_IMAGE_PIXELS,
    MAX_IMAGE_SOURCE_BYTES, MAX_TOTAL_IMAGE_BYTES, validate_image_attachments,
};
mod catalog;
pub use catalog::{
    LocalEngineInfo, ModelCatalog, ModelInfo, builtin_models, discover_local_engines,
    discover_models, is_local, local_defaults,
};
mod claude;
mod codex;
mod config;
mod credentials;
mod process;
#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod real_engine_tests;
mod runtime;
mod steering;

pub use config::{AiSettings, EngineConfig, EngineKind, ReasoningEffort};
pub use credentials::{CredentialStore, SessionKeys};
pub use runtime::{AiRuntime, RunningSession, SessionControl};

use crate::automation::ToolClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, atomic::AtomicBool};

/// Registered RPShop capabilities; used for host-gated CLI permission delegation.
pub fn is_local_tool(name: &str) -> bool {
    crate::automation::is_design_tool(name)
}

pub const TOOL_PROTOCOL_REVISION: u32 = 9;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionBinding {
    pub session_id: String,
    pub engine_id: String,
    /// Stable application project identity, not a filename or per-turn ID.
    pub scope: String,
    #[serde(default)]
    pub tool_revision: u32,
}

/// Ephemeral RPShop MCP bridge credentials, never persisted in engine settings.
#[derive(Clone)]
pub struct McpEndpoint {
    pub endpoint: String,
    pub token: String,
}

pub struct StartRequest {
    pub config: EngineConfig,
    pub key: Option<String>,
    pub prompt: String,
    pub context: String,
    /// Validated memory-only image inputs, separate from prompt text and projects.
    pub images: Vec<ImageAttachment>,
    pub working_dir: PathBuf,
    pub scope: String,
    pub resume: Option<SessionBinding>,
    pub tools: ToolClient,
    pub mcp: Option<McpEndpoint>,
    /// A fresh per-turn flag, also shared with the host's automation queue.
    pub cancelled: Arc<AtomicBool>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubagentState {
    Starting,
    Running,
    Completed,
    Failed,
    Stopped,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum AiEvent {
    TextDelta {
        text: String,
    },
    /// Public reasoning summaries/progress emitted by the CLI, never raw reasoning.
    ReasoningDelta {
        text: String,
    },
    State {
        state: String,
    },
    /// Observed search activity from a palette-only turn.
    PaletteSearch {
        query: Option<String>,
        url: Option<String>,
    },
    Session {
        binding: SessionBinding,
    },
    Subagent {
        id: String,
        page_id: Option<u64>,
        task_name: Option<String>,
        title: String,
        state: SubagentState,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        id: String,
        result: Value,
        is_error: bool,
    },
    ApprovalRequired {
        id: String,
        tool: String,
        arguments: Value,
    },
    Error {
        message: String,
    },
    Complete {
        cancelled: bool,
    },
}

fn page_agent_label(text: &str) -> (Option<u64>, String) {
    let first = text.lines().next().unwrap_or_default().trim();
    let (page_id, title) = first
        .strip_prefix("page_id=")
        .and_then(|value| {
            let end = value.find(char::is_whitespace).unwrap_or(value.len());
            value[..end]
                .parse()
                .ok()
                .map(|id| (Some(id), value[end..].trim()))
        })
        .unwrap_or((None, first));
    (
        page_id,
        title.chars().filter(|c| !c.is_control()).take(72).collect(),
    )
}

/// Reconcile streamed public text with a final CLI snapshot without replaying it.
/// Frames are bounded, so retaining more than one frame of a block cannot help.
#[derive(Default)]
pub(super) struct PublicText {
    prefix: Option<String>,
    started: bool,
}
impl PublicText {
    fn record_delta(&mut self, text: &str) {
        if !self.started {
            self.prefix = Some(String::new());
            self.started = true;
        }
        if let Some(prefix) = &mut self.prefix {
            if prefix.len().saturating_add(text.len()) <= process::MAX_FRAME {
                prefix.push_str(text);
            } else {
                self.prefix = None;
            }
        }
    }
    fn snapshot<'a>(&mut self, text: &'a str) -> Option<&'a str> {
        let offset = if self.started {
            let prefix = self.prefix.as_deref()?;
            if !text.starts_with(prefix) {
                return None;
            }
            prefix.len()
        } else {
            0
        };
        let tail = text.get(offset..)?;
        self.record_delta(tail);
        (!tail.is_empty()).then_some(tail)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostics {
    pub available: bool,
    pub version: Option<String>,
    /// Login is deliberately not inferred by reading CLI credential files.
    pub login_hint: String,
    pub action_hint: String,
}
