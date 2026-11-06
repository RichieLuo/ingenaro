use super::*;
use crate::ai::{SubagentState, page_agent_label};

pub(super) const MCP_NAME: &str = "rpshop_live";
pub(super) const MCP_TOKEN_ENV: &str = "RPSHOP_NATIVE_MCP_BEARER";

#[derive(Default)]
pub(super) struct Observer {
    agents: HashMap<String, (AgentLabel, SubagentState)>,
}

#[derive(Default)]
struct AgentLabel {
    page_id: Option<u64>,
    task_name: Option<String>,
    title: String,
}

impl Observer {
    pub(super) fn observe(&mut self, job: &Job, root: &str, method: &str, params: &Value) {
        let item = &params["item"];
        if method == "item/started"
            && params["threadId"] == root
            && item["type"] == "subAgentActivity"
            && item["kind"] == "started"
            && let Some(id) = item["agentThreadId"].as_str()
            && let Some(task_name) = item["agentPath"]
                .as_str()
                .and_then(|path| path.strip_prefix("/root/"))
                .filter(|name| !name.is_empty() && !name.contains('/'))
        {
            self.update(
                job,
                id,
                Some(AgentLabel {
                    task_name: Some(task_name.into()),
                    ..Default::default()
                }),
                SubagentState::Starting,
            );
            return;
        }
        if matches!(method, "item/started" | "item/completed")
            && params["threadId"] == root
            && item["type"] == "collabAgentToolCall"
            && item["senderThreadId"] == root
        {
            let spawning = item["tool"] == "spawnAgent";
            let caption = item["prompt"].as_str().filter(|_| spawning);
            for id in item["receiverThreadIds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if !spawning && !self.agents.contains_key(id) {
                    continue;
                }
                let state = match item["agentsStates"][id]["status"].as_str() {
                    Some("running") => SubagentState::Running,
                    Some("completed") => SubagentState::Completed,
                    Some("errored" | "notFound") => SubagentState::Failed,
                    Some("interrupted" | "shutdown") => SubagentState::Stopped,
                    _ if spawning => SubagentState::Starting,
                    _ => continue,
                };
                let label = caption.map(|text| {
                    let (page_id, title) = page_agent_label(text);
                    AgentLabel {
                        page_id,
                        title,
                        task_name: None,
                    }
                });
                self.update(job, id, label, state);
            }
            return;
        }
        let Some(id) = params["threadId"]
            .as_str()
            .filter(|id| self.agents.contains_key(*id))
        else {
            return;
        };
        match method {
            "turn/started" => self.update(job, id, None, SubagentState::Running),
            "turn/completed" => {
                let state = match params["turn"]["status"].as_str() {
                    Some("completed") => SubagentState::Completed,
                    Some("failed") => SubagentState::Failed,
                    Some("interrupted") => SubagentState::Stopped,
                    _ => return,
                };
                self.update(job, id, None, state);
            }
            _ => {}
        }
    }

    fn update(&mut self, job: &Job, id: &str, label: Option<AgentLabel>, state: SubagentState) {
        if self.agents.len() >= 128 && !self.agents.contains_key(id) {
            return;
        }
        let agent = self
            .agents
            .entry(id.into())
            .or_insert_with(|| (AgentLabel::default(), SubagentState::Starting));
        if !matches!(agent.1, SubagentState::Starting | SubagentState::Running)
            || agent.1 == SubagentState::Running && state == SubagentState::Starting
        {
            return;
        }
        if let Some(label) = label {
            agent.0 = label;
        }
        agent.1 = state;
        job.emit(AiEvent::Subagent {
            id: id.into(),
            page_id: agent.0.page_id,
            task_name: agent.0.task_name.clone(),
            title: agent.0.title.clone(),
            state,
        });
    }
}
