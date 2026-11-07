use super::*;
use rpshop::model::{CanvasMode, HorizontalLayout};

impl App {
    pub(super) fn inspector_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("inspector")
            .resizable(true)
            .default_width(244.0)
            .width_range(220.0..=340.0)
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(12.0))
            .show(ctx, |ui| {
                if self.confirm.is_some() || self.editing.is_some() {
                    ui.disable();
                }
                inspector_tabs(ui, &mut self.tab);
                ui.separator();
                ui.add_space(10.0);
                egui::ScrollArea::vertical()
                    .id_salt("properties")
                    .show(ui, |ui| {
                        if self.selected.is_empty() {
                            self.page_inspector(ui);
                            return;
                        }
                        if self.tab == 2 {
                            self.inspector_notes(ui);
                            return;
                        }
                        let editable_ids: BTreeSet<u64> = self.project.pages[self.page]
                            .widgets
                            .iter()
                            .filter(|widget| {
                                self.selected.contains(&widget.id)
                                    && !widget.locked
                                    && self.native.as_ref().is_none_or(|document| {
                                        document.is_editable_widget(widget.id)
                                    })
                            })
                            .map(|widget| widget.id)
                            .collect();
                        ui.label(
                            RichText::new(if self.selected.len() > 1 {
                                format!("已选择 {} 个组件", self.selected.len())
                            } else {
                                "组件调整".into()
                            })
                            .strong(),
                        );
                        ui.add_space(8.0);
                        if self.tab == 0 {
                            egui::CollapsingHeader::new("排列与变换")
                                .id_salt("inspector-transform")
                                .default_open(self.selected.len() > 1)
                                .show(ui, |ui| {
                                    ui.add_enabled_ui(!editable_ids.is_empty(), |ui| {
                                        let mut angle = self.project.pages[self.page]
                                            .widgets
                                            .iter()
                                            .find(|widget| editable_ids.contains(&widget.id))
                                            .or_else(|| {
                                                self.project.pages[self.page].widgets.iter().find(
                                                    |widget| self.selected.contains(&widget.id),
                                                )
                                            })
                                            .map_or(0.0, |widget| widget.rotation);
                                        let mixed = self.project.pages[self.page]
                                            .widgets
                                            .iter()
                                            .filter(|widget| editable_ids.contains(&widget.id))
                                            .any(|widget| (widget.rotation - angle).abs() > 0.001);
                                        ui.horizontal(|ui| {
                                            ui.label("角度");
                                            if ui
                                                .add(
                                                    egui::DragValue::new(&mut angle)
                                                        .range(-36000.0..=36000.0)
                                                        .speed(1.0)
                                                        .suffix("°"),
                                                )
                                                .on_hover_text(
                                                    "修改角度会分别旋转选中组件，保持各自中心位置",
                                                )
                                                .changed()
                                            {
                                                for widget in
                                                    &mut self.project.pages[self.page].widgets
                                                {
                                                    if editable_ids.contains(&widget.id) {
                                                        widget.rotation = angle.rem_euclid(360.0);
                                                    }
                                                }
                                            }
                                            if mixed {
                                                ui.label(
                                                    RichText::new("多种").small().color(MUTED),
                                                );
                                            }
                                        });
                                        ui.horizontal(|ui| {
                                            if ui
                                                .button("−90°")
                                                .on_hover_text("绕选中范围中心逆时针旋转 90°")
                                                .clicked()
                                            {
                                                self.align(12);
                                            }
                                            if ui
                                                .button("+90°")
                                                .on_hover_text("绕选中范围中心顺时针旋转 90°")
                                                .clicked()
                                            {
                                                self.align(13);
                                            }
                                        });
                                        ui.add_space(10.0);
                                        ui.add_enabled_ui(self.selected.len() > 1, |ui| {
                                            ui.label("对齐到选中范围");
                                            egui::Grid::new("selection-alignment")
                                                .num_columns(2)
                                                .spacing([4.0, 4.0])
                                                .show(ui, |ui| {
                                                    for (index, (label, mode)) in [
                                                        ("左对齐", 0),
                                                        ("水平居中", 1),
                                                        ("右对齐", 2),
                                                        ("顶对齐", 3),
                                                        ("垂直居中", 4),
                                                        ("底对齐", 5),
                                                    ]
                                                    .into_iter()
                                                    .enumerate()
                                                    {
                                                        if ui.button(label).clicked() {
                                                            self.align(mode);
                                                        }
                                                        if index % 2 == 1 {
                                                            ui.end_row();
                                                        }
                                                    }
                                                });
                                        });
                                        ui.add_enabled_ui(self.selected.len() > 2, |ui| {
                                            ui.horizontal(|ui| {
                                                if ui
                                                    .button("水平分布")
                                                    .on_hover_text(
                                                        "按旋转后的外接范围保持等距；锁定组件固定",
                                                    )
                                                    .clicked()
                                                {
                                                    self.align(6);
                                                }
                                                if ui
                                                    .button("垂直分布")
                                                    .on_hover_text(
                                                        "按旋转后的外接范围保持等距；锁定组件固定",
                                                    )
                                                    .clicked()
                                                {
                                                    self.align(7);
                                                }
                                            });
                                        });
                                        ui.add_space(10.0);
                                        ui.label("图层顺序");
                                        egui::Grid::new("selection-layer-order")
                                            .num_columns(2)
                                            .spacing([8.0, 4.0])
                                            .show(ui, |ui| {
                                                if ui.button("置于顶层").clicked() {
                                                    self.align(8);
                                                }
                                                if ui.button("上移一层").clicked() {
                                                    self.align(9);
                                                }
                                                ui.end_row();
                                                if ui.button("下移一层").clicked() {
                                                    self.align(10);
                                                }
                                                if ui.button("置于底层").clicked() {
                                                    self.align(11);
                                                }
                                            });
                                        ui.horizontal(|ui| {
                                            if ui
                                                .add_enabled(
                                                    self.selected.len() > 1,
                                                    egui::Button::new("组合"),
                                                )
                                                .on_hover_text(
                                                    "Ctrl+G：组合；Ctrl+Alt 单击可选择组内元件",
                                                )
                                                .clicked()
                                            {
                                                self.group_selection(false);
                                            }
                                            if ui
                                                .button("解组")
                                                .on_hover_text("Ctrl+Shift+G：移除一层组合")
                                                .clicked()
                                            {
                                                self.group_selection(true);
                                            }
                                        });
                                        ui.horizontal(|ui| {
                                            if ui.button("水平翻转").clicked() {
                                                self.flip_selection(true);
                                            }
                                            if ui.button("垂直翻转").clicked() {
                                                self.flip_selection(false);
                                            }
                                        });
                                    });
                                });
                            ui.add_space(12.0);
                            ui.separator();
                            ui.add_space(12.0);
                            if self.selected.len() > 1 {
                                if editable_ids.is_empty() {
                                    ui.label(
                                        RichText::new(
                                            "当前选择包含锁定组件或只读预览，不能直接修改。",
                                        )
                                        .color(MUTED),
                                    );
                                }
                                if ui.button("复制选中组件").clicked() {
                                    self.copy_to_system(ctx);
                                }
                                if ui
                                    .add_enabled(
                                        !editable_ids.is_empty(),
                                        egui::Button::new("删除选中组件"),
                                    )
                                    .clicked()
                                {
                                    self.delete_selection();
                                }
                                return;
                            }
                        }
                        if self.selected.len() > 1 {
                            ui.label(RichText::new("一次选择一个组件设置交互。").color(MUTED));
                            return;
                        }
                        let id = *self.selected.first().unwrap();
                        if self
                            .native
                            .as_ref()
                            .is_some_and(|d| !d.is_editable_widget(id))
                        {
                            ui.label("只读预览");
                            ui.label(
                                RichText::new(
                                    "当前内容为只读预览，不能直接修改。请选择可编辑组件进行编辑。",
                                )
                                .color(MUTED),
                            );
                            return;
                        }
                        if self.tab == 1 {
                            self.interaction_editor(ui, Some(id));
                            return;
                        }
                        let Some(index) = self.project.pages[self.page]
                            .widgets
                            .iter()
                            .position(|w| w.id == id)
                        else {
                            return;
                        };
                        let page = &mut self.project.pages[self.page];
                        let responsive = page.canvas == CanvasMode::ResponsiveWidth;
                        let widget = &mut page.widgets[index];
                        ui.label(RichText::new(widget.kind.label()).strong().size(15.0));
                        ui.add_space(12.0);
                        ui.label("组件名称");
                        ui.add_enabled(
                            !widget.locked,
                            egui::TextEdit::singleline(&mut widget.name),
                        );
                        if widget.kind == WidgetKind::Radio {
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new("相同组内只能选中一个单选按钮")
                                    .small()
                                    .color(MUTED),
                            );
                            let mut explicit_group = widget.radio_group.is_some();
                            if ui
                                .add_enabled(
                                    !widget.locked,
                                    egui::Checkbox::new(&mut explicit_group, "指定单选组"),
                                )
                                .changed()
                            {
                                widget.radio_group = explicit_group.then(|| widget.name.clone());
                            }
                            if let Some(group) = &mut widget.radio_group {
                                ui.add_enabled(
                                    !widget.locked,
                                    egui::TextEdit::singleline(group).hint_text("留空表示不分组"),
                                );
                            } else {
                                ui.label(RichText::new("默认按组件名称分组").small().color(MUTED));
                            }
                        }
                        ui.add_space(14.0);
                        let locked = widget.locked;
                        ui.add_enabled_ui(!locked, |ui| {
                            ui.label(RichText::new("位置与尺寸").strong());
                            ui.add_space(8.0);
                            egui::Grid::new("geometry")
                                .num_columns(2)
                                .spacing([6.0, 8.0])
                                .show(ui, |ui| {
                                    ui.label("X");
                                    ui.add_sized(
                                        [100.0, 22.0],
                                        egui::DragValue::new(&mut widget.x)
                                            .range(-100000.0..=100000.0)
                                            .speed(1.0),
                                    );
                                    ui.end_row();
                                    ui.label("Y");
                                    ui.add_sized(
                                        [100.0, 22.0],
                                        egui::DragValue::new(&mut widget.y)
                                            .range(-100000.0..=100000.0)
                                            .speed(1.0),
                                    );
                                    ui.end_row();
                                    ui.label("W");
                                    ui.add_sized(
                                        [100.0, 22.0],
                                        egui::DragValue::new(&mut widget.width)
                                            .range(1.0..=20000.0)
                                            .speed(1.0),
                                    );
                                    ui.end_row();
                                    ui.label("H");
                                    ui.add_sized(
                                        [100.0, 22.0],
                                        egui::DragValue::new(&mut widget.height)
                                            .range(1.0..=20000.0)
                                            .speed(1.0),
                                    );
                                    ui.end_row();
                                });
                            if responsive {
                                horizontal_layout_controls(ui, &mut widget.horizontal_layout);
                            }
                            ui.add_space(15.0);
                            ui.separator();
                            ui.add_space(12.0);
                            ui.label(
                                RichText::new(if widget.kind.is_option_list() {
                                    "选项（每行一个）"
                                } else {
                                    "内容"
                                })
                                .strong(),
                            );
                            ui.add_space(8.0);
                            ui.add(
                                egui::TextEdit::multiline(&mut widget.text)
                                    .desired_rows(3)
                                    .desired_width(f32::INFINITY),
                            );
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                ui.label("字号");
                                ui.add(
                                    egui::DragValue::new(&mut widget.font_size)
                                        .range(6.0..=240.0)
                                        .suffix(" px"),
                                );
                            });
                            ui.add_space(15.0);
                            ui.separator();
                            ui.add_space(12.0);
                            ui.label(RichText::new("外观").strong());
                            ui.add_space(8.0);
                            color_edit(ui, "填充", &mut widget.fill);
                            color_edit(ui, "边框", &mut widget.stroke);
                            color_edit(ui, "文字", &mut widget.text_color);
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.label("圆角");
                                ui.add(
                                    egui::DragValue::new(&mut widget.radius)
                                        .range(0.0..=200.0)
                                        .suffix(" px"),
                                );
                            });
                            ui.add(
                                egui::Slider::new(&mut widget.opacity, 0.0..=1.0).text("不透明度"),
                            );
                        });
                        ui.add_space(15.0);
                        ui.separator();
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut widget.locked, "锁定");
                            ui.checkbox(&mut widget.hidden, "隐藏");
                        });
                    });
            });
    }
}

