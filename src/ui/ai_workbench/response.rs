use super::*;
use design::{ChatActivity, ChatRole, ChatStep};

const MAX_STREAM_BYTES: usize = 4 * 1024 * 1024;

pub(super) struct Message {
    pub role: ChatRole,
    pub text: String,
    pub steps: Vec<ChatStep>,
}

impl Message {
    pub fn new(role: ChatRole, text: impl Into<String>) -> Self {
        Self {
            role,
            text: text.into(),
            steps: Vec::new(),
        }
    }
}

#[derive(Default)]
pub(super) struct Response {
    pub text: String,
    pub steps: Vec<ChatStep>,
    tools: HashMap<String, usize>,
    thinking_bytes: usize,
}

impl Response {
    pub fn text_delta(&mut self, text: &str) {
        if self.text.len() + text.len() <= MAX_STREAM_BYTES {
            self.text.push_str(text);
        }
    }

    pub fn thinking_delta(&mut self, text: &str, summary: bool) {
        if text.is_empty() || self.thinking_bytes + text.len() > MAX_STREAM_BYTES {
            return;
        }
        self.thinking_bytes += text.len();
        let after_text = self.text.len();
        if let Some(ChatStep {
            after_text: after,
            activity:
                ChatActivity::Thinking {
                    text: block,
                    summary: is_summary,
                },
        }) = self.steps.last_mut()
            && *after == after_text
            && *is_summary == summary
        {
            block.push_str(text);
        } else {
            self.steps.push(ChatStep {
                after_text,
                activity: ChatActivity::Thinking {
                    text: text.into(),
                    summary,
                },
            });
        }
    }

    pub fn tool_call(&mut self, id: String, name: String) {
        if self.tools.contains_key(&id) {
            return;
        }
        self.tools.insert(id, self.steps.len());
        self.steps.push(ChatStep {
            after_text: self.text.len(),
            activity: ChatActivity::Tool {
                name,
                outcome: None,
            },
        });
    }

    pub fn tool_result(&mut self, id: &str, succeeded: bool) {
        if let Some(&index) = self.tools.get(id)
            && let ChatActivity::Tool { outcome, .. } = &mut self.steps[index].activity
        {
            *outcome = Some(succeeded);
        }
    }
}

impl Workbench {
    pub(super) fn finish_response(&mut self) {
        let Response { text, steps, .. } = std::mem::take(&mut self.response);
        if text.trim().is_empty() && steps.is_empty() {
            return;
        }
        if let (Some(store), Some(scope), Some(id)) =
            (&mut self.store, &self.active_scope, self.session_id)
            && let Err(error) = store.append_response(scope, id, &text, &steps)
        {
            self.state = format!("助手回复未保存：{error}");
        }
        self.messages.push(Message {
            role: ChatRole::Assistant,
            text,
            steps,
        });
    }
}
