use super::*;

impl App {
    pub(super) fn header(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "windows")]
        self.windows_title_bar(ctx);
        #[cfg(not(target_os = "windows"))]
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(18, 10)),
            )
            .show(ctx, |ui| {
                if self.confirm.is_some() || self.editing.is_some() {
                    ui.disable();
                }
                ui.horizontal(|ui| {
                    if let Some(logo) = &self.logo {
                        ui.image((logo.id(), vec2(38.0, 38.0)));
                    }
                    ui.vertical(|ui| {
                        ui.label(RichText::new("rpshop").size(22.0).strong().color(INK));
                        ui.label(RichText::new("PROTOTYPE STUDIO").size(9.0).color(MUTED));
                    });
                    ui.add_space(24.0);
                    ui.label(
                        RichText::new("● 工作空间")
                            .color(Color32::from_rgb(71, 153, 130))
                            .size(12.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("▶  预览原型").color(Color32::WHITE),
                                )
                                .fill(PURPLE)
                                .min_size(vec2(118.0, 34.0)),
                            )
                            .clicked()
                        {
                            self.start_preview(ctx);
                        }
                        if ui.button("导出 HTML").clicked() {
                            self.export_file(false);
                        }
                        ui.add_space(14.0);
                        ui.label(
                            RichText::new(if self.project != self.saved {
                                "● 未保存"
                            } else if self.path.is_some() {
                                "已保存"
                            } else {
                                "未保存为文件"
                            })
                            .color(MUTED)
                            .size(11.0),
                        );
                        ui.add_sized(
                            [220.0, 24.0],
                            egui::Label::new(RichText::new(&self.project.name).strong()).truncate(),
                        )
                        .on_hover_text(&self.project.name);
                    });
                });
            });
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1.0, BORDER))
                    .inner_margin(egui::Margin::symmetric(18, 7)),
            )
            .show(ctx, |ui| {
                if self.confirm.is_some() || self.editing.is_some() {
                    ui.disable();
                }
                ui.horizontal(|ui| {
                    ui.menu_button("文件", |ui| {
                        if ui.button("新建工程       Ctrl+N").clicked() {
                            self.request_replace("new");
                            ui.close();
                        }
                        if ui.button("打开工程       Ctrl+O").clicked() {
                            self.request_replace("open");
                            ui.close();
                        }
                        if ui.button("保存              Ctrl+S").clicked() {
                            self.save_project(false);
                            ui.close();
                        }
                        if ui.button("另存为…").clicked() {
                            self.save_project(true);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("导出交互 HTML…").clicked() {
                            self.export_file(false);
                            ui.close();
                        }
                        if ui.button("导出当前页面 SVG…").clicked() {
                            self.export_file(true);
                            ui.close();
                        }
                        if ui.button("打开欢迎示例").clicked() {
                            self.request_replace("demo");
                            ui.close();
                        }
                    });
                    if ui
                        .add_enabled(!self.undo.is_empty(), egui::Button::new("撤销"))
                        .on_hover_text("撤销 Ctrl+Z")
                        .clicked()
                    {
                        self.undo();
                    }
                    if ui
                        .add_enabled(!self.redo.is_empty(), egui::Button::new("重做"))
                        .on_hover_text("重做 Ctrl+Y")
                        .clicked()
                    {
                        self.redo();
                    }
                    ui.separator();
                    if ui
                        .selectable_label(self.tool.is_none(), "↖  选择")
                        .clicked()
                    {
                        self.tool = None;
                    }
                    for (kind, label) in [
                        (WidgetKind::Rectangle, "□"),
                        (WidgetKind::Text, "T"),
                        (WidgetKind::Ellipse, "○"),
                        (WidgetKind::Line, "╱"),
                    ] {
                        if ui
                            .selectable_label(self.tool == Some(kind), label)
                            .on_hover_text(format!(
                                "绘制{}：拖动绘制，单击按默认尺寸放置",
                                kind.label()
                            ))
                            .clicked()
                        {
                            self.tool = Some(kind);
                        }
                    }
                    ui.separator();
                    let before = self.project.clone();
                    if ui
                        .add_enabled(!self.selected.is_empty(), egui::Button::new("复制"))
                        .clicked()
                    {
                        self.copy_to_system(ctx);
                    }
                    if ui.button("粘贴").on_hover_text("Ctrl+V").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                    }
                    if ui
                        .add_enabled(!self.selected.is_empty(), egui::Button::new("删除"))
                        .clicked()
                    {
                        self.delete_selection();
                    }
                    self.remember(before);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("?").clicked() {
                            self.help = true;
                        }
                        ui.menu_button("视图", |ui| {
                            for (label, command) in [
                                ("放大  Ctrl++", ViewCommand::Step(0.1)),
                                ("缩小  Ctrl+−", ViewCommand::Step(-0.1)),
                                ("100%  Ctrl+0", ViewCommand::Zoom(1.0)),
                                ("居中画布  Ctrl+1", ViewCommand::CenterPage),
                                ("适配选择  Ctrl+2", ViewCommand::FitSelection),
                                ("居中选择  Ctrl+3", ViewCommand::CenterSelection),
                                ("回到原点  Ctrl+9", ViewCommand::Origin),
                            ] {
                                if ui.button(label).clicked() {
                                    self.view.pending = Some(command);
                                    self.fit = false;
                                    ui.close();
                                }
                            }
                            ui.separator();
                            ui.label("Ctrl+滚轮缩放 · 空格拖动平移");
                        });
                        if ui.button("适应").clicked() {
                            self.fit = true;
                        }
                        egui::ComboBox::from_id_salt("zoom")
                            .selected_text(format!("{:.0}%", self.zoom * 100.0))
                            .width(65.0)
                            .show_ui(ui, |ui| {
                                for z in [
                                    0.1, 0.25, 0.5, 0.65, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 5.0, 10.0,
                                ] {
                                    if ui
                                        .selectable_label(
                                            (self.zoom - z).abs() < 0.01,
                                            format!("{:.0}%", z * 100.0),
                                        )
                                        .clicked()
                                    {
                                        self.view.pending = Some(ViewCommand::Zoom(z));
                                        self.fit = false;
                                    }
                                }
                            });
                        ui.checkbox(&mut self.snap, "吸附");
                        ui.checkbox(&mut self.grid, "网格");
                    });
                });
            });
    }

    pub(super) fn left_panel(&mut self, ctx: &egui::Context) {
        let Some(active) = self.sidebar_layout.active else {
            return;
        };
        egui::SidePanel::left("navigator")
            .resizable(true)
            .default_width(225.0)
            .width_range(195.0..=320.0)
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(14.0))
            .show(ctx, |ui| {
                if self.confirm.is_some() || self.editing.is_some() {
                    ui.disable();
                }
                ui.horizontal(|ui| {
                    let title = match active {
                        sidebar::LeftPane::Pages => RichText::new("目录").size(11.0).color(MUTED),
                        sidebar::LeftPane::Library => RichText::new("组件库").size(17.0).strong(),
                        sidebar::LeftPane::Layers => RichText::new("图层").size(11.0).color(MUTED),
                    };
                    ui.label(title);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").on_hover_text("收起侧栏").clicked() {
                            self.sidebar_layout.active = None;
                        }
                    });
                });
                if active != sidebar::LeftPane::Library {
                    ui.separator();
                    ui.add_space(8.0);
                }
                match active {
                    sidebar::LeftPane::Pages => self.pages_pane(ui),
                    sidebar::LeftPane::Library => self.library_pane(ui),
                    sidebar::LeftPane::Layers => self.layers_pane(ui),
                }
            });
    }

    pub(super) fn pages_pane(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("页面").strong().size(14.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(self.native.is_none(), egui::Button::new("＋").small())
                    .on_hover_text("新建页面")
                    .on_disabled_hover_text(
                        "当前 RP 原生编辑支持修改已有页面和组件，暂不支持新增页面",
                    )
                    .clicked()
                {
                    let id = self.project.allocate_id();
                    self.project.pages.push(Page {
                        id,
                        name: format!("页面 {}", self.project.pages.len() + 1),
                        width: 1280.0,
                        height: 800.0,
                        canvas: rpshop::model::CanvasMode::Fixed,
                        background: "#FFFFFF".into(),
                        widgets: vec![],
                        group_names: Default::default(),
                    });
                    if !self.project.page_tree.is_empty() {
                        self.project
                            .page_tree
                            .push(rpshop::model::PageTreeNode::Page {
                                page_id: id,
                                children: Vec::new(),
                            });
                    }
                    self.page = self.project.pages.len() - 1;
                    self.selected.clear();
                    self.fit = true;
                }
            });
        });
        ui.add_space(9.0);
        let navigation = page_tree::show(ui, &self.project, self.page, self.native.is_none());
        if let Some(i) = navigation.activate {
            self.page = i;
            self.selected.clear();
            self.fit = true;
        }
        if let Some(i) = navigation.remove {
            let p = self.project.pages.remove(i);
            page_tree::remove_entry(&mut self.project.page_tree, p.id);
            let ids: BTreeSet<_> = p.widgets.iter().map(|w| w.id).collect();
            for p2 in &mut self.project.pages {
                for w in &mut p2.widgets {
                    if w.interaction.target_page == Some(p.id)
                        || w.interaction
                            .target_widget
                            .is_some_and(|id| ids.contains(&id))
                    {
                        w.interaction = Default::default();
                    }
                }
            }
            self.project.prune_interaction_references();
            self.page = self.page.min(self.project.pages.len() - 1);
            self.selected.clear();
        }
    }

    pub(super) fn inspector(&mut self, ctx: &egui::Context) {
        self.inspector_panel(ctx);
    }
}