impl App {
    fn page_inspector(&mut self, ui: &mut egui::Ui) {
        if self.tab == 1 {
            self.interaction_editor(ui, None);
            ui.add_space(18.0);
            ui.label(RichText::new("本页元件交互").strong());
            let actions = self.project.pages[self.page]
                .widgets
                .iter()
                .filter(|widget| {
                    widget.interaction.action != Action::None
                        || self.project.interaction_events.iter().any(|event| {
                            event.page == self.project.pages[self.page].id
                                && event.owner == Some(widget.id)
                        })
                })
                .map(|widget| {
                    (
                        widget.id,
                        widget.name.clone(),
                        if widget.interaction.action == Action::None {
                            "单击事件"
                        } else {
                            action_label(widget.interaction.action)
                        },
                    )
                })
                .collect::<Vec<_>>();
            if actions.is_empty() {
                ui.label(
                    RichText::new("当前没有已解析的元件交互")
                        .small()
                        .color(MUTED),
                );
            }
            for (id, name, action) in actions {
                if ui
                    .add_sized(
                        [ui.available_width(), 28.0],
                        egui::Button::new(format!("{} · {}", name, action)).truncate(),
                    )
                    .on_hover_text(&name)
                    .clicked()
                {
                    self.select_hit(id, false, true);
                }
            }
            return;
        }
        if self.tab == 2 {
            self.inspector_notes(ui);
            return;
        }
        let native = self.native.is_some();
        let library = native
            && self.path.as_ref().is_some_and(|p| {
                p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("rplib"))
            });
        let page = &mut self.project.pages[self.page];
        property_label(ui, "页面尺寸");
        let size_label = match page.canvas {
            CanvasMode::Auto => "Auto".to_owned(),
            CanvasMode::FixedWidth => format!("{:.0} × Auto", page.width),
            CanvasMode::FixedHeight => format!("Auto × {:.0}", page.height),
            CanvasMode::Fixed => format!("{:.0} × {:.0}", page.width, page.height),
            CanvasMode::ResponsiveWidth => "响应式宽度".to_owned(),
        };
        if native {
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 26.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(size_label);
                },
            )
            .response
            .on_hover_text("尺寸模式来自原生页面设置；当前保持原文件尺寸设置");
        } else {
            let previous = page.canvas;
            egui::ComboBox::from_id_salt("inspector-page-mode")
                .selected_text(size_label)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for (mode, label) in [
                        (CanvasMode::Auto, "Auto"),
                        (CanvasMode::Fixed, "固定宽高"),
                        (CanvasMode::FixedWidth, "固定宽度"),
                        (CanvasMode::FixedHeight, "固定高度"),
                        (CanvasMode::ResponsiveWidth, "响应式宽度"),
                    ] {
                        ui.selectable_value(&mut page.canvas, mode, label);
                    }
                });
            if previous != page.canvas {
                self.fit = true;
            }
        }
        if !page.canvas.auto_width() || !page.canvas.auto_height() {
            ui.add_enabled_ui(!native, |ui| {
                egui::Grid::new("inspector-page-dimensions")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        if !page.canvas.auto_width() {
                            ui.label(if page.canvas == CanvasMode::ResponsiveWidth {
                                "设计宽度"
                            } else {
                                "宽度"
                            });
                            ui.add(
                                egui::DragValue::new(&mut page.width)
                                    .range(64.0..=20000.0)
                                    .suffix(" px"),
                            );
                            ui.end_row();
                        }
                        if !page.canvas.auto_height() {
                            ui.label("高度");
                            ui.add(
                                egui::DragValue::new(&mut page.height)
                                    .range(64.0..=20000.0)
                                    .suffix(" px"),
                            );
                            ui.end_row();
                        }
                        if let Some(minimum) = page.responsive_min_width() {
                            let mut width = minimum.min(page.width);
                            ui.label("最小宽度");
                            if ui
                                .add(
                                    egui::DragValue::new(&mut width)
                                        .range(64.0..=page.width)
                                        .suffix(" px"),
                                )
                                .on_hover_text("预览窄于此宽度时可以横向滚动")
                                .changed()
                                || width != minimum
                            {
                                page.responsive_min_width = Some(width);
                            }
                            ui.end_row();
                        }
                    });
            })
            .response
            .on_disabled_hover_text("原生页面的静态尺寸目前只读，保存时保留原设置");
        }
        if page.canvas == CanvasMode::ResponsiveWidth {
            ui.label(
                RichText::new("画布按设计宽度编辑。浏览器预览按元件的水平布局伸缩。")
                    .small()
                    .color(MUTED),
            );
        }
        ui.separator();
        ui.add_enabled(
            false,
            egui::Button::new(RichText::new("添加自适应视图").underline()).frame(false),
        )
        .on_disabled_hover_text("暂未实现自适应视图编辑；原生文件中的已有视图保持原样");
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(10.0);
        ui.label(RichText::new("默认").size(18.0));
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            property_label(ui, "页面排列");
            ui.label(
                RichText::new(if native { "保留原设置" } else { "默认" })
                    .size(12.0)
                    .color(MUTED),
            )
            .on_hover_text("页面排列尚未提供编辑；原生排列设置不会改写");
        });
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        property_label(ui, "填充");
        ui.add_space(5.0);
        ui.add_enabled_ui(!library, |ui| {
            color_edit(ui, "颜色", &mut page.background);
        })
        .response
        .on_disabled_hover_text("元件库展示页仅用于浏览，不能修改页面背景");
        ui.add_enabled(false, egui::Button::new("图片填充").frame(false))
            .on_disabled_hover_text("暂不支持替换页面背景图；原生页面背景图保持原样");
        ui.add_space(16.0);
        ui.separator();
        egui::CollapsingHeader::new("页面信息")
            .id_salt("inspector-page-info")
            .show(ui, |ui| {
                property_label(ui, "页面名称");
                ui.add_enabled(
                    !library,
                    egui::TextEdit::singleline(&mut page.name).desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);
                property_label(ui, "工程");
                ui.add(egui::Label::new(&self.project.name).truncate())
                    .on_hover_text(&self.project.name);
            });
    }

    fn inspector_notes(&self, ui: &mut egui::Ui) {
        let page = &self.project.pages[self.page];
        ui.label(
            RichText::new(if self.selected.is_empty() {
                "页面说明"
            } else {
                "元件说明"
            })
            .strong()
            .size(14.0),
        );
        ui.add_space(10.0);
        if self.selected.is_empty() {
            ui.label(&page.name);
            ui.label(
                RichText::new(format!(
                    "{} 个元件 · {} 个隐藏 · {} 个锁定",
                    page.widgets.len(),
                    page.widgets.iter().filter(|w| w.hidden).count(),
                    page.widgets.iter().filter(|w| w.locked).count()
                ))
                .size(12.0)
                .color(MUTED),
            );
        } else {
            for widget in page
                .widgets
                .iter()
                .filter(|w| self.selected.contains(&w.id))
            {
                ui.add(
                    egui::Label::new(format!("{} · {}", widget.name, widget.kind.label()))
                        .truncate(),
                )
                .on_hover_text(&widget.name);
                let state = if self
                    .native
                    .as_ref()
                    .is_some_and(|document| !document.is_editable_widget(widget.id))
                {
                    "只读预览"
                } else if widget.locked {
                    "已锁定"
                } else {
                    "可编辑"
                };
                ui.label(
                    RichText::new(format!("ID {} · {}", widget.id, state))
                        .size(11.0)
                        .color(MUTED),
                );
            }
        }
        ui.add_space(14.0);
        ui.separator();
        ui.add_space(10.0);
        ui.label(
            RichText::new(if self.native.is_some() {
                "原生说明尚未解析，源文件中的原始说明会保留。"
            } else {
                "当前工程暂未提供说明编辑。"
            })
            .size(12.0)
            .color(MUTED),
        );
        if !self.import_warnings.is_empty() {
            ui.add_space(14.0);
            egui::CollapsingHeader::new("导入提示").show(ui, |ui| {
                for warning in &self.import_warnings {
                    ui.label(RichText::new(warning).size(12.0).color(MUTED));
                }
            });
        }
    }
}

