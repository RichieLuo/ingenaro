//! Desktop coordination: workers never receive a mutable editor or its file path.
mod actions;
mod composer;
mod conversation;
#[cfg(test)]
mod draft_ui_tests;
mod engine_icons;
mod follow;
mod images;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod library_ui_tests;
#[cfg(test)]
mod model_tests;
mod models;
mod overview;
mod page_activity;
mod palette;
mod parallel;
mod permissions;
#[cfg(test)]
mod permissions_tests;
mod progress;
mod qa;
mod queue;
mod references;
mod settings;
mod steering;
#[cfg(test)]
mod tests;
mod tools;
mod views;

use super::*;
use rpshop::{ai, automation, design};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

pub(super) struct Workbench {
    pub visible: bool,
    artifact_open: bool,
    settings_open: bool,
    settings: ai::AiSettings,
    selected_engine: String,
    keys: HashMap<String, String>,
    protected_literals: Vec<String>,
    settings_engine: String,
    diagnosis: String,
    config_load_error: Option<String>,
    store: Option<design::Store>,
    session_id: Option<u64>,
    binding: Option<ai::SessionBinding>,
    running: Option<ai::RunningSession>,
    page_agents: parallel::PageAgents,
    steering: Option<steering::Pending>,
    pending_approval: Option<(String, String)>,
    host: automation::ToolHost,
    client: automation::ToolClient,
    server: Option<automation::Server>,
    agent_server: Option<automation::Server>,
    bindings: HashMap<u64, ai::SessionBinding>,
    pending_tool: Option<automation::ToolRequest>,
    document_id: String,
    generation: String,
    active_scope: Option<design::Scope>,
    task_cancelled: Arc<AtomicBool>,
    page_activity: page_activity::PageActivity,
    task_started: Option<Instant>,
    reasoning: String,
    effort_override: Option<ai::ReasoningEffort>,
    restore_focus_pending: bool,
    external_cancelled: Arc<AtomicBool>,
    pending_new_name: Option<String>,
    prompt: String,
    references: references::SelectionReferences,
    queue: queue::MessageQueue,
    images: Vec<ai::ImageAttachment>,
    image_draft: images::ImageDraft,
    palette: palette::PaletteEditor,
    prompt_editor_id: Option<egui::Id>,
    composer_height: f32,
    ime_active: bool,
    history_open: bool,
    mcp_open: bool,
    authorization: permissions::Authorization,
    catalogs: HashMap<String, models::CatalogState>,
    model_query: String,
    local_scan: Option<mpsc::Receiver<Vec<ai::LocalEngineInfo>>>,
    local_status: HashMap<String, ai::Diagnostics>,
    diagnostics_jobs: Vec<(String, mpsc::Receiver<Result<ai::Diagnostics, String>>)>,
    scan_started: bool,
    discuss_only: bool,
    task_readonly: bool,
    model_override: Option<String>,
    streamed: String,
    messages: Vec<(String, String)>,
    conversation: conversation::ConversationView,
    changes: Vec<String>,
    html: String,
    source: bool,
    kind: design::ArtifactKind,
    context: usize,
    task_context: usize,
    state: String,
    task_error: Option<String>,
    last_project_key: String,
    embedded: super::embedded_preview::EmbeddedPreview,
    persist: bool,
}

