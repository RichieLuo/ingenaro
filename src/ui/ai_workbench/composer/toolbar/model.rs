//! Model selection and model-backed effort share one stable popup.
use super::*;

impl App {
    pub(super) fn ai_model_effort_popup(&mut self, anchor: &egui::Response, id: &str, model: &str) {
        let popup_id = egui::Popup::default_response_id(anchor);
        if anchor.clicked() && !egui::Popup::is_id_open(&anchor.ctx, popup_id) {
            // Reopening starts with the full catalog, never a previous search filter.
            self.ai.model_query.clear();
        }
        if egui::Popup::is_id_open(&anchor.ctx, popup_id)
            && anchor.ctx.input(|input| {
                input.raw.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::Escape,
                            pressed: true,
                            ..
                        }
                    )
                })
            })
        {
            egui::Popup::close_id(&anchor.ctx, popup_id);
            return;
        }
        egui::Popup::menu(anchor)
            .align(egui::RectAlign::TOP_START)
            .gap(6.0)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1.0, Color32::from_gray(217)))
                    .corner_radius(8)
                    .inner_margin(6),
            )
            .show(|ui| {
                ui.set_width(modes::WIDTH);
                let selected = self.ai_model_picker(ui, id, model);
                let current = selected.as_deref().unwrap_or(model);
                if selected.as_ref().is_some_and(|value| value != model) {
                    self.ai.model_override = selected.clone();
                    self.ai.effort_override = None;
                    self.ai.binding = None;
                }
                let configured = self.ai_current_effort(id, current);
                let supported = self.ai_supported_efforts(id, current);
                ui.add_space(4.0);
                ui.separator();
                let next = effort::row(ui, configured, &supported);
                if next != configured {
                    self.ai_set_effort(next);
                }
            });
    }

    pub(super) fn ai_current_effort(&self, id: &str, model: &str) -> Option<ai::ReasoningEffort> {
        let supported = self.ai_supported_efforts(id, model);
        self.ai
            .effort_override
            .or_else(|| {
                self.ai
                    .settings
                    .engines
                    .iter()
                    .find(|engine| engine.id == id)
                    .and_then(|engine| engine.reasoning_effort)
            })
            .filter(|value| supported.iter().any(|name| name == value.as_str()))
    }

    fn ai_set_effort(&mut self, next: Option<ai::ReasoningEffort>) {
        if next.is_none()
            && let Some(engine) = self
                .ai
                .settings
                .engines
                .iter_mut()
                .find(|engine| engine.id == self.ai.selected_engine)
        {
            engine.reasoning_effort = None;
        }
        self.ai.effort_override = next;
    }
}
