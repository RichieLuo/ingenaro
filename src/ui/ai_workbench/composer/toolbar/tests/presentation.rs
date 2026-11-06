use super::*;

#[test]
fn model_suffix_is_visible_and_keeps_the_same_button_width() {
    let mut app = app();
    let id = app.ai.selected_engine.clone();
    app.ai.catalogs.get_mut(&id).unwrap().catalog.models[0].name = "Opus 5.5".into();
    let context = egui::Context::default();
    toolbar_frame(&mut app, &context, 640.0);
    let rect = control_bounds(&context, "model");
    for value in [
        None,
        Some(ai::ReasoningEffort::Low),
        Some(ai::ReasoningEffort::High),
        Some(ai::ReasoningEffort::Xhigh),
        Some(ai::ReasoningEffort::Max),
    ] {
        app.ai.effort_override = value;
        let output = toolbar_frame(&mut app, &context, 640.0);
        let suffix = text_center(&output, &format!("Opus 5.5({})", effort_short_label(value)));
        assert!(rect.contains(suffix));
        assert_eq!(control_bounds(&context, "model"), rect);
    }
}

#[test]
fn model_and_effort_share_one_galley_with_space_before_the_chevron() {
    for dpi in [1.0, 1.25, 1.5, 2.0] {
        let context = egui::Context::default();
        crate::ui::setup_style(&context);
        context.set_pixels_per_point(dpi);
        for width in [86.0, 96.0, 114.0, 154.0, 182.0] {
            let _ = context.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    for name in ["Opus", "Opus 5.5", "很长的模型名称 with details"] {
                        for effort in [
                            None,
                            Some(ai::ReasoningEffort::Low),
                            Some(ai::ReasoningEffort::Max),
                        ] {
                            let budget = width - 50.0;
                            let galley = controls::model_galley(
                                ui.painter(),
                                name,
                                effort,
                                budget,
                                SECONDARY,
                            );
                            assert_eq!(galley.rows.len(), 1);
                            assert!(
                                galley
                                    .text()
                                    .ends_with(&format!("({})", effort_short_label(effort)))
                            );
                            assert!(
                                galley.size().x.max(galley.mesh_bounds.right()) <= budget + 0.1,
                                "dpi={dpi}, width={width}, text={:?}, size={:?}, ink={:?}",
                                galley.text(),
                                galley.size(),
                                galley.mesh_bounds
                            );
                        }
                    }
                });
            });
        }
    }
}

#[test]
fn selecting_a_new_model_updates_supported_efforts_without_closing_the_menu() {
    let mut app = app();
    let id = app.ai.selected_engine.clone();
    app.ai
        .catalogs
        .get_mut(&id)
        .unwrap()
        .catalog
        .models
        .push(ai::ModelInfo {
            id: "quick".into(),
            name: "Quick".into(),
            description: String::new(),
            is_default: false,
            reasoning_efforts: vec!["low".into()],
        });
    app.ai.effort_override = Some(ai::ReasoningEffort::High);
    let context = egui::Context::default();
    let output = open_modes(&mut app, &context, "model");
    click(&mut app, &context, text_center(&output, "Quick"));
    assert_eq!(app.ai.model_override.as_deref(), Some("quick"));
    assert_eq!(app.ai.effort_override, None);
    assert!(egui::Popup::is_any_open(&context));
    let output = frame(&mut app, &context, vec![]);
    text_center(&output, "思考（自动）");
    let low = effort_bounds(&context, Some(ai::ReasoningEffort::Low)).center();
    click(&mut app, &context, low);
    assert_eq!(app.ai.effort_override, Some(ai::ReasoningEffort::Low));
    app.ai.prompt = "test".into();
    let submission = app.ai_capture_submission().unwrap();
    assert_eq!(submission.engine.model, "quick");
    assert_eq!(
        submission.engine.reasoning_effort,
        Some(ai::ReasoningEffort::Low)
    );
}

#[test]
fn permission_marks_match_the_menu_icon_for_every_mode() {
    fn paint(mode: permissions::PermissionMode, toolbar: bool) -> Vec<egui::epaint::ClippedShape> {
        let context = egui::Context::default();
        context
            .run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let rect = Rect::from_center_size(Pos2::new(30.0, 30.0), vec2(18.0, 18.0));
                    if toolbar {
                        controls::paint_mark(ui.painter(), rect, Mark::Permission(mode), INK);
                    } else {
                        modes::mode_icon(ui.painter(), rect, mode, INK);
                    }
                });
            })
            .shapes
    }
    let modes = [
        permissions::PermissionMode::Ask,
        permissions::PermissionMode::ReadOnly,
        permissions::PermissionMode::Full,
    ];
    for mode in modes {
        assert_eq!(paint(mode, true), paint(mode, false));
    }
    assert_ne!(paint(modes[0], true), paint(modes[1], true));
    assert_ne!(paint(modes[1], true), paint(modes[2], true));
}

#[test]
fn selected_mode_and_effort_fill_use_theme_color_without_changing_text_selection() {
    let mut app = app();
    app.ai.effort_override = Some(ai::ReasoningEffort::High);
    let context = egui::Context::default();
    crate::ui::setup_style(&context);
    open_modes(&mut app, &context, "permission");
    // Inspect settled colors rather than the popup's translucent fade-in frames.
    for _ in 0..20 {
        frame(&mut app, &context, vec![]);
    }
    let output = frame(&mut app, &context, vec![]);
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Rect(rect) if rect.fill == PURPLE && rect.rect.height() == 54.0)));
    egui::Popup::close_all(&context);
    open_modes(&mut app, &context, "model");
    for _ in 0..20 {
        frame(&mut app, &context, vec![]);
    }
    let output = frame(&mut app, &context, vec![]);
    let high = effort_bounds(&context, Some(ai::ReasoningEffort::High)).center();
    let start = effort_bounds(&context, None).center();
    let track_fill = output.shapes.iter().find(|shape| matches!(&shape.shape, egui::epaint::Shape::Rect(rect) if rect.fill == PURPLE && rect.rect.height() == 20.0)).expect("theme-colored filled track segment");
    assert!(track_fill.clip_rect.contains(start));
    assert!((track_fill.clip_rect.right() - high.x).abs() < 0.1);
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Circle(circle) if circle.fill == PURPLE && circle.center == high)));
    let model_row = context
        .data(|data| {
            data.get_temp::<Rect>(egui::Id::new((
                "ai-model-choice-test",
                "Opus 5.5 with an intentionally long configured display name  ·  CLI 推荐",
            )))
        })
        .unwrap();
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Rect(rect) if rect.fill == models::MODEL_SELECTION_FILL && rect.rect.intersects(model_row))));
    assert_eq!(
        context.style().visuals.selection,
        egui::Visuals::light().selection
    );
}
