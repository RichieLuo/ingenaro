use super::*;
use anyhow::{Context, Result, ensure};
use ingenaro::{
    grouping::{self, Layer},
    rp_format::NativeDocument,
};

const DISCLOSURE_WIDTH: f32 = 18.0;

#[derive(Clone, Copy)]
enum NameTarget {
    NewGroup,
    Group(u64),
    Widget(u64),
}

pub(super) struct NameDialog {
    target: NameTarget,
    page_id: u64,
    name: String,
    focus: bool,
    error: Option<String>,
}

enum RowAction {
    SelectGroup(BTreeSet<u64>, bool),
    SelectWidget(u64, bool, bool),
    Rename(NameTarget, String),
    ToggleVisibility(u64),
    ToggleLock(u64),
}

impl App {
    pub(super) fn layers_pane(&mut self, ui: &mut egui::Ui) {
        let page = &self.project.pages[self.page];
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = LAYER_COLUMN_SPACING;
            let title_width = layer_title_width(ui.available_width());
            ui.allocate_ui_with_layout(
                vec2(title_width, 22.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(title_width);
                    ui.label(RichText::new("图层").strong().size(14.0));
                    ui.label(
                        RichText::new(page.widgets.len().to_string())
                            .small()
                            .color(MUTED),
                    );
                },
            );
            for (label, hint) in [
                ("显示", "眼睛控制组件在画布和预览中的显示。"),
                ("锁定", "锁定后不能移动、缩放或修改组件。"),
            ] {
                ui.add_sized(
                    [LAYER_CONTROL_WIDTH, 22.0],
                    egui::Label::new(RichText::new(label).size(11.0).color(MUTED)),
                )
                .on_hover_text(hint);
            }
        });
        let new_group = ui
            .horizontal(|ui| {
                let create = ui
                    .add_enabled(self.selected.len() >= 2, egui::Button::new("新建分组"))
                    .on_hover_text("将所选图层编为一组，已有外层分组会整体编入")
                    .on_disabled_hover_text("选择至少两个图层，按住 Ctrl 或 Shift 多选")
                    .clicked();
                ui.add(
                    egui::Label::new(RichText::new("双击名称可重命名").size(11.0).color(MUTED))
                        .truncate(),
                );
                create
            })
            .inner;
        ui.add_space(4.0);
        if page.widgets.is_empty() {
            ui.label(RichText::new("当前页面没有图层").color(MUTED));
        }
        let click_key = ui.id().with("last-layer-click");
        let mut rows = LayerRows {
            page,
            selected: &self.selected,
            expanded: &mut self.expanded_layer_groups,
            native: self.native.as_ref(),
            last_click: ui.data(|data| data.get_temp::<Option<u64>>(click_key).flatten()),
            clicked: None,
            action: None,
        };
        egui::ScrollArea::vertical()
            .id_salt(("layers", page.id))
            .auto_shrink([false, false])
            .max_height(ui.available_height())
            .show(ui, |ui| rows.show(ui, &page.widgets, 0));
        if ui.input(|input| input.pointer.button_clicked(egui::PointerButton::Primary)) {
            ui.data_mut(|data| data.insert_temp(click_key, rows.clicked));
        }
        let action = rows.action;
        if new_group {
            self.open_layer_name_dialog(NameTarget::NewGroup, String::new());
        }
        match action {
            Some(RowAction::SelectGroup(ids, additive)) => self.select_layers(ids, additive),
            Some(RowAction::SelectWidget(id, additive, inside)) => {
                self.select_hit(id, additive, inside)
            }
            Some(RowAction::Rename(target, name)) => self.open_layer_name_dialog(target, name),
            Some(RowAction::ToggleVisibility(id)) => {
                if let Some(widget) = self.project.pages[self.page]
                    .widgets
                    .iter_mut()
                    .find(|w| w.id == id)
                {
                    widget.hidden = !widget.hidden;
                }
            }
            Some(RowAction::ToggleLock(id)) => {
                if let Some(widget) = self.project.pages[self.page]
                    .widgets
                    .iter_mut()
                    .find(|w| w.id == id)
                {
                    widget.locked = !widget.locked;
                }
            }
            None => {}
        }
    }

    fn open_layer_name_dialog(&mut self, target: NameTarget, name: String) {
        self.layer_name_dialog = Some(NameDialog {
            target,
            page_id: self.project.pages[self.page].id,
            name,
            focus: true,
            error: None,
        });
    }

    pub(super) fn show_layer_name_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.layer_name_dialog.take() else {
            return;
        };
        if self.confirm.is_some() || self.project.pages[self.page].id != dialog.page_id {
            return;
        }
        let (title, label, submit) = match dialog.target {
            NameTarget::NewGroup => ("新建图层分组", "分组名称", "创建"),
            NameTarget::Group(_) => ("重命名分组", "分组名称", "保存"),
            NameTarget::Widget(_) => ("重命名图层", "图层名称", "保存"),
        };
        let mut close = false;
        let mut commit = false;
        let response = dialog_style::modal(ctx, "layer-name").show(ctx, |ui| {
            ui.set_width(340.0);
            close = dialog_style::header(ui, dialog_style::Symbol::Text, title, "");
            dialog_style::section(ui, |ui| {
                ui.label(label);
                let mut edit = egui::TextEdit::singleline(&mut dialog.name)
                    .desired_width(f32::INFINITY)
                    .char_limit(1024)
                    .show(ui);
                if std::mem::take(&mut dialog.focus) {
                    edit.response.request_focus();
                    edit.state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(dialog.name.chars().count()),
                        )));
                    edit.state.store(ctx, edit.response.id);
                }
                if edit.response.changed() {
                    dialog.error = None;
                }
                if let Some(error) = &dialog.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                commit = edit.response.lost_focus()
                    && ui.input(|input| input.key_pressed(egui::Key::Enter));
            });
            dialog_style::actions(ui, |ui| {
                commit |= ui.add(dialog_style::primary(submit, PURPLE)).clicked();
                close |= ui.add(dialog_style::secondary("取消")).clicked();
            });
        });
        if close || response.should_close() {
            return;
        }
        if commit {
            match self.apply_layer_name(dialog.target, &dialog.name) {
                Ok(()) => {
                    self.status = format!("{title}成功");
                    return;
                }
                Err(error) => {
                    dialog.error = Some(format!("{error:#}"));
                    dialog.focus = true;
                }
            }
        }
        self.layer_name_dialog = Some(dialog);
    }

    fn apply_layer_name(&mut self, target: NameTarget, name: &str) -> Result<()> {
        if matches!(target, NameTarget::NewGroup) {
            return self.edit_groups(false, Some(name));
        }
        let before = self.project.clone();
        let mut draft = before.clone();
        let page = &mut draft.pages[self.page];
        match target {
            NameTarget::Group(id) => grouping::rename_group(page, id, name)?,
            NameTarget::Widget(id) => {
                ensure!(
                    !name.trim().is_empty() && name.len() <= 1024,
                    "图层名称不能为空且不能超过 1024 字节"
                );
                let widget = page
                    .widgets
                    .iter_mut()
                    .find(|w| w.id == id)
                    .context("图层已删除")?;
                ensure!(
                    !widget.locked
                        && self
                            .native
                            .as_ref()
                            .is_none_or(|native| native.is_editable_widget(id)),
                    "图层已锁定或为只读内容"
                );
                widget.name = name.to_owned();
            }
            NameTarget::NewGroup => unreachable!(),
        }
        if let Some(native) = &self.native {
            native.preview_scene(&draft)?;
        }
        self.project = draft;
        self.remember(before);
        Ok(())
    }
}

