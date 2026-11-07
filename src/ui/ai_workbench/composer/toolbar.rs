//! A single fixed-height row. Narrow layouts collapse labels into the same working icons.
use super::*;
mod controls;
mod effort;
mod model;
mod modes;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(in crate::ui::ai_workbench) use controls::bounds as control_bounds;
use controls::{Mark, ToolbarLayout, control};
#[cfg(test)]
pub(in crate::ui::ai_workbench) use effort::bounds as effort_bounds;

pub(super) const CONTROL_HEIGHT: f32 = 32.0;
pub(super) const TOOLBAR_HEIGHT: f32 = CONTROL_HEIGHT + 16.0;

impl App {
    pub(super) fn ai_composer_toolbar(&mut self, ui: &mut egui::Ui) -> bool {
        let engine = self
            .ai
            .settings
            .engines
            .iter()
            .find(|engine| engine.id == self.ai.selected_engine)
            .cloned();
        let engine_name = engine
            .as_ref()
            .map(|engine| match engine.kind {
                ai::EngineKind::Claude => "Claude",
                ai::EngineKind::Codex => "Codex",
                ai::EngineKind::OpenAiCompatible => "API",
            })
            .unwrap_or("引擎");
        let id = self.ai.selected_engine.clone();
        let model = self
            .ai
            .model_override
            .as_deref()
            .unwrap_or_else(|| {
                engine
                    .as_ref()
                    .map(|engine| engine.model.as_str())
                    .unwrap_or("")
            })
            .to_owned();
        let model_name = self.ai_model_display(&id, &model);
        let mode = self.ai.authorization.mode();
        let effort = self.ai_current_effort(&id, &model);
        let effort_text = effort_label(effort);
        let labels = [
            engine_name.to_owned(),
            mode.label().into(),
            short_model(&model_name, 80),
        ];
        let mut measured = labels.each_ref().map(|label| {
            ui.painter()
                .layout_no_wrap(label.clone(), FontId::proportional(12.0), INK)
                .size()
                .x
        });
        // Reserve the widest suffix regardless of the selected effort. The model
        // popup's anchor must not resize or cross a layout breakpoint while dragging.
        measured[2] += controls::effort_suffix_width(ui);
        let layout = ToolbarLayout::new(ui.available_width(), measured);
        let mut send = false;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = vec2(layout.gap, 0.0);
            ui.spacing_mut().interact_size.y = CONTROL_HEIGHT;
            ui.horizontal(|ui| {
                let response = control(
                    ui,
                    "engine",
                    layout.widths[0],
                    Mark::Engine(engine.as_ref().map(|engine| engine.kind)),
                    layout.labels[0].then_some(labels[0].as_str()),
                    &format!("智能体：{engine_name} · 点击选择"),
                    if engine
                        .as_ref()
                        .is_some_and(|engine| engine.kind == ai::EngineKind::Claude)
                    {
                        ORANGE
                    } else {
                        INK
                    },
                    true,
                    false,
                );
                egui::Popup::menu(&response).show(|ui| {
                    self.ai_engine_choices(ui);
                });

                let response = control(
                    ui,
                    "permission",
                    layout.widths[1],
                    Mark::Permission(mode),
                    layout.labels[1].then_some(labels[1].as_str()),
                    &format!("权限模式：{}", mode.label()),
                    ORANGE,
                    true,
                    layout.labels[1] || mode == permissions::PermissionMode::Full,
                );
                self.ai_modes_popup(&response);

                let response = control(
                    ui,
                    "model",
                    layout.widths[2],
                    Mark::Model(effort),
                    layout.labels[2].then_some(labels[2].as_str()),
                    &format!("模型：{model_name} · 思考强度：{effort_text} · 点击调整"),
                    SECONDARY,
                    true,
                    false,
                );
                self.ai_model_effort_popup(&response, &id, &model);

                let extra = (ui.available_width() - layout.widths[3]).max(0.0);
                if extra > 0.0 {
                    ui.add_space(extra);
                }
                let running = self.ai.running.is_some() || self.ai.page_activity.has_work();
                let ready = !self.ai.prompt.trim().is_empty() || !self.ai.images.is_empty();
                let stop = running && !ready;
                let response = control(
                    ui,
                    "send",
                    layout.widths[3],
                    Mark::Basic(if stop { Icon::Stop } else { Icon::Send }),
                    None,
                    if stop {
                        "停止设计任务"
                    } else if running {
                        "发送后续消息 · 选择发送方式"
                    } else {
                        "发送需求"
                    },
                    if stop { ORANGE } else { BLUE },
                    running || ready,
                    false,
                );
                if response.clicked() {
                    if stop {
                        self.ai_stop_design();
                    } else if !running {
                        send = true;
                    }
                }
                if running && ready {
                    egui::Popup::menu(&response).show(|ui| {
                        ui.set_max_width(230.0);
                        if ui.button("排队发送").on_hover_text("保留当前引用和附件，当前任务完成后发送").clicked() {
                            send = true;
                            ui.close();
                        }
                        let has_files = !self.ai.reference_files.files.is_empty() || self.ai.reference_files.loading();
                        if ui.add_enabled(!has_files && self.ai.steering.is_none() && !self.ai.page_agents.has_active(), egui::Button::new("引导当前对话"))
                            .on_hover_text("将补充要求和图片交给正在运行的任务。附带参考文件时请使用排队发送。")
                            .clicked() {
                            self.ai_steer();
                            ui.close();
                        }
                    });
                }
            });
        });
        send
    }

    fn ai_engine_choices(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(210.0);
        ui.label(RichText::new("选择本地智能体").strong());
        ui.separator();
        let previous = self.ai.selected_engine.clone();
        for engine in self
            .ai
            .settings
            .engines
            .iter()
            .filter(|engine| ai::is_local(engine.kind))
        {
            if engine_icons::choice(
                ui,
                engine.kind,
                &engine.name,
                self.ai.selected_engine == engine.id,
            )
            .clicked()
            {
                self.ai.selected_engine = engine.id.clone();
                ui.close();
            }
        }
        ui.separator();
        if ui.button("管理引擎…").clicked() {
            self.ai.settings_open = true;
            ui.close();
        }
        if previous != self.ai.selected_engine {
            self.ai.isolate();
            self.ai_reveal.clear();
            self.ai.model_override = None;
            self.ai.model_query.clear();
            let id = self.ai.selected_engine.clone();
            self.ai_request_models(&id, false);
        }
    }
    fn ai_supported_efforts(&self, id: &str, model: &str) -> Vec<String> {
        self.ai
            .catalogs
            .get(id)
            .and_then(|state| {
                state
                    .catalog
                    .models
                    .iter()
                    .find(|row| row.id == model)
                    .or_else(|| {
                        if model.is_empty() {
                            state
                                .catalog
                                .models
                                .iter()
                                .find(|row| row.is_default || row.id == "default")
                        } else {
                            None
                        }
                    })
            })
            .map(|row| row.reasoning_efforts.clone())
            .unwrap_or_default()
    }
}

fn effort_short_label(effort: Option<ai::ReasoningEffort>) -> &'static str {
    match effort {
        None => "自动",
        Some(ai::ReasoningEffort::Minimal) => "最小",
        Some(ai::ReasoningEffort::Low) => "低",
        Some(ai::ReasoningEffort::Medium) => "中",
        Some(ai::ReasoningEffort::High) => "高",
        Some(ai::ReasoningEffort::Xhigh) => "很高",
        Some(ai::ReasoningEffort::Max) => "最高",
    }
}

fn effort_label(effort: Option<ai::ReasoningEffort>) -> String {
    format!(
        "{}（{}）",
        effort_short_label(effort),
        effort.map_or("auto", |value| value.as_str())
    )
}
