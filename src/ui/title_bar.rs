use super::*;

impl App {
    pub(super) fn windows_title_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .exact_height(window_chrome::TITLE_HEIGHT)
            .frame(egui::Frame::new().fill(window_chrome::title_fill(ctx)))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.spacing_mut().button_padding.y = 4.0;
                ui.spacing_mut().interact_size.y = 24.0;
                ui.horizontal(|ui| {
                    ui.set_height(window_chrome::TITLE_HEIGHT);
                    let brand = ui.allocate_ui_with_layout(
                        vec2(160.0, window_chrome::TITLE_HEIGHT),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add_space(16.0);
                            if let Some(logo) = &self.logo {
                                ui.add(
                                    egui::Image::new(logo)
                                        .fit_to_exact_size(vec2(28.0, 28.0))
                                        .alt_text("Ingenaro Logo"),
                                );
                            }
                            ui.add_space(8.0);
                            ui.label(RichText::new("Ingenaro").strong().size(18.0));
                        },
                    );
                    let brand_drag = ui.interact(
                        brand.response.rect,
                        ui.id().with("title-brand-drag"),
                        Sense::click_and_drag(),
                    );
                    window_chrome::drag_response(&brand_drag);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        window_chrome::controls(ui);
                        ui.add_space(16.0);
                        ui.add_enabled_ui(self.confirm.is_none() && self.editing.is_none(), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("▶  预览原型").color(Color32::WHITE),
                                    )
                                    .fill(PURPLE)
                                    .min_size(vec2(110.0, 24.0)),
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
                                [ui.available_width().clamp(0.0, 220.0), 24.0],
                                egui::Label::new(RichText::new(&self.project.name).strong())
                                    .truncate(),
                            )
                            .on_hover_text(&self.project.name);
                        });
                        window_chrome::drag_area(ui);
                    });
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests::fixture;

    fn render(
        app: &mut App,
        ctx: &egui::Context,
        width: f32,
        time: f64,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 720.0))),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| app.header(ctx),
        )
    }

    fn pointer(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn title_bar_keeps_sqlshop_geometry_and_existing_project_actions() {
        for width in [1100.0, 1500.0, 1900.0] {
            let ctx = egui::Context::default();
            let mut app = fixture(Project::blank());
            app.project.name = "A long prototype project title".repeat(12);
            app.logo = Some(ctx.load_texture(
                "test-caption-logo",
                egui::ColorImage::filled([2, 2], Color32::WHITE),
                egui::TextureOptions::LINEAR,
            ));
            let _ = render(&mut app, &ctx, width, 0.0, Vec::new());
            let output = render(&mut app, &ctx, width, 0.1, Vec::new());
            let panel = egui::containers::panel::PanelState::load(&ctx, egui::Id::new("header"))
                .expect("the title panel must be laid out");
            assert_eq!(panel.rect.height(), 34.0);
            assert_eq!(panel.rect.width(), width);
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|clipped| match &clipped.shape {
                    egui::Shape::Text(shape) => Some(shape.galley.text()),
                    _ => None,
                })
                .collect();
            assert_eq!(texts.iter().filter(|text| **text == "Ingenaro").count(), 1);
            assert!(!texts.contains(&"rpshop"));
            assert!(!texts.contains(&"PROTOTYPE STUDIO"));
            for required in ["▶  预览原型", "导出 HTML", "● 未保存", "文件"] {
                assert!(
                    texts.contains(&required),
                    "missing action/status: {required}"
                );
            }
            let image = output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    egui::Shape::Rect(rect)
                        if rect.brush.as_ref().is_some_and(|brush| {
                            brush.fill_texture_id == app.logo.as_ref().unwrap().id()
                        }) =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .expect("the title icon must be painted");
            assert_eq!(image.min.x, 16.0);
            assert_eq!(image.size(), vec2(28.0, 28.0));
            let title = output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    egui::Shape::Text(shape) if shape.galley.text() == "Ingenaro" => Some(shape),
                    _ => None,
                })
                .unwrap();
            assert_eq!(title.galley.job.sections[0].format.font_id.size, 18.0);
        }
    }

    #[test]
    fn brand_drag_and_double_click_emit_native_window_commands() {
        let ctx = egui::Context::default();
        let mut app = fixture(Project::blank());
        let position = Pos2::new(80.0, 17.0);
        let _ = render(&mut app, &ctx, 1100.0, 0.0, Vec::new());
        let pressed = render(
            &mut app,
            &ctx,
            1100.0,
            0.1,
            vec![egui::Event::PointerMoved(position), pointer(position, true)],
        );
        assert!(
            pressed.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::StartDrag)
        );
        let _ = render(&mut app, &ctx, 1100.0, 0.15, vec![pointer(position, false)]);
        let _ = render(&mut app, &ctx, 1100.0, 0.2, vec![pointer(position, true)]);
        let released = render(&mut app, &ctx, 1100.0, 0.25, vec![pointer(position, false)]);
        assert!(
            released.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::Maximized(true))
        );
    }

    #[test]
    fn native_close_request_preserves_dirty_project_until_confirmation() {
        use eframe::App as _;

        let ctx = egui::Context::default();
        let mut app = fixture(Project::blank());
        app.screenshot_mode = false;
        app.project.name = "unsaved prototype title".into();
        let expected = app.project.clone();
        let mut frame = eframe::Frame::_new_kittest();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 720.0))),
                viewports: [(
                    egui::ViewportId::ROOT,
                    egui::ViewportInfo {
                        events: vec![egui::ViewportEvent::Close],
                        ..Default::default()
                    },
                )]
                .into_iter()
                .collect(),
                ..Default::default()
            },
            |ctx| app.update(ctx, &mut frame),
        );
        assert!(
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::CancelClose)
        );
        assert_eq!(app.confirm.as_deref(), Some("close"));
        assert_eq!(app.project, expected);
    }
}
