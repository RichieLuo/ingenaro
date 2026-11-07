use super::*;
use ingenaro::model::{AuthoredEvent, EventTrigger, Interaction};

impl App {
    pub(super) fn interaction_editor(&mut self, ui: &mut egui::Ui, owner: Option<u64>) {
        let page_id = self.project.pages[self.page].id;
        let locked = owner.is_some_and(|id| {
            self.project.pages[self.page]
                .widgets
                .iter()
                .any(|w| w.id == id && w.locked)
        });
        let library = self.path.as_ref().is_some_and(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("rplib"))
        });
        ui.label(
            RichText::new(if owner.is_some() {
                "点击时"
            } else {
                "页面交互"
            })
            .strong(),
        );
        ui.add_space(8.0);
        ui.add_enabled_ui(!locked && !library, |ui| {
            if ui
                .add_sized(
                    [ui.available_width(), 30.0],
                    egui::Button::new(RichText::new("＋ 新建交互").color(Color32::WHITE))
                        .fill(PURPLE)
                        .stroke(Stroke::NONE),
                )
                .clicked()
            {
                let trigger = if owner.is_some() {
                    EventTrigger::Click
                } else {
                    EventTrigger::PageLoad
                };
                self.project
                    .interaction_events
                    .push(AuthoredEvent::new(page_id, owner, trigger));
            }
        })
        .response
        .on_disabled_hover_text(if library {
            "组件库展示页暂不新建事件"
        } else {
            "解锁组件后可编辑交互"
        });
        ui.label(
            RichText::new(if owner.is_some() {
                "单击元件时执行下面的动作。"
            } else {
                "页面载入时执行下面的动作。"
            })
            .small()
            .color(MUTED),
        );
        if self.native.is_some() {
            ui.label(
                RichText::new("新建用例与原有交互并存，保存回原文件。")
                    .small()
                    .color(MUTED),
            );
        }
        let pages = self
            .project
            .pages
            .iter()
            .map(|page| (page.id, page.name.clone()))
            .collect::<Vec<_>>();
        let targets = self.project.pages[self.page]
            .widgets
            .iter()
            .filter(|w| {
                self.native
                    .as_ref()
                    .is_none_or(|native| native.is_editable_widget(w.id))
            })
            .map(|w| (w.id, w.name.clone()))
            .collect::<Vec<_>>();
        let mut remove_event = None;
        for event in self
            .project
            .interaction_events
            .iter_mut()
            .filter(|event| event.page == page_id && event.owner == owner)
        {
            ui.add_space(10.0);
            let event_id = event.id.clone();
            ui.push_id(event_id, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(248, 249, 252))
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(event.trigger.label()).strong());
                            if ui
                                .add_enabled(!locked, egui::Button::new("删除交互").small())
                                .clicked()
                            {
                                remove_event = Some(event.id.clone());
                            }
                        });
                        ui.add_enabled_ui(!locked, |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut event.name)
                                    .hint_text("用例名称")
                                    .desired_width(f32::INFINITY),
                            );
                            let mut remove = None;
                            let mut reorder = None;
                            let count = event.actions.len();
                            for (index, action) in event.actions.iter_mut().enumerate() {
                                ui.push_id(index, |ui| {
                                    ui.add_space(8.0);
                                    ui.horizontal(|ui| {
                                        ui.label(format!("动作 {}", index + 1));
                                        if ui
                                            .add_enabled(index > 0, egui::Button::new("↑").small())
                                            .on_hover_text("上移动作")
                                            .clicked()
                                        {
                                            reorder = Some((index, index - 1));
                                        }
                                        if ui
                                            .add_enabled(
                                                index + 1 < count,
                                                egui::Button::new("↓").small(),
                                            )
                                            .on_hover_text("下移动作")
                                            .clicked()
                                        {
                                            reorder = Some((index, index + 1));
                                        }
                                        if ui.small_button("×").on_hover_text("删除动作").clicked()
                                        {
                                            remove = Some(index);
                                        }
                                    });
                                    edit_action(ui, action, &pages, &targets, false);
                                });
                            }
                            if let Some(index) = remove {
                                event.actions.remove(index);
                            } else if let Some((a, b)) = reorder {
                                event.actions.swap(a, b);
                            }
                            if event.actions.is_empty() {
                                ui.label(
                                    RichText::new("尚无动作，点击下方添加。")
                                        .small()
                                        .color(MUTED),
                                );
                            }
                            ui.add_space(8.0);
                            ui.menu_button("＋ 添加动作", |ui| {
                                for action in
                                    [Action::Navigate, Action::ToggleVisibility, Action::SetText]
                                {
                                    if ui
                                        .add_enabled(
                                            action == Action::Navigate || !targets.is_empty(),
                                            egui::Button::new(action_label(action)),
                                        )
                                        .clicked()
                                    {
                                        event.actions.push(Interaction {
                                            action,
                                            target_page: (action == Action::Navigate).then(|| {
                                                pages
                                                    .iter()
                                                    .find(|p| p.0 != page_id)
                                                    .or_else(|| pages.first())
                                                    .unwrap()
                                                    .0
                                            }),
                                            target_widget: matches!(
                                                action,
                                                Action::ToggleVisibility | Action::SetText
                                            )
                                            .then(|| targets.first().unwrap().0),
                                            value: String::new(),
                                        });
                                        ui.close();
                                    }
                                }
                            });
                        });
                    });
            });
        }
        if let Some(id) = remove_event {
            self.project
                .interaction_events
                .retain(|event| event.id != id);
        }
        let has_authored = self
            .project
            .interaction_events
            .iter()
            .any(|event| event.page == page_id && event.owner == owner);
        if let Some(id) = owner {
            let widget = self.project.pages[self.page]
                .widgets
                .iter_mut()
                .find(|w| w.id == id)
                .unwrap();
            if widget.interaction.action != Action::None {
                ui.add_space(12.0);
                egui::CollapsingHeader::new("已有单击动作")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add_enabled_ui(!locked, |ui| {
                            if self.native.is_none() || !has_authored {
                                edit_action(
                                    ui,
                                    &mut widget.interaction,
                                    &pages,
                                    &targets,
                                    self.native.is_some(),
                                );
                            } else {
                                ui.label(action_label(widget.interaction.action));
                            }
                            if ui
                                .add_enabled(
                                    self.native.is_none() || !has_authored,
                                    egui::Button::new("移除已有动作").small(),
                                )
                                .on_disabled_hover_text("原有原生动作保持原样；上方可编辑新建用例")
                                .clicked()
                            {
                                widget.interaction = Default::default();
                            }
                        });
                    });
            }
        }
        ui.add_space(12.0);
        ui.label(
            RichText::new("按 F5 预览；Ctrl+S 保存。浏览器运行态不改工程。")
                .small()
                .color(MUTED),
        );
    }
}

