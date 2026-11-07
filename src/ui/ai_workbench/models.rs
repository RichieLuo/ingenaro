//! Shared asynchronous CLI discovery and model picker for settings and composer.
use super::*;
#[cfg(test)]
mod tests;

pub(super) const MODEL_SELECTION_FILL: Color32 = Color32::from_rgb(239, 233, 255);

fn model_choice(ui: &mut egui::Ui, selected: bool, title: &str) -> egui::Response {
    // Theme only the model rows, never the search editor's text selection.
    ui.scope(|ui| {
        ui.visuals_mut().selection.bg_fill = MODEL_SELECTION_FILL;
        ui.visuals_mut().selection.stroke = Stroke::new(1.0, PURPLE);
        let response = ui.selectable_label(selected, title);
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::new(("ai-model-choice-test", title)),
                response.rect,
            )
        });
        response
    })
    .inner
}

pub(super) struct CatalogState {
    pub catalog: ai::ModelCatalog,
    pub receiver: Option<mpsc::Receiver<Result<ai::ModelCatalog, String>>>,
    fingerprint: String,
    retry_after: Option<Instant>,
}
impl CatalogState {
    fn needs_request(&self, fingerprint: &str, force: bool, now: Instant) -> bool {
        self.receiver.is_none()
            && (force
                || self.fingerprint != fingerprint
                || self.retry_after.is_some_and(|retry| now >= retry))
    }

    fn poll(&mut self, now: Instant) {
        let result = match self.receiver.as_ref().map(|receiver| receiver.try_recv()) {
            Some(Ok(Ok(catalog))) if !catalog.models.is_empty() => Ok(catalog),
            Some(Ok(Ok(_))) => Err("CLI 返回的模型目录为空".into()),
            Some(Ok(Err(error))) => Err(error),
            Some(Err(mpsc::TryRecvError::Disconnected)) => Err("模型目录读取已中断".into()),
            _ => return,
        };
        self.receiver = None;
        match result {
            Ok(catalog) => {
                self.catalog = catalog;
                self.retry_after = None;
            }
            Err(error) => {
                // Keep the last usable list instead of replacing it with aliases.
                self.catalog.warning = Some(error);
                self.retry_after = Some(now + Duration::from_secs(15));
            }
        }
    }

