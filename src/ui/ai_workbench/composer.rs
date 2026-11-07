//! Native egui composer: context strip, free text, and one integrated control bar.
//! No credential fields or fake context-usage percentages live in this surface.
use super::*;

#[cfg(test)]
mod effort_tests;
mod icons;
mod input;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod tests;
pub(super) mod toolbar;
use icons::paint_icon;
pub(super) use icons::{Icon, icon_button};

const BLUE: Color32 = Color32::from_rgb(22, 126, 239);
const SECONDARY: Color32 = Color32::from_rgb(116, 124, 139);
const ORANGE: Color32 = Color32::from_rgb(198, 116, 51);
const SURFACE: Color32 = Color32::from_rgb(247, 248, 250);
const RESIZE_GRIP_ID: &str = "ai-composer-resize-grip";
const RESIZE_GRIP_SIZE: egui::Vec2 = vec2(46.0, 3.0);

impl App {
    pub(super) fn ai_resize_composer(&mut self, ui: &egui::Ui, height_limit: f32) {
        let ctx = ui.ctx();
        let grip_id = ui.make_persistent_id(RESIZE_GRIP_ID);
        // Read the grip interaction before laying out either part of the panel.
        if let Some(grip) = ctx.read_response(grip_id)
            && grip.dragged()
        {
            if grip.drag_started() {
                // Keep the same grab offset even after hitting a height limit.
                ctx.data_mut(|data| data.insert_temp(grip_id, self.ai.composer_height));
            }
            if let Some(start_height) = ctx.data(|data| data.get_temp::<f32>(grip_id))
                && let Some(delta) = ui.input(|input| input.pointer.total_drag_delta())
            {
                self.ai.composer_height = start_height - delta.y;
            }
        }
        let minimum = self.ai_composer_min_height();
        let maximum = height_limit.clamp(minimum, 440.0);
        self.ai.composer_height = self.ai.composer_height.clamp(minimum, maximum);
    }