fn edit_action(
    ui: &mut egui::Ui,
    action: &mut Interaction,
    pages: &[(u64, String)],
    targets: &[(u64, String)],
    native_legacy: bool,
) {
    let previous = action.action;
    egui::ComboBox::from_id_salt("action-type")
        .selected_text(action_label(action.action))
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for kind in [Action::Navigate, Action::ToggleVisibility, Action::SetText] {
                ui.add_enabled_ui(
                    kind == Action::Navigate || !native_legacy && !targets.is_empty(),
                    |ui| {
                        ui.selectable_value(&mut action.action, kind, action_label(kind));
                    },
                );
            }
        });
    if action.action != previous {
        if action.action == Action::Navigate {
            action.target_page = pages.first().map(|p| p.0);
            action.target_widget = None;
        } else {
            action.target_widget = targets.first().map(|p| p.0);
            action.target_page = None;
        }
    }
    match action.action {
        Action::Navigate => {
            let label = pages
                .iter()
                .find(|p| Some(p.0) == action.target_page)
                .map_or("请选择页面", |p| p.1.as_str());
            egui::ComboBox::from_id_salt("action-target")
                .selected_text(label)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for (id, name) in pages {
                        ui.selectable_value(&mut action.target_page, Some(*id), name);
                    }
                });
        }
        Action::ToggleVisibility | Action::SetText => {
            let label = targets
                .iter()
                .find(|p| Some(p.0) == action.target_widget)
                .map_or("请选择元件", |p| p.1.as_str());
            egui::ComboBox::from_id_salt("action-target")
                .selected_text(label)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for (id, name) in targets {
                        ui.selectable_value(&mut action.target_widget, Some(*id), name);
                    }
                });
            if action.action == Action::SetText {
                ui.add(
                    egui::TextEdit::multiline(&mut action.value)
                        .hint_text("要设置的文本")
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
            }
        }
        Action::None => {}
    }
}
