use super::*;

impl App {
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        if !self.import_warnings.is_empty() && !self.screenshot_mode {
            let mut close = false;
            let response = dialog_style::modal(ctx, "import-report").show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 96.0).clamp(280.0, 520.0));
                close |= dialog_style::header(
                    ui,
                    dialog_style::Symbol::Info,
                    "原生文件打开结果",
                    "工程已载入，以下兼容范围需要留意",
                );
                dialog_style::section(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height((ctx.content_rect().height() - 330.0).clamp(100.0, 330.0))
                        .show(ui, |ui| {
                            for warning in &self.import_warnings {
                                ui.label(format!("• {warning}"));
                            }
                        });
                });
                ui.add_space(10.0);
                dialog_style::note(
                    ui,
                    "支持的编辑可按 Ctrl+S 保存回原格式，未编辑的原始数据保持不变。",
                );
                dialog_style::actions(ui, |ui| {
                    if ui.add(dialog_style::primary("知道了", PURPLE)).clicked() {
                        close = true;
                    }
                });
            });
            if close || response.should_close() {
                self.import_warnings.clear();
            }
        }
        if let Some(id) = self.editing {
            let mut close = false;
            let response = dialog_style::modal(ctx, "text-editor").show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 96.0).clamp(280.0, 480.0));
                close |= dialog_style::header(
                    ui,
                    dialog_style::Symbol::Text,
                    "编辑文本",
                    "直接修改所选元件，完成后返回画布",
                );
                if let Some(widget) = self.project.pages[self.page]
                    .widgets
                    .iter_mut()
                    .find(|widget| widget.id == id)
                {
                    dialog_style::section(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut widget.text)
                                .desired_width(f32::INFINITY)
                                .desired_rows(7),
                        );
                    });
                }
                dialog_style::actions(ui, |ui| {
                    if ui.add(dialog_style::primary("完成", PURPLE)).clicked() {
                        close = true;
                    }
                });
            });
            if close || response.should_close() {
                self.editing = None;
            }
        }
        if let Some(action) = self.confirm.clone() {
            let response = dialog_style::modal(ctx, "unsaved-project").show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 96.0).clamp(280.0, 464.0));
                if dialog_style::header(
                    ui,
                    dialog_style::Symbol::Save,
                    "保存当前修改？",
                    "当前工程包含尚未保存的编辑",
                ) {
                    self.cancel_replace();
                }
                dialog_style::section(ui, |ui| {
                    ui.label(RichText::new(&self.project.name).size(15.0).strong());
                    dialog_style::note(ui, "保存后再继续，或者放弃这些修改。取消会留在当前工程。");
                });
                dialog_style::actions(ui, |ui| {
                    if ui
                        .add(dialog_style::primary("保存并继续", PURPLE))
                        .clicked()
                        && self.save_project(false)
                    {
                        self.confirm = None;
                        if action == "close" {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        } else {
                            self.replace(&action);
                        }
                    }
                    if ui.add(dialog_style::danger("放弃修改")).clicked() {
                        self.confirm = None;
                        if action == "close" {
                            self.saved = self.project.clone();
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        } else {
                            self.replace(&action);
                        }
                    }
                    if ui.add(dialog_style::secondary("取消")).clicked() {
                        self.cancel_replace();
                    }
                });
            });
            if response.should_close() {
                self.cancel_replace();
            }
        }
        if self.help {
            let mut close = false;
            dialog_style::window(ctx, "关于 RPShop")
                .id(egui::Id::new("rpshop-about"))
                .default_width(590.0)
                .resizable(false)
                .show(ctx, |ui| {
                    close |= dialog_style::header(
                        ui,
                        dialog_style::Symbol::Info,
                        "关于 RPShop",
                        "把想法，变成可以点击的原型。",
                    );
                    dialog_style::section(ui, |ui| {
                        ui.label(RichText::new("RPShop · 原型工作室").size(17.0).strong());
                        dialog_style::note(
                            ui,
                            "原型设计、AI 画布编辑、真实交互预览，以及 HTML / SVG 导出。",
                        );
                    });
                    ui.add_space(10.0);
                    egui::ScrollArea::vertical()
                        .max_height((ctx.content_rect().height() - 360.0).clamp(160.0, 390.0))
                        .show(ui, |ui| {
                            egui::Grid::new("about-shortcuts")
                                .num_columns(2)
                                .spacing([22.0, 10.0])
                                .show(ui, |ui| {
                                    for (key, usage) in [
                                        ("Ctrl+N / O / S", "新建 / 打开 / 保存"),
                                        ("Ctrl+Z / Y", "撤销 / 重做"),
                                        ("Ctrl+C / X / V / D", "复制 / 剪切 / 粘贴 / 创建副本"),
                                        ("Ctrl+Shift+S / Z", "另存为 / 重做"),
                                        ("V / R / O / T / L", "选择 / 矩形 / 椭圆 / 文本 / 直线"),
                                        ("Ctrl+方向键", "调整尺寸；加 Shift 每次 10 像素"),
                                        ("Shift / Ctrl + 点击", "多选 / 增减选择"),
                                        ("Ctrl+Alt + 点击", "选择组内元件"),
                                        ("Ctrl + 滚轮 / + / −", "缩放画布"),
                                        ("滚轮 / Shift + 滚轮", "纵向 / 横向滚动画布"),
                                        ("空格+拖动 / 中键拖动", "平移画布"),
                                        (
                                            "Ctrl+0 / 1 / 2 / 3",
                                            "100% / 居中画布 / 适配选择 / 居中选择",
                                        ),
                                        ("Ctrl+G / Ctrl+Shift+G", "组合 / 解组"),
                                        ("Shift + 拖动 / 旋转", "主轴移动 / 15° 角度吸附"),
                                        ("Shift / Alt + 缩放", "等比缩放 / 从中心缩放"),
                                        ("方向键 / Shift+方向键", "移动 1 / 10 像素"),
                                        ("Delete / Backspace", "删除选中的未锁定元件"),
                                        ("F5 / Esc", "预览当前页 / 取消绘图工具"),
                                    ] {
                                        ui.label(RichText::new(key).strong());
                                        ui.label(usage);
                                        ui.end_row();
                                    }
                                });
                        });
                    dialog_style::actions(ui, |ui| {
                        if ui.add(dialog_style::primary("完成", PURPLE)).clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.help = false;
            }
        }
    }
}
