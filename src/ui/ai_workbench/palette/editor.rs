use super::*;

impl App {
    pub(in crate::ui::ai_workbench) fn ai_palette_entry(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.ai.running.is_none(), egui::Button::new("项目配色"))
                .on_hover_text("用取色器、参考图片或文字准备项目配色")
                .clicked()
            {
                self.ai_open_palette();
            }
            if let Some(palette) = self.ai_confirmed_palette() {
                for swatch in palette.0.iter().take(6) {
                    preview_color(ui, swatch.hex).on_hover_text(swatch.label());
                }
            } else {
                ui.label(RichText::new("可选 · 设计前设定").size(11.0).color(MUTED));
            }
            if self.ai.palette.worker.is_some() {
                ui.label(RichText::new("配色处理中").size(11.0).color(PURPLE));
            }
        });
    }

    pub(in crate::ui) fn ai_palette_dialog(&mut self, ctx: &egui::Context) {
        if !self.ai.palette.open {
            return;
        }
        self.ai.embedded.hide();
        let active = self.ai_confirmed_palette().cloned().unwrap_or_default();
        let mut close = false;
        let mut confirm = false;
        let response = dialog_style::modal(ctx, "project-palette-editor").show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 88.0).clamp(280.0, 550.0));
            close = dialog_style::header(
                ui,
                dialog_style::Symbol::Settings,
                "项目配色预设",
                "编辑同一份草案，确认后用于后续 AI 设计",
            );
            let working = self.ai.palette.worker.is_some();
            let editable = !working && self.ai.palette.conflict.is_none();
            egui::ScrollArea::vertical()
                .id_salt("project-palette-content")
                .max_height((ctx.content_rect().height() - 215.0).max(120.0))
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.label(RichText::new("配色草案").strong());
                    ui.add_enabled_ui(editable, |ui| self.ai.palette.draw_swatches(ui));
                    self.ai.palette.draw_review(ui, &active);
                    ui.separator();
                    self.ai_palette_sources(ui, editable);
                });
            ui.separator();
            let valid = self.ai.palette.valid_palette();
            let changed = valid.as_ref().is_ok_and(|draft| draft != &active);
            let ready = editable && self.ai.running.is_none();
            ui.horizontal_wrapped(|ui| {
                let label = if valid.as_ref().is_ok_and(|draft| draft.0.is_empty()) {
                    "确认清除配色"
                } else {
                    "确认项目配色"
                };
                confirm = ui
                    .add_enabled(ready && changed, dialog_style::primary(label, PURPLE))
                    .clicked();
                close |= ui.add(dialog_style::secondary("稍后")).clicked();
            });
        });
        if confirm {
            if let Err(error) = self.ai_confirm_palette() {
                self.ai.palette.report(error, true);
            }
        } else if close || response.should_close() {
            self.ai.palette.open = false;
        }
    }

    fn ai_palette_sources(&mut self, ui: &mut egui::Ui, editable: bool) {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(editable, egui::Button::new("上传参考图片…"))
                .clicked()
            {
                if let Err(error) = self.ai.palette.valid_palette() {
                    self.ai.palette.report(error, true);
                } else {
                    self.ai_start_palette_images();
                }
            }
            if self.ai.palette.worker.is_some() {
                ui.spinner();
                if ui.small_button("停止").clicked() {
                    self.ai.palette.worker = None;
                    self.ai.palette.report("已停止配色任务。", false);
                }
            }
        });
        ui.add_enabled_ui(editable, |ui| self.ai.palette.draw_candidates(ui));
        ui.add_enabled_ui(editable && self.ai.palette.worker.is_none(), |ui| {
            ui.label(
                RichText::new("用文字生成或微调")
                    .strong()
                    .color(dialog_style::MUTED),
            );
            ui.add(
                egui::TextEdit::multiline(&mut self.ai.palette.text)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .hint_text(
                        RichText::new("例如：为旅行网站配色，或保留主色、让背景更暖")
                            .color(dialog_style::MUTED),
                    ),
            );
            if ui
                .add_enabled(
                    !self.ai.palette.text.trim().is_empty(),
                    egui::Button::new("生成配色建议"),
                )
                .clicked()
            {
                let text = self.ai.palette.text.to_lowercase();
                let research = !self.ai.palette.has_colors()
                    || [
                        "风格",
                        "主题",
                        "搜索",
                        "视觉参考",
                        "style",
                        "theme",
                        "search",
                    ]
                    .iter()
                    .any(|term| text.contains(term));
                if let Err(error) = self.ai_generate_palette(research) {
                    self.ai.palette.report(error, true);
                }
            }
        });
    }
}