fn inspector_tabs(ui: &mut egui::Ui, tab: &mut usize) {
    let width = (ui.available_width() - 16.0) / 3.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        for (index, label) in ["样式", "交互", "说明"].into_iter().enumerate() {
            let active = *tab == index;
            let response = ui.add_sized(
                [width, 29.0],
                egui::Button::new(RichText::new(label).color(if active {
                    Color32::from_rgb(0, 151, 224)
                } else {
                    INK
                }))
                .frame(false),
            );
            if response.clicked() {
                *tab = index;
            }
            if active {
                let y = response.rect.bottom();
                ui.painter().line_segment(
                    [
                        Pos2::new(response.rect.left() + 8.0, y),
                        Pos2::new(response.rect.right() - 8.0, y),
                    ],
                    Stroke::new(2.0, Color32::from_rgb(0, 151, 224)),
                );
            }
        }
    });
}

fn horizontal_layout_controls(ui: &mut egui::Ui, layout: &mut HorizontalLayout) {
    ui.add_space(10.0);
    ui.label(RichText::new("水平布局").strong());
    ui.horizontal_wrapped(|ui| {
        for (preset, label) in [
            (HorizontalLayout::LEFT, "固定左侧"),
            (HorizontalLayout::RIGHT, "固定右侧"),
            (HorizontalLayout::STRETCH, "横向伸展"),
        ] {
            if ui.selectable_label(*layout == preset, label).clicked() {
                *layout = preset;
            }
        }
    });
    egui::Grid::new("horizontal-layout").show(ui, |ui| {
        ui.label("左边缘");
        ui.add(
            egui::DragValue::new(&mut layout.left)
                .range(0.0..=layout.right)
                .suffix("%"),
        );
        ui.end_row();
        ui.label("右边缘");
        ui.add(
            egui::DragValue::new(&mut layout.right)
                .range(layout.left..=100.0)
                .suffix("%"),
        );
        ui.end_row();
    });
    ui.label(
        RichText::new("比例决定边缘跟随画布宽度变化的幅度。")
            .small()
            .color(MUTED),
    );
}

