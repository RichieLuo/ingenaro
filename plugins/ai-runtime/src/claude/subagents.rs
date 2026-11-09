use super::*;
use crate::{NativeDesign, SubagentState, page_agent_label};

pub(super) fn definitions(job: &Job, native: &NativeDesign) -> Value {
    let server = job.request.host.claude_mcp_name;
    let mut coordinator = vec![format!("Agent({})", native.claude_page_agent)];
    coordinator.extend(
        job.request
            .tools
            .tool_definitions()
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| {
                tool["name"]
                    .as_str()
                    .map(|name| format!("mcp__{server}__{name}"))
            }),
    );
    let page_tools: Vec<_> = native
        .claude_page_tools
        .iter()
        .map(|name| format!("mcp__{server}__{name}"))
        .collect();
    json!({
        (native.claude_coordinator): {
            "description":"Coordinates live page design in the current conversation.",
            "prompt":native.claude_coordinator_prompt,
            "model":"inherit", "tools":coordinator
        },
        (native.claude_page_agent): {
            "description":"Designs the native pages assigned by the coordinator.",
            "prompt":native.claude_page_prompt,
            "model":"inherit", "background":true, "tools":page_tools, "mcpServers":[server]
        }
    })
}

pub(super) fn allowed_call(native: &NativeDesign, name: &str, arguments: &Value) -> bool {
    matches!(name, "Agent" | "Task")
        && arguments["subagent_type"] == native.claude_page_agent
        && arguments["run_in_background"] != false
        && arguments["isolation"].is_null()
        && arguments["team_name"].is_null()
}

#[derive(Default)]
pub(super) struct Observer {
    agents: HashMap<String, Agent>,
    task_to_agent: HashMap<String, String>,
}

struct Agent {
    page_id: Option<u64>,
    title: String,
    state: SubagentState,
}

impl Observer {
    pub(super) fn observe(&mut self, job: &Job, native: &NativeDesign, frame: &Value) {
        if let Some(parent) = frame["parent_tool_use_id"].as_str() {
            self.update(job, parent, SubagentState::Running);
            return;
        }
        match (frame["type"].as_str(), frame["subtype"].as_str()) {
            (Some("assistant"), _) => {
                for block in frame["message"]["content"].as_array().into_iter().flatten() {
                    let Some(id) = block["id"].as_str() else {
                        continue;
                    };
                    let input = &block["input"];
                    if block["type"] != "tool_use"
                        || !allowed_call(native, block["name"].as_str().unwrap_or_default(), input)
                        || self.agents.contains_key(id)
                        || self.agents.len() >= 128
                    {
                        continue;
                    }
                    let (page_id, title) =
                        page_agent_label(input["description"].as_str().unwrap_or_default());
                    let page_id = page_id.or_else(|| {
                        page_agent_label(input["prompt"].as_str().unwrap_or_default()).0
                    });
                    let agent = Agent {
                        page_id,
                        title,
                        state: SubagentState::Starting,
                    };
                    agent.emit(job, id);
                    self.agents.insert(id.into(), agent);
                }
            }
            (Some("system"), Some("task_started" | "task_progress")) => {
                if let Some(id) = frame["tool_use_id"].as_str()
                    && self.agents.contains_key(id)
                {
                    if let Some(task) = frame["task_id"].as_str() {
                        self.task_to_agent.insert(task.into(), id.into());
                    }
                    self.update(job, id, SubagentState::Running);
                }
            }
            (Some("system"), Some("task_notification")) => {
                let id = frame["tool_use_id"]
                    .as_str()
                    .or_else(|| {
                        self.task_to_agent
                            .get(frame["task_id"].as_str()?)
                            .map(String::as_str)
                    })
                    .map(str::to_owned);
                if let Some(id) = id {
                    let state = match frame["status"].as_str() {
                        Some("completed") => SubagentState::Completed,
                        Some("failed") => SubagentState::Failed,
                        Some("stopped") => SubagentState::Stopped,
                        _ => return,
                    };
                    self.update(job, &id, state);
                }
            }
            (Some("user"), _) => {
                for block in frame["message"]["content"].as_array().into_iter().flatten() {
                    let Some(id) = block["tool_use_id"].as_str() else {
                        continue;
                    };
                    if block["type"] != "tool_result" {
                        continue;
                    }
                    if block["is_error"] == true {
                        self.update(job, id, SubagentState::Failed);
                    }
                }
            }
            _ => {}
        }
    }

    fn update(&mut self, job: &Job, id: &str, state: SubagentState) {
        if let Some(agent) = self.agents.get_mut(id)
            && matches!(
                agent.state,
                SubagentState::Starting | SubagentState::Running
            )
            && agent.state != state
        {
            agent.state = state;
            agent.emit(job, id);
        }
    }
}

impl Agent {
    fn emit(&self, job: &Job, id: &str) {
        job.emit(AiEvent::Subagent {
            id: id.into(),
            page_id: self.page_id,
            task_name: None,
            title: self.title.clone(),
            state: self.state,
        });
    }
}