    pub(super) fn qa(
        catalog: ai::ModelCatalog,
        receiver: mpsc::Receiver<Result<ai::ModelCatalog, String>>,
    ) -> Self {
        Self {
            catalog,
            receiver: Some(receiver),
            fingerprint: format!("{:?}|{:?}", ai::EngineKind::Claude, Option::<PathBuf>::None),
            retry_after: None,
        }
    }
}
#[cfg(test)]
impl CatalogState {
    pub(super) fn ready(catalog: ai::ModelCatalog) -> Self {
        Self {
            catalog,
            receiver: None,
            fingerprint: format!("{:?}|{:?}", ai::EngineKind::Claude, Option::<PathBuf>::None),
            retry_after: None,
        }
    }
}
impl App {
    pub(super) fn ai_request_models(&mut self, id: &str, force: bool) {
        let Some(config) = self
            .ai
            .settings
            .engines
            .iter()
            .find(|engine| engine.id == id && ai::is_local(engine.kind))
            .cloned()
        else {
            return;
        };
        let fingerprint = format!("{:?}|{:?}", config.kind, config.executable);
        if self
            .ai
            .catalogs
            .get(id)
            .is_some_and(|state| !state.needs_request(&fingerprint, force, Instant::now()))
        {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let catalog = self
            .ai
            .catalogs
            .get(id)
            .filter(|state| state.fingerprint == fingerprint)
            .map(|state| state.catalog.clone())
            .unwrap_or_else(|| ai::builtin_models(config.kind));
        self.ai.catalogs.insert(
            id.into(),
            CatalogState {
                catalog,
                receiver: Some(rx),
                fingerprint,
                retry_after: None,
            },
        );
        std::thread::spawn(move || {
            let _ = tx.send(ai::discover_models(&config));
        });
    }
    pub(super) fn ai_scan_local(&mut self) {
        if self.ai.local_scan.is_some() {
            return;
        }
        self.ai.scan_started = true;
        let (tx, rx) = mpsc::channel();
        self.ai.local_scan = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(ai::discover_local_engines());
        });
    }
    pub(super) fn ai_poll_discovery(&mut self) {
        if !self.ai.scan_started && !self.screenshot_mode && !cfg!(test) {
            self.ai_scan_local();
        }
        if let Some(receiver) = &self.ai.local_scan
            && let Ok(engines) = receiver.try_recv()
        {
            self.ai.local_scan = None;
            for local in engines {
                let existing = self
                    .ai
                    .settings
                    .engines
                    .iter()
                    .find(|engine| engine.kind == local.config.kind)
                    .map(|engine| engine.id.clone());
                let id = if let Some(id) = existing {
                    id
                } else {
                    let id = local.config.id.clone();
                    self.ai.settings.engines.push(local.config);
                    id
                };
                if let Some(config) = self
                    .ai
                    .settings
                    .engines
                    .iter()
                    .find(|engine| engine.id == id)
                    && config.executable.is_some()
                {
                    // A manual executable must be diagnosed itself, not mistaken for PATH's CLI.
                    let config = config.clone();
                    let (tx, rx) = mpsc::channel();
                    self.ai.diagnostics_jobs.push((id.clone(), rx));
                    std::thread::spawn(move || {
                        let _ = tx.send(config.diagnostics());
                    });
                } else {
                    self.ai.local_status.insert(id.clone(), local.diagnostics);
                }
                self.ai_request_models(&id, true);
            }
            let selected =
                self.ai.settings.engines.iter().any(|engine| {
                    engine.id == self.ai.selected_engine && ai::is_local(engine.kind)
                });
            if !selected {
                self.ai.selected_engine = self
                    .ai
                    .settings
                    .engines
                    .iter()
                    .filter(|engine| ai::is_local(engine.kind))
                    .find(|engine| {
                        self.ai
                            .local_status
                            .get(&engine.id)
                            .is_some_and(|status| status.available)
                    })
                    .or_else(|| {
                        self.ai
                            .settings
                            .engines
                            .iter()
                            .find(|engine| ai::is_local(engine.kind))
                    })
                    .map(|engine| engine.id.clone())
                    .unwrap_or_default();
                self.ai.restore_composer_preferences(None);
            }
            if self.ai.persist && self.ai.config_load_error.is_none() {
                if self
                    .ai
                    .settings
                    .default_engine_id
                    .as_ref()
                    .is_none_or(|id| {
                        !self
                            .ai
                            .settings
                            .engines
                            .iter()
                            .any(|engine| &engine.id == id && ai::is_local(engine.kind))
                    })
                {
                    self.ai.settings.default_engine_id = Some(self.ai.selected_engine.clone());
                }
                if let Err(error) = self.ai.settings.save_default() {
                    self.ai.state = format!("本地工具配置未保存：{error}");
                }
            }
        }
        self.ai
            .diagnostics_jobs
            .retain(|(id, receiver)| match receiver.try_recv() {
                Ok(Ok(status)) => {
                    self.ai.local_status.insert(id.clone(), status);
                    false
                }
                Ok(Err(error)) => {
                    self.ai.diagnosis = error;
                    false
                }
                Err(mpsc::TryRecvError::Disconnected) => false,
                Err(_) => true,
            });
        for state in self.ai.catalogs.values_mut() {
            state.poll(Instant::now());
        }
        // Prefetch the selected CLI before the picker opens. In headless tests
        // catalogs remain explicit fixtures rather than launching local tools.
        if !cfg!(test) && (self.ai.visible || self.ai.settings_open) {
            let id = self.ai.selected_engine.clone();
            self.ai_request_models(&id, false);
        }
    }
    pub(super) fn ai_model_picker(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        current: &str,
    ) -> Option<String> {
        self.ai_request_models(id, false);
        let mut selected = None;
        let mut refresh = false;
        ui.set_min_width(290.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("选择模型").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("刷新列表").clicked() {
                    refresh = true;
                }
            });
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.ai.model_query)
                .hint_text("搜索模型名称…")
                .desired_width(280.0),
        );
        let query = self.ai.model_query.trim().to_lowercase();
        ui.separator();
        if model_choice(ui, current.is_empty(), "沿用 CLI 默认模型").clicked() {
            selected = Some(String::new());
        }
        if let Some(state) = self.ai.catalogs.get(id) {
            if state.receiver.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("正在读取本地 CLI 模型目录…");
                });
            }
            egui::ScrollArea::vertical()
                .id_salt(("ai-model-catalog", id))
                .max_height(300.0)
                .show(ui, |ui| {
                    let mut count = 0;
                    for model in &state.catalog.models {
                        if !query.is_empty()
                            && !format!("{} {}", model.name, model.id)
                                .to_lowercase()
                                .contains(&query)
                        {
                            continue;
                        }
                        count += 1;
                        let title = if model.is_default {
                            format!("{}  ·  CLI 推荐", model.name)
                        } else {
                            model.name.clone()
                        };
                        if model_choice(ui, current == model.id, &title)
                            .on_hover_text(format!("{}\n{}", model.id, model.description))
                            .clicked()
                        {
                            selected = Some(model.id.clone());
                        }
                        if model.id != model.name {
                            ui.label(RichText::new(&model.id).size(10.0).color(MUTED));
                        }
                    }
                    if count == 0 {
                        ui.label("没有匹配的模型；可选择 CLI 默认或刷新列表。");
                    }
                });
            ui.separator();
            ui.label(RichText::new(&state.catalog.source).small().color(MUTED));
            if let Some(warning) = &state.catalog.warning {
                ui.label(RichText::new(warning).size(10.0).color(MUTED));
            }
        }
        if refresh {
            self.ai_request_models(id, true);
        }
        selected
    }
    pub(super) fn ai_model_display(&self, id: &str, model: &str) -> String {
        if model.is_empty() {
            return "CLI 默认模型".into();
        }
        self.ai
            .catalogs
            .get(id)
            .and_then(|state| state.catalog.models.iter().find(|row| row.id == model))
            .map(|model| model.name.clone())
            .unwrap_or_else(|| model.into())
    }
    pub(super) fn ai_choose_local(&mut self, id: String) {
        if self.ai.selected_engine != id {
            self.ai.isolate();
            self.ai_reveal.clear();
            self.ai.model_query.clear();
            self.ai.selected_engine = id.clone();
            self.ai.restore_composer_preferences(None);
        }
        self.ai_request_models(&id, false);
    }
}