    pub(super) fn ai_composer(&mut self, ui: &mut egui::Ui) {
        self.ai_observe_selection();
        let (_, grip) = ui.allocate_space(vec2(ui.available_width(), 12.0));
        let response = ui
            .interact(grip, ui.make_persistent_id(RESIZE_GRIP_ID), Sense::drag())
            .on_hover_and_drag_cursor(egui::CursorIcon::ResizeVertical)
            .on_hover_text("上下拖动调整输入框高度");
        ui.painter().rect_filled(
            Rect::from_center_size(grip.center(), RESIZE_GRIP_SIZE),
            2.0,
            if response.hovered() || response.dragged() {
                SECONDARY
            } else {
                Color32::from_gray(202)
            },
        );
        let editor_id = ui.make_persistent_id("ai-prompt-editor");
        self.ai.prompt_editor_id = Some(editor_id);
        let focused = ui.memory(|memory| memory.has_focus(editor_id));
        let has_hover = ui.rect_contains_pointer(Rect::from_min_size(
            ui.cursor().min,
            vec2(ui.available_width(), self.ai.composer_height),
        ));
        let outline = if focused || has_hover || response.dragged() {
            BLUE
        } else {
            Color32::from_rgb(202, 209, 220)
        };
        let mut send = false;
        let mut changed_reference = None;
        egui::Frame::new()
            .fill(Color32::WHITE)
            // Frame stroke width contributes to layout; focus must only change its color.
            .stroke(Stroke::new(1.0, outline))
            .corner_radius(9)
            .inner_margin(0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                ui.scope(|ui| {
                    ui.spacing_mut().button_padding = vec2(5.0, 3.0);
                    ui.spacing_mut().interact_size.y = 25.0;
                    egui::Frame::new()
                        .fill(SURFACE)
                        .corner_radius(egui::CornerRadius {
                            nw: 9,
                            ne: 9,
                            sw: 0,
                            se: 0,
                        })
                        .inner_margin(egui::Margin::symmetric(7, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.menu_button(
                                    RichText::new("＋").size(17.0).color(SECONDARY),
                                    |ui| {
                                        if ui.button("添加参考图片…").clicked() {
                                            self.ai_pick_images();
                                            ui.close();
                                        }
                                        if ui.button("粘贴剪贴板图片").clicked() {
                                            self.ai_paste_image();
                                            ui.close();
                                        }
                                        if ui
                                            .button("添加参考文件…")
                                            .on_hover_text(
                                                "发送后，所选引擎可读取附带的 UTF-8 源码或文本",
                                            )
                                            .clicked()
                                        {
                                            self.ai_pick_reference_files();
                                            ui.close();
                                        }
                                        ui.separator();
                                        ui.label(RichText::new("引用设计上下文").strong());
                                        for (context, label) in [
                                            (0, "当前工程"),
                                            (1, "当前页面"),
                                            (2, "当前选择"),
                                            (3, "不附带上下文"),
                                        ] {
                                            if ui
                                                .selectable_label(self.ai.context == context, label)
                                                .clicked()
                                            {
                                                changed_reference = Some(context);
                                                ui.close();
                                            }
                                        }
                                    },
                                )
                                .response
                                .on_hover_text(
                                    "添加参考文件、图片或设计上下文。引用内容会交给所选引擎",
                                );
                                if self.ai.context != 3 || self.ai.references.selected.is_some() {
                                    ui.label(RichText::new("│").color(Color32::from_gray(202)));
                                    let (title, tooltip) = self.ai_reference_label();
                                    let chip_width =
                                        (ui.available_width() - 98.0).clamp(38.0, 250.0);
                                    egui::Frame::new()
                                        .fill(Color32::WHITE)
                                        .stroke(Stroke::new(1.0, Color32::from_rgb(216, 221, 228)))
                                        .corner_radius(4)
                                        .inner_margin(egui::Margin::symmetric(5, 0))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                let (rect, _) = ui.allocate_exact_size(
                                                    vec2(15.0, 18.0),
                                                    Sense::hover(),
                                                );
                                                paint_icon(ui.painter(), rect, Icon::Page, BLUE);
                                                ui.add_sized(
                                                    [
                                                        chip_width
                                                            .min(ui.available_width() - 22.0)
                                                            .max(32.0),
                                                        22.0,
                                                    ],
                                                    egui::Label::new(
                                                        RichText::new(title).size(11.0).color(INK),
                                                    )
                                                    .truncate(),
                                                )
                                                .on_hover_text(tooltip);
                                                let remove = icon_button(
                                                    ui,
                                                    Icon::Close,
                                                    "移除上下文引用",
                                                    true,
                                                    SECONDARY,
                                                );
                                                #[cfg(test)]
                                                ui.ctx().data_mut(|data| {
                                                    data.insert_temp(
                                                        egui::Id::new("ai-reference-remove-test"),
                                                        remove.rect,
                                                    )
                                                });
                                                if remove.clicked() {
                                                    self.ai.references.remove();
                                                    changed_reference = Some(3);
                                                }
                                            });
                                        });
                                } else {
                                    ui.label(
                                        RichText::new("未附带引用").size(11.0).color(SECONDARY),
                                    );
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if icon_button(
                                            ui,
                                            Icon::Undo,
                                            "撤销最近一次画布编辑",
                                            !self.undo.is_empty(),
                                            SECONDARY,
                                        )
                                        .clicked()
                                        {
                                            self.undo();
                                        }
                                    },
                                );
                            });
                        });
                });
                if let Some(context) = changed_reference {
                    self.ai.context = context;
                }
                self.ai_images_strip(ui);
                let file_height = self.ai_reference_files_height();
                self.ai_reference_files_strip(ui);
                let toolbar_height = toolbar::TOOLBAR_HEIGHT;
                let height = (self.ai.composer_height
                    - 36.0
                    - toolbar_height
                    - 14.0
                    - self.ai_images_height()
                    - file_height)
                    .max(32.0);
                send |= input::handle_keys(ui, editor_id, &mut self.ai.ime_active);
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("ai-input-scroll")
                            .max_height(height)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let selection = input::selection_before_right_click(ui, editor_id);
                                let edit = ui.add_sized(
                                    [ui.available_width(), height],
                                    egui::TextEdit::multiline(&mut self.ai.prompt)
                                        .id(editor_id)
                                        .frame(false)
                                        .font(FontId::proportional(13.0))
                                        .text_color(INK)
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(1)
                                        .return_key(egui::KeyboardShortcut::new(
                                            egui::Modifiers::SHIFT,
                                            egui::Key::Enter,
                                        ))
                                        .hint_text(
                                            RichText::new(
                                                "描述你的设计，选中元件会自动引用，Enter 发送",
                                            )
                                            .color(Color32::from_rgb(157, 164, 178)),
                                        ),
                                );
                                if !edit.has_focus() {
                                    self.ai.ime_active = false;
                                }
                                #[cfg(windows)]
                                if edit.has_focus() {
                                    // Place the Windows candidate window at the caret, even in a tall composer.
                                    ui.output_mut(|output| {
                                        if let Some(ime) = &mut output.ime {
                                            ime.rect = ime.cursor_rect;
                                        }
                                    });
                                }
                                if input::context_menu(&edit, &self.ai.prompt, selection) {
                                    self.ai_paste_image();
                                }
                            });
                    });
                if self.ai.prompt.ends_with('@') && !self.ai.ime_active {
                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 2.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("引用").small().color(SECONDARY));
                            for (context, label) in
                                [(0, "工程"), (1, "当前页面"), (2, "选择的元件")]
                            {
                                if ui.small_button(label).clicked() {
                                    self.ai.prompt.pop();
                                    self.ai.context = context;
                                    ui.memory_mut(|m| m.request_focus(editor_id));
                                }
                            }
                        });
                    });
                }
                egui::Frame::new()
                    .fill(Color32::from_rgb(237, 239, 242))
                    .corner_radius(egui::CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 9,
                        se: 9,
                    })
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        send |= self.ai_composer_toolbar(ui);
                    });
            });
        if send
            && !self.ai.ime_active
            && (!self.ai.prompt.trim().is_empty() || !self.ai.images.is_empty())
        {
            self.ai_start();
        }
        ui.add_space(2.0);
        ui.scope(|ui| {
            ui.spacing_mut().interact_size.y = 14.0;
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Enter 发送 · Shift+Enter 换行")
                        .size(10.0)
                        .color(SECONDARY),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(if self.ai.discuss_only {
                            "只读讨论"
                        } else {
                            "修改可撤销"
                        })
                        .size(10.0)
                        .color(SECONDARY),
                    );
                });
            });
        });
    }

    fn ai_composer_min_height(&self) -> f32 {
        (36.0
            + toolbar::TOOLBAR_HEIGHT
            + 14.0
            + self.ai_images_height()
            + self.ai_reference_files_height()
            + 32.0)
            .max(150.0)
    }

    fn ai_reference_label(&self) -> (String, String) {
        if let Some(label) = self.ai.references.label() {
            return (
                label,
                "当前选中元件已自动加入引用；发送时固定内容，可点击 × 移除".into(),
            );
        }
        match self.ai.context {
            1 => (
                self.project.pages[self.page].name.clone(),
                "引用当前页面；发送时读取页面最新内容".into(),
            ),
            2 => (
                format!("已选 {} 个元件", self.selected.len()),
                "引用当前选择；发送时读取选中元件最新内容".into(),
            ),
            _ => (
                self.project.name.clone(),
                "引用当前工程；发送时读取工程最新内容".into(),
            ),
        }
    }

    pub(super) fn ai_stop_design(&mut self) {
        self.ai.cancel();
        self.ai_reveal.clear();
        self.ai.state = "任务已停止；已完成的编辑可撤销".into();
    }
}

fn short_model(model: &str, maximum: usize) -> String {
    if model.chars().count() <= maximum {
        model.into()
    } else {
        format!("{}…", model.chars().take(maximum - 1).collect::<String>())
    }
}