fn property_label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(MUTED));
}

#[cfg(test)]
mod tests {
    use super::super::tests::{click, fixture, frame, text_center};
    use super::*;

    fn has_text(output: &egui::FullOutput, text: &str) -> bool {
        fn contains(shape: &egui::Shape, text: &str) -> bool {
            match shape {
                egui::Shape::Text(shape) => shape.galley.text() == text,
                egui::Shape::Vec(shapes) => shapes.iter().any(|shape| contains(shape, text)),
                _ => false,
            }
        }
        output
            .shapes
            .iter()
            .any(|shape| contains(&shape.shape, text))
    }

    #[test]
    fn page_inspector_shows_actual_auto_modes_and_three_contextual_tabs() {
        let mut project = Project::default();
        project.pages[0].canvas = CanvasMode::Auto;
        project.pages[1].canvas = CanvasMode::FixedWidth;
        project.pages[1].width = 975.0;
        let mut app = fixture(project);
        let ctx = egui::Context::default();
        let output = frame(&mut app, &ctx, vec![]);
        assert!(has_text(&output, "Auto"));
        assert!(has_text(&output, "页面尺寸"));
        assert!(!has_text(&output, "工程名称"));
        assert!(!has_text(&output, "画布预设"));
        let notes = text_center(&output, "说明");
        let output = click(&mut app, &ctx, notes);
        assert_eq!(app.tab, 2);
        assert!(has_text(&output, "页面说明"));
        let styles = text_center(&output, "样式");
        click(&mut app, &ctx, styles);
        app.page = 1;
        let output = frame(&mut app, &ctx, vec![]);
        assert!(has_text(&output, "975 × Auto"));
        assert!(!has_text(&output, "高度"));
    }

