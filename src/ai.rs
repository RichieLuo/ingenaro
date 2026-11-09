//! Ingenaro's adapter for the bundled AI runtime.
mod config;
mod credentials;
#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod real_engine_tests;

pub use ai_design_runtime::{
    AI_PLUGIN_PROTOCOL, AI_PLUGIN_VERSION, AiEvent, ComposerPreferences, Diagnostics, EngineConfig,
    EngineKind, ImageAttachment, LocalEngineInfo, MAX_IMAGE_ATTACHMENTS, MAX_IMAGE_BYTES,
    MAX_IMAGE_EDGE, MAX_IMAGE_PIXELS, MAX_IMAGE_SOURCE_BYTES, MAX_TOTAL_IMAGE_BYTES, McpEndpoint,
    ModelCatalog, ModelInfo, PermissionMode, ReasoningEffort, ReasoningPreference, RunningSession,
    SessionBinding, SessionControl, SubagentState, builtin_models, discover_local_engines,
    discover_models, is_local, local_defaults, validate_image_attachments,
};
pub use config::AiSettings;
pub use credentials::{CredentialStore, SessionKeys};

use crate::automation::{self, ToolClient};
use ai_design_runtime::{HostProfile, NativeDesign};
use std::path::PathBuf;
use std::sync::{Arc, atomic::AtomicBool};

pub fn is_local_tool(name: &str) -> bool {
    automation::is_design_tool(name)
}

pub fn is_desktop_engine(kind: EngineKind) -> bool {
    is_local(kind) || kind == EngineKind::Plugin
}

pub const TOOL_PROTOCOL_REVISION: u32 = 12;

const HOST: HostProfile = HostProfile {
    client_id: "ingenaro",
    name: "Ingenaro",
    version: env!("CARGO_PKG_VERSION"),
    tool_revision: TOOL_PROTOCOL_REVISION,
    instructions: include_str!("../.agents/skills/rpshop-automation/SKILL.md"),
    claude_mcp_name: "rpshop",
    image_result_field: "_rpshop_image",
    native_design: Some(NativeDesign {
        codex_mcp_name: "rpshop_live",
        codex_mcp_token_env: "RPSHOP_NATIVE_MCP_BEARER",
        codex_instructions: "Use Ingenaro tools for the live prototype. Put all independent page assignments in a single plan and spawn every assigned native subagent before calling wait. Do single-group and dependent work directly. Children use the rpshop_live MCP server and edit only assigned pages. Report their actual results in the parent conversation.",
        claude_coordinator: "rpshop-coordinator",
        claude_coordinator_prompt: "Design the open prototype using the Ingenaro tools. For independent pages, prepare one plan for the whole turn and call the rpshop-page Agent once per assignment in the same assistant message, with run_in_background set to true. Launch all groups before waiting and wait for every completion notification before replying. Do single-group and dependent work directly. Summarize the actual results in this conversation.",
        claude_page_agent: "rpshop-page",
        claude_page_prompt: "Work on the page IDs listed in your assignment. Read each page explicitly and edit it with rpshop_edit_assigned_page using its page_id. Apply complete batches of real editable widgets. Follow the shared design rules, leave other pages alone, and report what was applied.",
        claude_page_tools: &[
            "rpshop_capabilities",
            "rpshop_read_pages",
            "rpshop_read_layers",
            "rpshop_read_widgets",
            "rpshop_read_styles",
            "rpshop_read_interactions",
            "rpshop_read_library",
            automation::EDIT_PAGE_TOOL,
        ],
        detailed_error_tools: &[automation::DESIGN_PAGES_TOOL, automation::EDIT_PAGE_TOOL],
    }),
};

pub struct StartRequest {
    pub config: EngineConfig,
    pub key: Option<String>,
    pub prompt: String,
    pub context: String,
    pub images: Vec<ImageAttachment>,
    pub working_dir: PathBuf,
    pub scope: String,
    pub resume: Option<SessionBinding>,
    pub tools: ToolClient,
    pub mcp: Option<McpEndpoint>,
    pub cancelled: Arc<AtomicBool>,
}

impl From<StartRequest> for ai_design_runtime::StartRequest {
    fn from(request: StartRequest) -> Self {
        let mut tools = request
            .tools
            .scoped(request.scope.clone(), request.cancelled.clone());
        if is_desktop_engine(request.config.kind) {
            tools = tools.until_cancelled();
        }
        Self {
            host: &HOST,
            config: request.config,
            key: request.key,
            prompt: request.prompt,
            context: request.context,
            images: request.images,
            working_dir: request.working_dir,
            scope: request.scope,
            resume: request.resume,
            tools: Arc::new(tools),
            mcp: request.mcp,
            cancelled: request.cancelled,
        }
    }
}

pub struct AiRuntime;

impl AiRuntime {
    pub fn start(request: StartRequest) -> Result<RunningSession, String> {
        ai_design_runtime::AiRuntime::start(request.into())
    }

    pub fn start_with_subagents(request: StartRequest) -> Result<RunningSession, String> {
        ai_design_runtime::AiRuntime::start_with_subagents(request.into())
    }

    pub fn start_palette(
        request: StartRequest,
        web_search: bool,
    ) -> Result<RunningSession, String> {
        ai_design_runtime::AiRuntime::start_palette(request.into(), web_search)
    }

    pub fn start_title(request: StartRequest) -> Result<RunningSession, String> {
        ai_design_runtime::AiRuntime::start_title(request.into())
    }
}
