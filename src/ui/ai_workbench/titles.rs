use super::*;
use std::collections::{HashSet, VecDeque};
use std::task::Poll;

#[cfg(test)]
mod tests;

pub(super) const UNTITLED: &str = "设计会话";
const MAX_TITLE_CHARS: usize = 36;

#[derive(Default)]
pub(super) struct Titles {
    attempted: HashSet<u64>,
    queued: VecDeque<Candidate>,
    running: Option<TitleRun>,
}

struct Candidate {
    scope: design::Scope,
    id: u64,
    config: ai::EngineConfig,
    prompt: String,
}

struct TitleRun {
    scope: design::Scope,
    id: u64,
    session: ai::RunningSession,
    reply: String,
}

impl TitleRun {
    fn poll(&mut self) -> Poll<Option<String>> {
        for _ in 0..256 {
            match self.session.events.try_recv() {
                Ok(ai::AiEvent::TextDelta { text }) => {
                    if self.reply.len() + text.len() > 4096 {
                        return Poll::Ready(None);
                    }
                    self.reply.push_str(&text);
                }
                Ok(ai::AiEvent::Complete { cancelled: false }) => {
                    return Poll::Ready(generated_title(&self.reply));
                }
                Ok(ai::AiEvent::Complete { cancelled: true } | ai::AiEvent::Error { .. })
                | Err(mpsc::TryRecvError::Disconnected) => return Poll::Ready(None),
                Err(mpsc::TryRecvError::Empty) => break,
                _ => {}
            }
        }
        Poll::Pending
    }
}

impl App {
    pub(super) fn ai_queue_title(
        &mut self,
        scope: design::Scope,
        id: u64,
        config: ai::EngineConfig,
    ) {
        if !ai::is_local(config.kind) || self.ai.titles.attempted.contains(&id) {
            return;
        }
        let session = self
            .ai
            .store
            .as_ref()
            .and_then(|store| store.session(&scope, id).ok());
        if let Some(prompt) = session.and_then(untitled_request) {
            self.ai.titles.attempted.insert(id);
            self.ai.titles.queued.push_back(Candidate {
                scope,
                id,
                config,
                prompt: prompt.trim().chars().take(800).collect(),
            });
        }
    }

    pub(super) fn ai_poll_titles(&mut self, ctx: &egui::Context) {
        if let Some(run) = &mut self.ai.titles.running {
            let Poll::Ready(result) = run.poll() else {
                ctx.request_repaint_after(Duration::from_millis(32));
                return;
            };
            if let Some(title) = result
                && let Some(store) = &mut self.ai.store
                && store
                    .session(&run.scope, run.id)
                    .is_ok_and(|session| session.title == UNTITLED)
                && let Err(error) = store.set_session_title(&run.scope, run.id, &title)
            {
                eprintln!("会话标题未保存：{error}");
            }
            self.ai.titles.running = None;
        }

        // Backfill the current project's old history one session at a time when viewed.
        if self.ai.titles.queued.is_empty()
            && self.ai.visible
            && self.ai.history_open
            && let Some(scope) = self.ai.scope(&self.ai_project_key())
            && let Ok(config) = self.ai_submission_engine()
            && ai::is_local(config.kind)
            && let Some(store) = &self.ai.store
            && let Some(id) = store.sessions(&scope).into_iter().find_map(|session| {
                (!self.ai.titles.attempted.contains(&session.id)
                    && untitled_request(session).is_some())
                .then_some(session.id)
            })
        {
            self.ai_queue_title(scope, id, config);
        }

        if let Some(candidate) = self.ai.titles.queued.pop_front() {
            self.ai.titles.running = self.ai_start_title(candidate).ok();
            ctx.request_repaint_after(Duration::from_millis(32));
        }
    }

    fn ai_start_title(&self, candidate: Candidate) -> Result<TitleRun, String> {
        let Candidate {
            scope,
            id,
            config,
            prompt,
        } = candidate;
        let working_dir = std::env::temp_dir()
            .join("rpshop-agent-workspaces")
            .join("titles");
        std::fs::create_dir_all(&working_dir).map_err(|error| error.to_string())?;
        let session = ai::AiRuntime::start_title(ai::StartRequest {
            config,
            key: None,
            prompt,
            context: String::new(),
            images: Vec::new(),
            working_dir,
            scope: scope.project_key.clone(),
            resume: None,
            tools: self.ai.client.clone(),
            mcp: None,
            cancelled: Arc::new(AtomicBool::new(false)),
        })?;
        Ok(TitleRun {
            scope,
            id,
            session,
            reply: String::new(),
        })
    }
}

fn first_request(session: &design::Session) -> Option<&str> {
    session
        .messages
        .iter()
        .find(|message| message.role == design::ChatRole::User)
        .map(|message| message.text.as_str())
}

fn untitled_request(session: &design::Session) -> Option<&str> {
    if session.title == UNTITLED {
        first_request(session)
    } else {
        None
    }
}

pub(super) fn session_title(session: &design::Session) -> String {
    if session.title != UNTITLED {
        return session.title.clone();
    }
    first_request(session)
        .map(|prompt| line_excerpt(prompt.trim().lines().next().unwrap_or_default()))
        .filter(|title| !title.is_empty())
        .or_else(|| {
            session
                .revisions
                .first()
                .map(|revision| revision.label().to_owned())
        })
        .unwrap_or_else(|| "未命名会话".into())
}

fn line_excerpt(line: &str) -> String {
    let mut chars = line.chars().filter(|c| !c.is_control());
    let mut excerpt: String = chars.by_ref().take(MAX_TITLE_CHARS).collect();
    if chars.next().is_some() {
        excerpt.pop();
        excerpt.push('…');
    }
    excerpt
}

fn generated_title(reply: &str) -> Option<String> {
    let line = reply
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("```"))?;
    let line = line
        .strip_prefix("标题：")
        .or_else(|| line.strip_prefix("标题:"))
        .unwrap_or(line);
    let title = line.trim_matches(['"', '\'', '“', '”', '「', '」', '*', '#', '`', ' ']);
    let title = line_excerpt(title);
    (!title.is_empty() && title != UNTITLED).then_some(title)
}