impl Workbench {
    pub fn new(persist: bool, document_id: String) -> Self {
        let (host, client) = automation::ToolHost::new();
        let (settings, config_load_error) = if persist {
            match ai::AiSettings::load_default() {
                Ok(settings) => (settings, None),
                Err(error) => (ai::AiSettings::default(), Some(error)),
            }
        } else {
            (ai::AiSettings::default(), None)
        };
        let selected_engine = settings
            .default_engine_id
            .as_ref()
            .filter(|id| {
                settings
                    .engines
                    .iter()
                    .any(|engine| &engine.id == *id && ai::is_local(engine.kind))
            })
            .cloned()
            .unwrap_or_else(|| {
                settings
                    .engines
                    .iter()
                    .find(|engine| ai::is_local(engine.kind))
                    .map(|engine| engine.id.clone())
                    .unwrap_or_default()
            });
        let (store, state) = if persist {
            match design::Store::open_default() {
                Ok(store) => (Some(store), "描述需求，或打开无需模型的内置示例。".into()),
                Err(error) => (None, format!("设计历史不可用：{error}")),
            }
        } else {
            (None, "隔离验证模式".into())
        };
        Self {
            visible: false,
            artifact_open: false,
            settings_open: false,
            settings,
            selected_engine,
            keys: HashMap::new(),
            protected_literals: Vec::new(),
            settings_engine: String::new(),
            diagnosis: config_load_error.clone().unwrap_or_default(),
            config_load_error,
            store,
            session_id: None,
            binding: None,
            running: None,
            page_agents: Default::default(),
            steering: None,
            pending_approval: None,
            host,
            client,
            server: None,
            agent_server: None,
            bindings: if persist {
                ai::AiSettings::default_path()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.join("ai-session-bindings.json")))
                    .and_then(|p| std::fs::read(p).ok())
                    .and_then(|b| serde_json::from_slice(&b).ok())
                    .unwrap_or_default()
            } else {
                HashMap::new()
            },
            pending_tool: None,
            document_id,
            generation: uuid::Uuid::new_v4().to_string(),
            active_scope: None,
            task_cancelled: Arc::new(AtomicBool::new(false)),
            page_activity: Default::default(),
            task_started: None,
            reasoning: String::new(),
            effort_override: None,
            restore_focus_pending: false,
            external_cancelled: Arc::new(AtomicBool::new(false)),
            pending_new_name: None,
            prompt: String::new(),
            references: Default::default(),
            queue: Default::default(),
            images: Vec::new(),
            image_draft: Default::default(),
            palette: Default::default(),
            prompt_editor_id: None,
            composer_height: 222.0,
            ime_active: false,
            history_open: false,
            mcp_open: false,
            authorization: Default::default(),
            catalogs: HashMap::new(),
            model_query: String::new(),
            local_scan: None,
            local_status: HashMap::new(),
            diagnostics_jobs: Vec::new(),
            scan_started: false,
            discuss_only: false,
            task_readonly: false,
            model_override: None,
            streamed: String::new(),
            messages: vec![],
            conversation: Default::default(),
            changes: vec![],
            html: String::new(),
            source: false,
            kind: design::ArtifactKind::Native,
            context: 0,
            task_context: 0,
            state,
            task_error: None,
            last_project_key: String::new(),
            embedded: Default::default(),
            persist,
        }
    }

    pub(super) fn open_startup(&mut self, example: bool) {
        self.visible = true;
        if example {
            self.artifact_open = true;
            self.kind = design::ArtifactKind::Html;
            if let design::ArtifactContent::Html { source } = design::builtin_sample() {
                self.html = source;
            }
            self.state = "内置示例 · 无需模型配置".into();
        }
    }

    pub(super) fn open_qa_dialog(&mut self, kind: &str) {
        self.visible = true;
        self.settings.engines = ai::local_defaults();
        self.selected_engine = self.settings.engines[0].id.clone();
        match kind {
            "authorization" => self.authorization.confirm_full = true,
            "settings" => self.settings_open = true,
            _ => {}
        }
    }

    fn cancel(&mut self) {
        self.steering = None;
        self.queue.paused = true;
        self.page_activity
            .clear_owner(page_activity::Owner::Builtin);
        self.restore_focus_pending = true;
        self.task_started = None;
        // Revoke document access before sending process cancellation.
        self.task_cancelled.store(true, Ordering::SeqCst);
        self.page_agents.finish();
        if let Some(running) = self.running.take() {
            running.control.cancel();
        }
        self.agent_server = None;
        self.pending_approval = None;
        if let Some(request) = self.pending_tool.take() {
            let _ = request.reply.send(Err("任务已取消".into()));
        }
    }

    fn isolate(&mut self) {
        self.cancel();
        self.authorization.reset();
        self.queue.clear();
        self.images.clear();
        self.image_draft = Default::default();
        self.references.clear();
        self.page_activity.clear();
        self.reasoning.clear();
        self.effort_override = None;
        self.discuss_only = false;
        self.external_cancelled.store(true, Ordering::SeqCst);
        self.server = None;
        self.generation = uuid::Uuid::new_v4().to_string();
        self.binding = None;
        self.session_id = None;
        self.messages.clear();
        self.conversation = Default::default();
        self.changes.clear();
        self.page_agents = Default::default();
        self.html.clear();
        self.artifact_open = false;
        self.streamed.clear();
        self.active_scope = None;
        self.embedded.clear();
    }

    fn scope(&self, project_key: &str) -> Option<design::Scope> {
        design::Scope::new(project_key, &self.selected_engine).ok()
    }
}