    #[test]
    fn responsive_page_and_widget_controls_are_editable_and_undoable() {
        let mut project = Project::blank();
        let id = project.allocate_id();
        project.pages[0]
            .widgets
            .push(Widget::new(id, WidgetKind::Panel, 240.0, 20.0));
        let mut app = fixture(project);
        let ctx = egui::Context::default();
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, text_center(&output, "1280 × 800"));
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, text_center(&output, "响应式宽度"));
        let output = frame(&mut app, &ctx, vec![]);
        assert_eq!(app.project.pages[0].canvas, CanvasMode::ResponsiveWidth);
        assert!(has_text(&output, "设计宽度"));
        assert!(has_text(&output, "最小宽度"));

        app.selected.insert(id);
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, text_center(&output, "横向伸展"));
        assert_eq!(
            app.project.pages[0].widgets[0].horizontal_layout,
            HorizontalLayout::STRETCH
        );
        app.undo();
        assert_eq!(
            app.project.pages[0].widgets[0].horizontal_layout,
            HorizontalLayout::LEFT
        );
        assert_eq!(app.project.pages[0].canvas, CanvasMode::ResponsiveWidth);
    }

    #[test]
    fn page_interaction_list_selects_component_and_updates_inspector() {
        let mut project = Project::blank();
        let id = project.allocate_id();
        let mut widget = Widget::new(id, WidgetKind::Button, 80.0, 80.0);
        widget.name = "navigate-item".into();
        widget.interaction.action = Action::Navigate;
        widget.interaction.target_page = Some(project.pages[0].id);
        project.pages[0].widgets.push(widget);
        let mut app = fixture(project);
        app.tab = 1;
        let ctx = egui::Context::default();
        let output = frame(&mut app, &ctx, vec![]);
        let item = text_center(&output, "navigate-item · 跳转页面");
        click(&mut app, &ctx, item);
        let output = frame(&mut app, &ctx, vec![]);
        assert_eq!(app.selected, BTreeSet::from([id]));
        assert!(has_text(&output, "点击时"));
        assert!(!has_text(&output, "位置与尺寸"));
        let output = click(&mut app, &ctx, text_center(&output, "说明"));
        assert!(has_text(&output, "元件说明"));
        let output = click(&mut app, &ctx, text_center(&output, "样式"));
        assert!(has_text(&output, "位置与尺寸"));
        assert!(!has_text(&output, "点击时"));
    }

    #[test]
    fn native_page_controls_do_not_change_preserved_source_properties() {
        let source = std::path::Path::new(
            r"D:\Program Files (x86)\Axure\Axure RP 11\DefaultSettings\Training\Quick Win.rp",
        );
        if !source.is_file() {
            return;
        }
        let original = std::fs::read(source).unwrap();
        let document = load_document(source).unwrap();
        let mut app = fixture(document.project);
        app.native = document.native;
        app.path = document.save_path;
        let before = app.project.clone();
        let ctx = egui::Context::default();
        let output = frame(&mut app, &ctx, vec![]);
        let output = click(&mut app, &ctx, text_center(&output, "添加自适应视图"));
        let output = click(&mut app, &ctx, text_center(&output, "图片填充"));
        assert_eq!(app.project, before);
        assert!(has_text(&output, "保留原设置"));
        assert_eq!(std::fs::read(source).unwrap(), original);
    }

    #[test]
    fn narrow_component_inspector_keeps_large_geometry_inside_panel() {
        let mut project = Project::blank();
        let id = project.allocate_id();
        let mut widget = Widget::new(id, WidgetKind::Rectangle, -100000.0, 100000.0);
        widget.width = 20000.0;
        widget.height = 20000.0;
        widget.name = "long component name that should stay inside the inspector".into();
        project.pages[0].widgets.push(widget);
        let mut app = fixture(project);
        app.selected.insert(id);
        let ctx = egui::Context::default();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(220.0, 950.0))),
                ..Default::default()
            },
            |ctx| app.inspector(ctx),
        );
        assert!(has_text(&output, "位置与尺寸"));
        let mut count = 0;
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape
                && ["-100000", "100000", "20000"].contains(&text.galley.text())
            {
                count += 1;
                assert!(text.pos.x >= 0.0 && text.pos.x + text.galley.size().x <= 220.0);
            }
        }
        assert!(count >= 3, "large geometry values must be visible");
    }
}
