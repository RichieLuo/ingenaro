use super::*;

mod local_engines;

impl App {
    pub(in crate::ui) fn ai_toggle(&mut self) {
        self.ai.visible = !self.ai.visible;
        self.sidebar_layout.inspector_open = false;
    }
    pub(in crate::ui) fn ai_document_identity(&self) -> String {
        self.ai.document_id.clone()
    }

    pub(in crate::ui) fn ai_settings(&mut self, ctx: &egui::Context) {
        if !self.ai.settings_open {
            return;
        }
        self.ai.embedded.hide();
        let mut open = true;
        dialog_style::window(ctx, "AI 设置 · 本地智能体")
            .default_width(790.0)
            .resizable(true)
            .show(ctx, |ui| {
                if dialog_style::header(
                    ui,
                    dialog_style::Symbol::Settings,
                    "AI 引擎",
                    "选择本地 CLI 或配置自定义引擎。",
                ) {
                    open = false;
                }

                if let Some(error) = self.ai.config_load_error.clone() {
                    ui.colored_label(
                        Color32::DARK_RED,
                        format!("原配置读取失败，未覆盖：{error}"),
                    );
                    if ui.button("重新读取配置").clicked() {
                        match ai::AiSettings::load_default() {
                            Ok(settings) => {
                                self.ai.settings = settings;
                                self.ai.config_load_error = None;
                            }
                            Err(error) => self.ai.config_load_error = Some(error),
                        }
                    }
                    return;
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.ai.local_scan.is_none(),
                            egui::Button::new("重新检测本机工具"),
                        )
                        .clicked()
                    {
                        self.ai_scan_local();
                    }
                    if self.ai.local_scan.is_some() {
                        ui.spinner();
                        ui.label("正在检测安装、版本与登录…");
                    }
                    if self
                        .ai
                        .settings
                        .engines
                        .iter()
                        .any(|engine| !ai::is_desktop_engine(engine.kind))
                    {
                        ui.label(
                            RichText::new("其他平台配置已隐藏，原配置和凭据保持不变")
                                .small()
                                .color(MUTED),
                        );
                    }
                });
                self.ai_local_engine_picker(ui);
                ui.separator();
                let ids = self
                    .ai
                    .settings
                    .engines
                    .iter()
                    .filter(|engine| ai::is_desktop_engine(engine.kind))
                    .map(|engine| engine.id.clone())
                    .collect::<Vec<_>>();
                if ids.is_empty() {
                    ui.add_space(20.0);
                    ui.label("点击检测即可发现本机工具，不需要填写 API Key。");
                    ui.horizontal(|ui| {
                        for (kind, name) in [
                            (ai::EngineKind::Claude, "Claude Code"),
                            (ai::EngineKind::Codex, "Codex"),
                        ] {
                            if ui.button(format!("手动添加 {name}")).clicked() {
                                self.ai.settings.engines.push(ai::EngineConfig::new(kind));
                            }
                        }
                    });
                    ui.label(&self.ai.diagnosis);
                    return;
                }
                if !ids.contains(&self.ai.settings_engine) {
                    self.ai.settings_engine = ids[0].clone();
                }
                let mut choose = None;
                ui.columns(2, |columns| {
                    columns[0].set_max_width(220.0);
                    for id in &ids {
                        let engine = self
                            .ai
                            .settings
                            .engines
                            .iter()
                            .find(|engine| &engine.id == id)
                            .unwrap();
                        let name = match engine.kind {
                            ai::EngineKind::Claude => "Claude Code",
                            ai::EngineKind::Codex => "Codex",
                            _ => &engine.name,
                        };
                        let status = self.ai.local_status.get(id);
                        let selected = self.ai.settings_engine == *id;
                        let card = egui::Frame::new()
                            .fill(if selected {
                                Color32::from_rgb(240, 244, 255)
                            } else {
                                Color32::from_rgb(248, 249, 251)
                            })
                            .corner_radius(9)
                            .inner_margin(12)
                            .show(&mut columns[0], |ui| {
                                ui.set_min_width((ui.available_width()).max(0.0));
                                ui.horizontal(|ui| {
                                    engine_icons::inline(ui, engine.kind, 26.0);
                                    ui.label(RichText::new(name).strong().size(16.0));
                                });
                                let status_label = if engine.kind == ai::EngineKind::Plugin {
                                    "已配置 · 发送时检查"
                                } else {
                                    status
                                        .map(|d| {
                                            if d.available {
                                                "已检测到工具"
                                            } else {
                                                "未就绪"
                                            }
                                        })
                                        .unwrap_or("等待检测")
                                };
                                ui.label(RichText::new(status_label).small().color(MUTED));
                                if let Some(version) = status.and_then(|d| d.version.as_deref()) {
                                    ui.label(RichText::new(version).size(10.0).color(MUTED));
                                }
                                if self.ai.settings.default_engine_id.as_deref() == Some(id) {
                                    ui.label(RichText::new("默认智能体").small().color(PURPLE));
                                }
                            });
                        let response = columns[0]
                            .interact(
                                card.response.rect,
                                egui::Id::new(("settings-engine-card", id)),
                                Sense::click(),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                true,
                                selected,
                                name,
                            )
                        });
                        if response.hovered() {
                            columns[0].painter().rect_stroke(
                                card.response.rect,
                                9.0,
                                Stroke::new(1.0, PURPLE),
                                StrokeKind::Inside,
                            );
                        }
                        if response.clicked() {
                            choose = Some(id.clone());
                        }
                        columns[0].add_space(8.0);
                    }
                    let id = self.ai.settings_engine.clone();
                    let Some(index) = self
                        .ai
                        .settings
                        .engines
                        .iter()
                        .position(|engine| engine.id == id)
                    else {
                        return;
                    };
                    let engine = self.ai.settings.engines[index].clone();
                    let ui = &mut columns[1];
                    if engine.kind == ai::EngineKind::Plugin {
                        self.ai_local_engine_settings(ui, &engine);
                        return;
                    }
                    ui.horizontal(|ui| {
                        engine_icons::inline(ui, engine.kind, 24.0);
                        ui.heading(if engine.kind == ai::EngineKind::Claude {
                            "Claude Code"
                        } else {
                            "Codex"
                        });
                    });
                    if let Some(status) = self.ai.local_status.get(&id) {
                        ui.label(&status.login_hint);
                        ui.label(RichText::new(&status.action_hint).small().color(MUTED));
                    } else {
                        ui.label("点击上方检测安装、版本与登录状态。");
                    }
                    ui.add_space(10.0);
                    ui.label(RichText::new("模型").strong());
                    let current = self.ai.settings.engines[index].model.clone();
                    let label = self.ai_model_display(&id, &current);
                    egui::ComboBox::from_id_salt(("settings-model", &id))
                        .selected_text(label)
                        .width(260.0)
                        .show_ui(ui, |ui| {
                            if let Some(model) = self.ai_model_picker(ui, &id, &current) {
                                if model != current {
                                    self.ai.settings.engines[index].model = model;
                                    if let Some(preferences) =
                                        self.ai.settings.composer_preferences.get_mut(&id)
                                    {
                                        preferences.model = None;
                                        preferences.effort = ai::ReasoningPreference::Engine;
                                    }
                                }
                                ui.close();
                            }
                        });
                    ui.label(
                        RichText::new("模型来自该 CLI 的实时目录；不需要手动填写名称。")
                            .small()
                            .color(MUTED),
                    );
                    ui.add_space(8.0);
                    ui.collapsing("高级设置", |ui| {
                        let engine = &mut self.ai.settings.engines[index];
                        ui.label("可执行文件（留空自动检测）");
                        let mut executable = engine
                            .executable
                            .as_ref()
                            .map(|path| path.to_string_lossy().to_string())
                            .unwrap_or_default();
                        if ui.text_edit_singleline(&mut executable).changed() {
                            engine.executable = (!executable.trim().is_empty())
                                .then(|| PathBuf::from(executable.trim()));
                        }
                        dialog_style::note(ui, "生成任务持续等待，直到完成或你手动停止。");
                        dialog_style::note(
                            ui,
                            "思考强度可在聊天输入框底栏调整，仅显示当前模型支持的档位。",
                        );
                        if engine.kind == ai::EngineKind::Claude {
                            let mut enabled = engine.max_budget_usd.is_some();
                            if ui
                                .checkbox(&mut enabled, "设置本轮预算上限（USD）")
                                .changed()
                            {
                                engine.max_budget_usd = enabled.then_some(1.0);
                            }
                            if let Some(value) = &mut engine.max_budget_usd {
                                ui.add(egui::DragValue::new(value).range(0.01..=10000.0));
                            }
                        }
                    });
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if ui.button("设为默认").clicked() {
                            self.ai.settings.default_engine_id = Some(id.clone());
                        }
                        if ui
                            .add(dialog_style::primary("保存并使用", PURPLE))
                            .clicked()
                        {
                            self.ai.settings.default_engine_id.get_or_insert(id.clone());
                            let result = if self.ai.persist {
                                self.ai.settings.save_default()
                            } else {
                                Ok(())
                            };
                            match result {
                                Ok(()) => {
                                    if self.ai.selected_engine == id {
                                        self.ai.isolate();
                                        self.ai.restore_composer_preferences(None);
                                    }
                                    self.ai_choose_local(id.clone());
                                    self.ai.state = "本地智能体设置已保存".into();
                                    self.ai.settings_open = false;
                                }
                                Err(error) => self.ai.diagnosis = error,
                            }
                        }
                    });
                });
                if let Some(id) = choose {
                    self.ai.settings_engine = id;
                }
                if !self.ai.diagnosis.is_empty() {
                    ui.separator();
                    ui.label(&self.ai.diagnosis);
                }
                ui.separator();
                ui.label(
                    RichText::new("认证由各引擎管理。CLI 模型目录检测只读取模型信息。")
                        .small()
                        .color(MUTED),
                );
            });
        if !open {
            self.ai.settings_open = false;
        }
    }
}