impl Drop for Workbench {
    fn drop(&mut self) {
        self.cancel();
        self.external_cancelled.store(true, Ordering::SeqCst);
    }
}

impl App {
    fn ai_project_key(&self) -> String {
        self.ai.document_id.clone()
    }

    pub(super) fn ai_bind_document_path(&mut self, opening: bool) {
        let Some(path) = self.ai_document_path_key() else {
            return;
        };
        if opening && let Some(id) = self.ai.settings.project_document_ids.get(&path) {
            self.ai.document_id = id.clone();
        }
        self.ai
            .settings
            .project_document_ids
            .insert(path, self.ai.document_id.clone());
        if self.ai.persist
            && self.ai.config_load_error.is_none()
            && let Err(error) = self.ai.settings.save_default()
        {
            self.ai.state = format!("工程会话绑定未保存：{error}");
        }
    }

    fn ai_document_path_key(&self) -> Option<String> {
        let path = self.path.as_ref()?;
        let path = std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.clone())
            .to_string_lossy()
            .to_string();
        Some(if cfg!(windows) {
            path.to_lowercase()
        } else {
            path
        })
    }

    pub(super) fn ai_native_checkpoint(&mut self) {
        self.ai_reveal.clear();
        self.ai.cancel();
        self.ai.external_cancelled.store(true, Ordering::SeqCst);
        self.ai.server = None;
        self.ai.binding = None;
        self.ai.state = "原生保存已重新载入对象绑定，本轮已结束；继续设计将使用新会话。".into();
    }

    pub(super) fn ai_apply_new_name(&mut self) {
        if let Some(name) = self.ai.pending_new_name.take() {
            self.project.name = name;
        }
    }

    pub(super) fn ai_document_replaced(&mut self) {
        self.ai_reveal.clear();
        self.ai.isolate();
        self.ai.palette = Default::default();
        self.ai.document_id = uuid::Uuid::new_v4().to_string();
        self.ai.last_project_key.clear();
    }

    pub(super) fn poll_ai(&mut self, ctx: &egui::Context) {
        if self.ai.last_project_key.is_empty() {
            self.ai_bind_document_path(true);
        }
        let key = self.ai_project_key();
        if self.ai.last_project_key != key {
            if !self.ai.last_project_key.is_empty() {
                self.ai.isolate();
                self.ai.palette = Default::default();
            }
            self.ai.last_project_key = key.clone();
            self.ai.selected_engine = self
                .ai
                .settings
                .project_engines
                .get(&key)
                .cloned()
                .unwrap_or_else(|| {
                    self.ai
                        .settings
                        .default_engine_id
                        .clone()
                        .unwrap_or_default()
                });
        }
        self.ai_poll_discovery();
        self.ai_poll_images();
        self.ai_poll_palette(ctx);
        self.poll_ai_events();
        self.poll_ai_tools(ctx);
        self.ai_poll_queue();
        if self.ai.page_activity.prune(&self.project) || self.ai.restore_focus_pending {
            self.ai.restore_focus_pending = false;
            self.ai_sync_activity_view();
        }
        if self.ai.running.is_some()
            || self.ai_image_loading()
            || self.ai.server.is_some()
            || self.ai.local_scan.is_some()
            || !self.ai.diagnostics_jobs.is_empty()
            || self
                .ai
                .catalogs
                .values()
                .any(|state| state.receiver.is_some())
        {
            ctx.request_repaint_after(Duration::from_millis(32));
        }
    }
}
