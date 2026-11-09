use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::AtomicBool};

/// One call against the document owned by the host.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    #[serde(default = "empty_object")]
    pub arguments: Value,
}

fn empty_object() -> Value {
    json!({})
}

/// A per-turn tool capability. The host is responsible for project identity,
/// approvals and checking cancellation before executing or committing a call.
pub trait ToolClient: Send + Sync + 'static {
    /// MCP-style definitions with name, description and inputSchema.
    fn tool_definitions(&self) -> Value;
    fn supports_tool(&self, name: &str) -> bool;
    /// The per-call flag can expire before the turn finishes, for example on timeout.
    fn call(&self, call: ToolCall, interrupted: &Arc<AtomicBool>) -> Result<Value, String>;
}

/// Product identity and design instructions supplied by the editor.
pub struct HostProfile {
    pub client_id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub tool_revision: u32,
    pub instructions: &'static str,
    pub claude_mcp_name: &'static str,
    pub image_result_field: &'static str,
    pub native_design: Option<NativeDesign>,
}

/// Optional native page delegation for hosts that provide a scoped MCP server.
pub struct NativeDesign {
    pub codex_mcp_name: &'static str,
    pub codex_mcp_token_env: &'static str,
    pub codex_instructions: &'static str,
    pub claude_coordinator: &'static str,
    pub claude_coordinator_prompt: &'static str,
    pub claude_page_agent: &'static str,
    pub claude_page_prompt: &'static str,
    pub claude_page_tools: &'static [&'static str],
    pub detailed_error_tools: &'static [&'static str],
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn definitions() -> Value {
        json!([{"name":"read_document","description":"Read the document","inputSchema":{"type":"object"}}])
    }

    pub struct TestTools;
    impl ToolClient for TestTools {
        fn tool_definitions(&self) -> Value {
            definitions()
        }
        fn supports_tool(&self, name: &str) -> bool {
            name == "read_document"
        }
        fn call(&self, _call: ToolCall, _interrupted: &Arc<AtomicBool>) -> Result<Value, String> {
            Ok(json!({}))
        }
    }
}