impl PaletteEditor {
    fn draw_swatches(&mut self, ui: &mut egui::Ui) {
        let Some(rows) = &mut self.draft else { return };
        if rows.is_empty() {
            dialog_style::note(ui, "添加色块，或通过图片与描述生成第一版草案。");
        }
        let mut remove = None;
        let mut edited = false;
        for (i, row) in rows.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.add_enabled_ui(!row.locked, |ui| {
                        ui.horizontal(|ui| {
                            let parsed = HexColor::parse(&row.hex);
                            let mut rgb = parsed.map_or([128; 3], |hex| hex.0);
                            if ui.color_edit_button_srgb(&mut rgb).changed() {
                                row.hex = HexColor(rgb).to_string();
                                edited = true;
                            }
                            let text = egui::TextEdit::singleline(&mut row.hex)
                                .desired_width(85.0)
                                .char_limit(12)
                                .hint_text(RichText::new("#RRGGBB").color(dialog_style::MUTED));
                            let response = ui.add(text).on_hover_text("六位 HEX 色值");
                            edited |= response.changed();
                            if response.lost_focus()
                                && let Ok(hex) = HexColor::parse(&row.hex)
                            {
                                row.hex = hex.to_string();
                            }
                            let old = row.role;
                            egui::ComboBox::from_id_salt("color-role")
                                .width(82.0)
                                .selected_text(row.role.label())
                                .show_ui(ui, |ui| {
                                    for role in ColorRole::ALL {
                                        ui.selectable_value(&mut row.role, role, role.label());
                                    }
                                });
                            edited |= row.role != old;
                        });
                    });
                    edited |= ui
                        .checkbox(&mut row.locked, "锁定")
                        .on_hover_text("锁定后需先解锁才能修改或删除")
                        .changed();
                    if ui
                        .add_enabled(!row.locked, egui::Button::new("删除"))
                        .clicked()
                    {
                        remove = Some(i);
                    }
                });
            });
        }
        if let Some(index) = remove {
            rows.remove(index);
            edited = true;
        }
        if ui
            .add_enabled(rows.len() < MAX_SWATCHES, egui::Button::new("＋ 添加色块"))
            .clicked()
        {
            let (role, hex) = [
                (ColorRole::Primary, "#3478D7"),
                (ColorRole::Background, "#FAF8F4"),
                (ColorRole::Text, "#1A2335"),
                (ColorRole::Accent, "#F2A736"),
                (ColorRole::Secondary, "#64789D"),
                (ColorRole::Neutral, "#DDE1E8"),
            ]
            .into_iter()
            .find(|(role, _)| rows.iter().all(|row| row.role != *role))
            .unwrap_or((ColorRole::Neutral, "#DDE1E8"));
            rows.push(DraftRow {
                hex: hex.into(),
                role,
                locked: false,
            });
            edited = true;
        }
        if edited {
            self.changes.clear();
            self.report("草案已编辑，确认后应用。", false);
        }
        if let Ok(palette) = self.valid_palette()
            && let (Some(background), Some(text)) = (
                palette.first(ColorRole::Background),
                palette.first(ColorRole::Text),
            )
            && text.contrast(background) < 4.5
        {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!(
                        "文字与背景对比度 {:.1}:1",
                        text.contrast(background)
                    ))
                    .color(MUTED),
                );
                if ui.small_button("建议更清晰的文字色").clicked() {
                    let index = palette
                        .0
                        .iter()
                        .position(|row| row.role == ColorRole::Text)
                        .unwrap()
                        + 1;
                    let dark = HexColor([0, 0, 0]);
                    let light = HexColor([255, 255, 255]);
                    let hex = if dark.contrast(background) >= light.contrast(background) {
                        dark
                    } else {
                        light
                    };
                    if let Err(error) = self.apply(&[ColorEdit::Update {
                        index,
                        role: None,
                        hex: Some(hex),
                    }]) {
                        self.report(error, true);
                    }
                }
            });
        }
    }

    fn draw_candidates(&mut self, ui: &mut egui::Ui) {
        let Some(colors) = &self.candidates else {
            return;
        };
        let draft = self.valid_palette().ok();
        let mut add = None;
        ui.label(RichText::new("图片候选色").strong());
        for (index, row) in colors.suggested.0.iter().copied().enumerate() {
            ui.horizontal_wrapped(|ui| {
                preview_color(ui, row.hex);
                ui.label(row.label());
                if index >= colors.sampled {
                    dialog_style::note(ui, "建议中性色");
                }
                let available = draft.as_ref().is_some_and(|draft| {
                    draft.0.len() < MAX_SWATCHES
                        && draft
                            .0
                            .iter()
                            .all(|swatch| swatch.role != row.role || swatch.hex != row.hex)
                });
                if ui
                    .add_enabled(available, egui::Button::new("补充此色"))
                    .clicked()
                {
                    add = Some(ColorEdit::Add {
                        hex: row.hex,
                        role: row.role,
                    });
                }
            });
        }
        if let Some(edit) = add {
            self.basis = Basis {
                text: "使用上方图片候选色。".into(),
                ..Basis::default()
            };
            if let Err(error) = self.apply(&[edit]) {
                self.report(error, true);
            }
        }
        if ui
            .add_enabled(draft.is_some(), egui::Button::new("用候选替换未锁定色"))
            .clicked()
            && let Err(error) = self.replace_unlocked_from_image()
        {
            self.report(error, true);
        }
    }

    fn draw_review(&mut self, ui: &mut egui::Ui, active: &Palette) {
        if !self.notice.is_empty() {
            ui.label(RichText::new(&self.notice).color(if self.error {
                Color32::DARK_RED
            } else {
                MUTED
            }));
        }
        if !self.basis.text.is_empty() {
            ui.label(RichText::new("最近一次建议的参考依据").strong());
            dialog_style::note(ui, &self.basis.text);
            if !self.basis.rationale.is_empty() {
                dialog_style::note(ui, &self.basis.rationale);
            }
            for link in &self.basis.links {
                ui.hyperlink_to(link, link);
            }
        }
        if let Some(conflict) = &self.conflict {
            ui.label(RichText::new("锁定色需要单独确认").strong());
            for line in &conflict.lines {
                ui.label(line);
            }
            let mut choice = None;
            ui.horizontal_wrapped(|ui| {
                if ui.button("保留锁定色").clicked() {
                    choice = Some(false);
                }
                if ui.button("解锁并修改草案").clicked() {
                    choice = Some(true);
                }
            });
            if let Some(unlock) = choice {
                self.resolve_conflict(unlock);
            }
        }
        if !self.changes.is_empty() {
            egui::CollapsingHeader::new("本次建议改动")
                .default_open(true)
                .show(ui, |ui| {
                    for change in &self.changes {
                        ui.label(change);
                    }
                });
        }
        match self.valid_palette() {
            Ok(draft) if draft != *active => {
                ui.label(RichText::new("确认后生效的变更").strong());
                for change in summarize(active, &draft) {
                    dialog_style::note(ui, &change);
                }
            }
            Ok(_) => {}
            Err(error) => {
                ui.colored_label(Color32::DARK_RED, error);
            }
        }
    }
}

fn summarize(active: &Palette, draft: &Palette) -> Vec<String> {
    let mut added = draft.0.clone();
    let mut removed = Vec::new();
    for row in &active.0 {
        if let Some(index) = added.iter().position(|other| other == row) {
            added.remove(index);
        } else {
            removed.push(*row);
        }
    }
    let mut lines = Vec::new();
    for row in removed {
        if let Some(i) = added.iter().position(|other| other.role == row.role) {
            lines.push(format!("{} → {}", row.label(), added.remove(i).label()));
        } else {
            lines.push(format!("移除 {}", row.label()));
        }
    }
    lines.extend(added.into_iter().map(|row| format!("新增 {}", row.label())));
    lines
}

fn preview_color(ui: &mut egui::Ui, color: HexColor) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
    let [r, g, b] = color.0;
    ui.painter().rect(
        rect,
        3.0,
        Color32::from_rgb(r, g, b),
        Stroke::new(1.0, Color32::from_black_alpha(42)),
        StrokeKind::Inside,
    );
    response
}
