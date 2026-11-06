use super::*;

const MIN_CHAT_HEIGHT: f32 = 80.0;

impl App {
    /// Keep the native canvas and give the conversation one quiet, integrated composer.
    pub(in crate::ui) fn ai_chat_panel(&mut self, ctx: &egui::Context) {
        use super::composer::{Icon, icon_button};
        let mut panel = egui::SidePanel::right("ai-conversation")
            .resizable(true)
            .default_width(420.0)
            .width_range(260.0..=680.0)
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(12));
        if self.screenshot_mode
            && let Ok(width) = std::env::var("RPSHOP_QA_SIDEBAR_WIDTH")
            && let Ok(width) = width.parse::<f32>()
            && width.is_finite()
        {
            // QA controls the initial size only; it must not disable live resizing.
            panel = panel.default_width(width.clamp(260.0, 680.0));
        }
        let panel_response = panel.show(ctx, |ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("AI 设计").size(15.0).strong());
                ui.label(
                    RichText::new(if self.ai.kind == design::ArtifactKind::Native {
                        "画布"
                    } else {
                        "HTML"
                    })
                    .size(10.0)
                    .color(MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let more = icon_button(ui, Icon::More, "更多操作", true, MUTED);
                    egui::Popup::menu(&more).show(|ui| {
                        if ui.button("AI 设置…").clicked() {
                            self.ai.settings_open = true;
                            ui.close();
                        }
                        if ui.button("外部 API / MCP…").clicked() {
                            self.ai.mcp_open = true;
                            ui.close();
                        }
                        if ui.button("播放画布绘制示范").clicked() {
                            self.ai_play_drawing_demo(ctx);
                            ui.close();
                        }
                        if ui.button("预览多页设计总览").clicked() {
                            self.ai_activity_demo(ctx);
                            ui.close();
                        }
                        if ui.button("体验内置动态示例").clicked() {
                            self.ai_open_sample();
                            ui.close();
                        }
                        ui.separator();
                        let old = self.ai.kind;
                        ui.selectable_value(
                            &mut self.ai.kind,
                            design::ArtifactKind::Native,
                            "原生画布设计",
                        );
                        ui.selectable_value(
                            &mut self.ai.kind,
                            design::ArtifactKind::Html,
                            "独立动态 HTML",
                        );
                        if old != self.ai.kind {
                            self.ai_stop_design();
                            self.ai_new_conversation();
                        }
                    });
                    if icon_button(ui, Icon::Canvas, "收起 AI 面板", true, MUTED).clicked() {
                        self.ai.visible = false;
                        self.sidebar_layout.inspector_open = false;
                    }
                    if icon_button(ui, Icon::History, "历史会话与设计版本", true, MUTED).clicked()
                    {
                        self.ai.history_open = true;
                    }
                    if icon_button(
                        ui,
                        Icon::Plus,
                        "新建 AI 会话",
                        self.ai.running.is_none(),
                        MUTED,
                    )
                    .clicked()
                    {
                        self.ai_new_conversation();
                    }
                });
            });
            ui.separator();
            self.ai_palette_entry(ui);
            if self.ai.kind == design::ArtifactKind::Html
                && !self.ai.html.is_empty()
                && ui.small_button("HTML 产物 · 预览 / 源码").clicked()
            {
                self.ai.artifact_open = true;
            }
            // Reserve space for the queue, progress row, grip, and footer.
            let height_budget = ui.available_height() - self.ai_queue_height() - 89.0;
            self.ai_resize_composer(ui, height_budget - MIN_CHAT_HEIGHT);
            let chat_height = (height_budget - self.ai.composer_height).max(0.0);
            egui::ScrollArea::vertical()
                .id_salt("ai-chat-scroll")
                .max_height(chat_height)
                .min_scrolled_height(0.0)
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if self.ai.messages.is_empty() {
                        ui.add_space(28.0);
                        ui.label(
                            RichText::new("把想法，画出来。")
                                .size(20.0)
                                .strong()
                                .color(INK),
                        );
                        ui.add_space(5.0);
                        ui.label(
                            RichText::new("描述你想要的界面，或选中画布上的元件继续调整。")
                                .size(12.0)
                                .color(MUTED),
                        );
                        ui.add_space(20.0);
                        for (title, prompt) in [
                            (
                                "设计一个旅行规划首页",
                                "设计一个精致的旅行规划首页，包含侧边导航、精选路线和预约交互。",
                            ),
                            (
                                "保留布局，重新设计配色",
                                "保留当前布局，重新设计一套协调、有质感的配色。",
                            ),
                        ] {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(title)
                                            .size(12.0)
                                            .color(Color32::from_rgb(91, 101, 121)),
                                    )
                                    .fill(Color32::from_rgb(248, 249, 251))
                                    .stroke(Stroke::new(1.0, Color32::from_rgb(231, 234, 240)))
                                    .corner_radius(7)
                                    .min_size(vec2(ui.available_width(), 34.0)),
                                )
                                .clicked()
                            {
                                self.ai.prompt = prompt.into();
                            }
                        }
                    }
                    self.ai_conversation(ui);
                });
            ui.horizontal(|ui| {
                let time = ctx.input(|input| input.time);
                let drawing = self.ai_reveal.is_active(time);
                if self.ai.running.is_some() || drawing {
                    ui.add(egui::Spinner::new().size(12.0));
                }
                self.ai_subagent_indicator(ui);
                let state = self.ai_progress_text(time);
                ui.add(egui::Label::new(RichText::new(&state).size(10.0).color(MUTED)).truncate())
                    .on_hover_text(if drawing {
                        format!(
                            "当前画布事务的呈现动画，约 {:.1}s；不是模型生成百分比",
                            self.ai_reveal.remaining_seconds(time)
                        )
                    } else {
                        state
                    });
            });
            self.ai_queue_panel(ui);
            self.ai_composer(ui);
        });
        #[cfg(test)]
        ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::new("ai-chat-panel-test"),
                panel_response.response.rect,
            )
        });
        #[cfg(not(test))]
        let _ = panel_response;
        self.ai_history_windows(ctx);
    }

    pub(super) fn ai_new_conversation(&mut self) {
        self.ai_stop_design();
        self.ai.queue.clear();
        self.ai.images.clear();
        self.ai.image_draft = Default::default();
        self.ai.references.clear();
        self.ai.authorization.reset();
        self.ai.discuss_only = false;
        self.ai.generation = uuid::Uuid::new_v4().to_string();
        self.ai.session_id = None;
        self.ai.binding = None;
        self.ai.messages.clear();
        self.ai.conversation = Default::default();
        self.ai.streamed.clear();
        self.ai.reasoning.clear();
        self.ai.task_error = None;
        self.ai.active_scope = None;
        self.ai.changes.clear();
        self.ai.page_agents = Default::default();
        self.ai.html.clear();
        self.ai.artifact_open = false;
        self.ai.embedded.clear();
    }

    pub(super) fn ai_load_conversation(
        &mut self,
        scope: design::Scope,
        id: u64,
    ) -> Result<(), String> {
        if self.ai.scope(&self.ai_project_key()).as_ref() != Some(&scope) {
            return Err("历史会话不属于当前工程或引擎".into());
        }
        let session = self
            .ai
            .store
            .as_ref()
            .ok_or("历史存储不可用")?
            .session(&scope, id)
            .map_err(|error| error.to_string())?;
        let messages = session
            .messages
            .iter()
            .map(|message| {
                (
                    if message.role == design::ChatRole::User {
                        "你"
                    } else {
                        "AI"
                    }
                    .into(),
                    message.text.clone(),
                )
            })
            .collect();
        let kind = session.kind;
        let html = session
            .revisions
            .last()
            .and_then(|revision| match &revision.content {
                design::ArtifactContent::Html { source } => Some(source.clone()),
                _ => None,
            })
            .unwrap_or_default();
        self.ai_new_conversation();
        self.ai.messages = messages;
        self.ai.kind = kind;
        self.ai.html = html;
        self.ai.session_id = Some(id);
        self.ai.active_scope = Some(scope);
        self.ai.binding = self.ai.bindings.get(&id).cloned();
        self.ai.state = format!("已加载会话 #{id}");
        Ok(())
    }

    pub(super) fn ai_play_drawing_demo(&mut self, ctx: &egui::Context) {
        let mut empty = self.project.clone();
        empty.pages.iter_mut().for_each(|page| page.widgets.clear());
        self.ai_reveal
            .record(&empty, &self.project, ctx.input(|input| input.time));
        self.ai.state = "内置动效演示 · 重播当前画布绘制，不调用模型、不修改工程".into();
        ctx.request_repaint();
    }

    fn ai_open_sample(&mut self) {
        self.ai_new_conversation();
        self.ai.kind = design::ArtifactKind::Html;
        if self.ai.selected_engine.is_empty() {
            self.ai.selected_engine = "builtin-example".into();
        }
        if let design::ArtifactContent::Html { source } = design::builtin_sample() {
            self.ai.html = source;
        }
        self.ai.source = false;
        self.ai.artifact_open = true;
        self.ai_record_version(
            "内置示例",
            design::ArtifactContent::Html {
                source: self.ai.html.clone(),
            },
            true,
        );
        self.ai.state = "内置示例 · 不是模型生成结果".into();
    }

    fn ai_history_windows(&mut self, ctx: &egui::Context) {
        if self.ai.history_open {
            let mut open = true;
            dialog_style::window(ctx, "工程上下文与历史")
                .default_width(440.0)
                .show(ctx, |ui| {
                    if dialog_style::header(
                        ui,
                        dialog_style::Symbol::History,
                        "工程上下文与历史",
                        "管理引用范围、聊天会话和设计版本",
                    ) {
                        open = false;
                    }
                    self.ai_context_history(ui);
                });
            self.ai.history_open = open;
        }
        if self.ai.mcp_open {
            let mut open = true;
            dialog_style::window(ctx, "外部 API / MCP")
                .default_width(480.0)
                .show(ctx, |ui| {
                    if dialog_style::header(
                        ui,
                        dialog_style::Symbol::Link,
                        "外部 API / MCP",
                        "将当前桌面工程连接到受信任的本机客户端",
                    ) {
                        open = false;
                    }
                    dialog_style::section(ui, |ui| {
                        self.ai_external_controls(ui, ctx);
                    });
                });
            self.ai.mcp_open = open;
        }
    }

    fn ai_context_history(&mut self, ui: &mut egui::Ui) {
        egui::ComboBox::from_id_salt("ai-context")
            .selected_text(
                [
                    "上下文：整个工程",
                    "上下文：当前页面",
                    "上下文：当前选择",
                    "上下文：不自动附带",
                ][self.ai.context],
            )
            .show_ui(ui, |ui| {
                for (index, label) in [
                    "整个工程",
                    "当前页面",
                    "当前选择",
                    "不自动附带（读取需授权）",
                ]
                .iter()
                .enumerate()
                {
                    ui.selectable_value(&mut self.ai.context, index, *label);
                }
            });
        ui.label(
            RichText::new("所附带内容将发送给所选引擎及其实际模型服务；超出范围的读取需授权。")
                .small()
                .color(MUTED),
        );
        if ui.button("绑定此引擎到当前工程").clicked() {
            if self
                .ai
                .settings
                .engines
                .iter()
                .any(|engine| engine.id == self.ai.selected_engine)
            {
                self.ai
                    .settings
                    .project_engines
                    .insert(self.ai_project_key(), self.ai.selected_engine.clone());
                self.ai.state = if self.ai.persist {
                    self.ai
                        .settings
                        .save_default()
                        .map(|_| "工程引擎绑定已保存".into())
                        .unwrap_or_else(|error| error)
                } else {
                    "已绑定（验证模式）".into()
                };
            } else {
                self.ai.state = "请先选择已配置的引擎".into();
            }
        }
        let scope = self.ai.scope(&self.ai_project_key());
        let sessions = scope
            .as_ref()
            .and_then(|scope| {
                self.ai.store.as_ref().map(|store| {
                    store
                        .sessions(scope)
                        .into_iter()
                        .map(|session| (session.id, session.title.clone()))
                        .collect::<Vec<_>>()
                })
            })
            .unwrap_or_default();
        let mut load = None;
        egui::ComboBox::from_id_salt("ai-history")
            .selected_text(
                self.ai
                    .session_id
                    .map(|id| format!("会话 #{id}"))
                    .unwrap_or("历史会话".into()),
            )
            .show_ui(ui, |ui| {
                for (id, title) in sessions {
                    if ui
                        .selectable_label(self.ai.session_id == Some(id), format!("#{id} {title}"))
                        .clicked()
                    {
                        load = Some(id);
                    }
                }
            });
        if let (Some(id), Some(scope)) = (load, scope)
            && let Err(error) = self.ai_load_conversation(scope, id)
        {
            self.ai.state = error;
        }
        let mut restore = None;
        egui::ComboBox::from_id_salt("ai-versions")
            .selected_text("生成版本 / 恢复")
            .show_ui(ui, |ui| {
                if let (Some(store), Some(scope), Some(id)) =
                    (&self.ai.store, &self.ai.active_scope, self.ai.session_id)
                    && let Ok(session) = store.session(scope, id)
                {
                    for revision in session.revisions.iter().rev() {
                        if ui
                            .button(format!("#{} {}", revision.id, revision.label()))
                            .clicked()
                        {
                            restore = Some(revision.id);
                        }
                    }
                }
            });
        if let Some(id) = restore
            && let Err(error) = self.ai_restore(id)
        {
            self.ai.state = error;
        }
        ui.label(
            RichText::new("聊天历史、生成版本、画布撤销历史互相独立。恢复原生版本后仍可撤销。")
                .small()
                .color(MUTED),
        );
    }

    fn ai_external_controls(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label("仅回环地址和临时密钥。写操作须在此窗口授权；切换工程即断开。");
        if self.ai.server.is_none() {
            if ui.button("启用当前工程 MCP").clicked() {
                self.ai.external_cancelled = Arc::new(AtomicBool::new(false));
                self.ai.server = automation::start(
                    self.ai
                        .client
                        .scoped(self.ai_project_key(), self.ai.external_cancelled.clone()),
                )
                .map_err(|error| {
                    self.ai.state = error;
                })
                .ok();
            }
        } else {
            let server = self.ai.server.as_ref().unwrap();
            ui.monospace(server.endpoint());
            if ui.button("复制 MCP 配置（含临时密钥）").clicked() {
                let config = serde_json::json!({"mcpServers":{"rpshop":{"type":"http","url":server.endpoint(),"headers":{"Authorization":format!("Bearer {}",server.token())}}}});
                ctx.copy_text(serde_json::to_string_pretty(&config).unwrap_or_default());
                self.ai.state = "已复制临时配置；仅交给信任的本机客户端，不提交到仓库。".into();
            }
            if ui.button("停止外部访问").clicked() {
                self.ai.external_cancelled.store(true, Ordering::SeqCst);
                self.ai.server = None;
            }
        }
    }

    /// HTML remains an explicitly opened separate product, above the unchanged editor.
    pub(in crate::ui) fn ai_artifact_window(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if !self.ai.artifact_open {
            self.ai.embedded.hide();
            return;
        }
        let mut open = true;
        dialog_style::window(ctx, "动态 HTML 产物 · 独立于画布工程")
            .default_width(940.0)
            .default_height(680.0)
            .resizable(true)
            .show(ctx, |ui| {
                if dialog_style::header(
                    ui,
                    dialog_style::Symbol::Preview,
                    "动态 HTML 产物",
                    "独立交互作品 · 不会替换原生画布工程",
                ) {
                    open = false;
                }
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.ai.source, false, "交互预览");
                    ui.selectable_value(&mut self.ai.source, true, "源码");
                    if ui.button("浏览器预览").clicked() {
                        self.ai.state = design::open_preview(&self.ai.html)
                            .map(|_| "浏览器预览已打开".into())
                            .unwrap_or_else(|e| e.to_string());
                    }
                    if ui.button("导出 HTML").clicked()
                        && let Err(error) = self.ai_export_html()
                    {
                        self.ai.state = error;
                    }
                    ui.label(RichText::new("自包含 · 非原生元件").small().color(MUTED));
                });
                ui.separator();
                if self.ai.source {
                    self.ai.embedded.hide();
                    egui::ScrollArea::both().max_height(540.0).show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.ai.html)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .desired_rows(28),
                        );
                    });
                    if ui.button("校验并保存源码版本").clicked() {
                        match design::validate_html(&self.ai.html) {
                            Ok(()) => self.ai_record_version(
                                "手动源码编辑",
                                design::ArtifactContent::Html {
                                    source: self.ai.html.clone(),
                                },
                                false,
                            ),
                            Err(error) => self.ai.state = error.to_string(),
                        }
                    }
                } else if self.ai.html.is_empty() {
                    self.ai.embedded.hide();
                    ui.label("还没有 HTML 产物，请描述需求或打开内置示例。");
                } else {
                    let (rect, _) = ui.allocate_exact_size(
                        ui.available_size().max(vec2(500.0, 400.0)),
                        Sense::hover(),
                    );
                    let blocked = self.ai.settings_open
                        || self.ai.pending_tool.is_some()
                        || self.ai.pending_approval.is_some()
                        || self.confirm.is_some();
                    if blocked {
                        self.ai.embedded.hide();
                    } else {
                        self.ai.embedded.show(frame, ctx, rect, &self.ai.html);
                    }
                    if let Some(error) = &self.ai.embedded.error {
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            error,
                            FontId::proportional(13.0),
                            MUTED,
                        );
                    }
                }
            });
        if !open {
            self.ai.artifact_open = false;
            self.ai.embedded.hide();
        }
    }
}