struct LayerRows<'a> {
    page: &'a Page,
    selected: &'a BTreeSet<u64>,
    expanded: &'a mut BTreeSet<u64>,
    native: Option<&'a NativeDocument>,
    last_click: Option<u64>,
    clicked: Option<u64>,
    action: Option<RowAction>,
}

impl LayerRows<'_> {
    fn show(&mut self, ui: &mut egui::Ui, widgets: &[Widget], depth: usize) {
        for layer in grouping::layers(widgets, depth) {
            let (id, target, name, text, selected, editable) = match layer {
                Layer::Group { id, members } => {
                    let name = self.page.group_name(id);
                    let editable = self.native.is_none_or(|native| {
                        members.iter().any(|w| native.is_editable_widget(w.id))
                    });
                    let text = format!("{name} ({})", members.len());
                    (
                        id,
                        NameTarget::Group(id),
                        name,
                        text,
                        members.iter().all(|w| self.selected.contains(&w.id)),
                        editable,
                    )
                }
                Layer::Widget(widget) => {
                    let icon = if widget.kind == WidgetKind::Text {
                        "T"
                    } else {
                        "◇"
                    };
                    let editable = self
                        .native
                        .is_none_or(|native| native.is_editable_widget(widget.id));
                    (
                        widget.id,
                        NameTarget::Widget(widget.id),
                        widget.name.as_str().into(),
                        format!("{icon} {}", widget.name),
                        self.selected.contains(&widget.id),
                        editable,
                    )
                }
            };
            ui.push_id(id, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = LAYER_COLUMN_SPACING;
                    let title_width = layer_title_width(ui.available_width());
                    ui.allocate_ui_with_layout(
                        vec2(title_width, 27.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add_space((depth as f32 * 14.0).min(42.0));
                            if matches!(layer, Layer::Group { .. }) {
                                let open = self.expanded.contains(&id);
                                if group_disclosure(ui, open).clicked() {
                                    if open {
                                        self.expanded.remove(&id);
                                    } else {
                                        self.expanded.insert(id);
                                    }
                                }
                            } else {
                                ui.add_space(DISCLOSURE_WIDTH + LAYER_COLUMN_SPACING);
                            }
                            let response = ui
                                .add_sized(
                                    [ui.available_width().max(0.0), 27.0],
                                    egui::Button::selectable(selected, (text, egui::Atom::grow()))
                                        .truncate(),
                                )
                                .on_hover_text(name.as_ref());
                            let can_rename =
                                editable && !matches!(layer, Layer::Widget(w) if w.locked);
                            if response.clicked() {
                                let additive = ui.input(|i| i.modifiers.shift || i.modifiers.ctrl);
                                self.clicked = (!additive).then_some(id);
                                // egui counts nearby clicks across controls, including the disclosure arrow.
                                if can_rename
                                    && !additive
                                    && self.last_click == Some(id)
                                    && (response.double_clicked() || response.triple_clicked())
                                {
                                    self.action = Some(RowAction::Rename(target, name.to_string()));
                                } else {
                                    self.action = Some(match layer {
                                        Layer::Group { members, .. } => RowAction::SelectGroup(
                                            members.iter().map(|w| w.id).collect(),
                                            additive,
                                        ),
                                        Layer::Widget(widget) => {
                                            RowAction::SelectWidget(widget.id, additive, depth > 0)
                                        }
                                    });
                                }
                            }
                            response.context_menu(|ui| {
                                if ui
                                    .add_enabled(can_rename, egui::Button::new("重命名"))
                                    .clicked()
                                {
                                    self.action = Some(RowAction::Rename(target, name.to_string()));
                                    ui.close();
                                }
                            });
                        },
                    );
                    if let Layer::Widget(widget) = layer {
                        ui.add_enabled_ui(editable, |ui| {
                            if layer_control(
                                ui,
                                LayerControl::Visibility {
                                    hidden: widget.hidden,
                                },
                                &widget.name,
                            )
                            .clicked()
                            {
                                self.action = Some(RowAction::ToggleVisibility(widget.id));
                            }
                            if layer_control(
                                ui,
                                LayerControl::Lock {
                                    locked: widget.locked,
                                },
                                &widget.name,
                            )
                            .clicked()
                            {
                                self.action = Some(RowAction::ToggleLock(widget.id));
                            }
                        });
                    }
                });
                if let Layer::Group { members, .. } = layer
                    && self.expanded.contains(&id)
                {
                    self.show(ui, members, depth + 1);
                }
            });
        }
    }
}

fn group_disclosure(ui: &mut egui::Ui, open: bool) -> egui::Response {
    let hint = if open { "收起分组" } else { "展开分组" };
    let (rect, response) = ui.allocate_exact_size(vec2(DISCLOSURE_WIDTH, 27.0), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), hint));
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        if response.hovered() || response.has_focus() {
            ui.painter()
                .rect_filled(rect.shrink(1.0), 4.0, visuals.weak_bg_fill);
        }
        sidebar::paint_disclosure(ui.painter(), rect.center(), open, visuals.text_color());
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(hint)
}
