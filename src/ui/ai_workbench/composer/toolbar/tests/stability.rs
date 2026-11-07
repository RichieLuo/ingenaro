use super::*;

fn render(
    app: &mut App,
    context: &egui::Context,
    width: f32,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    context.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 500.0))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.allocate_ui_with_layout(
                    vec2(width, CONTROL_HEIGHT),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        app.ai_composer_toolbar(ui);
                    },
                );
            });
        },
    )
}

#[test]
fn toolbar_anchor_and_effort_track_stay_fixed_across_all_effort_labels() {
    for dpi in [1.0, 1.25, 1.5, 2.0] {
        for width in [
            260.0, 330.0, 370.0, 420.0, 480.0, 520.0, 580.0, 600.0, 610.0, 620.0, 630.0, 640.0,
            800.0,
        ] {
            let mut app = app();
            let context = egui::Context::default();
            context.set_pixels_per_point(dpi);
            render(&mut app, &context, width, vec![]);
            let anchor = control_bounds(&context, "model");
            // Open the actual toolbar popup, not a detached slider-only fixture.
            let toggle = |pressed| egui::Event::PointerButton {
                pos: anchor.center(),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            render(
                &mut app,
                &context,
                width,
                vec![egui::Event::PointerMoved(anchor.center()), toggle(true)],
            );
            render(&mut app, &context, width, vec![toggle(false)]);
            for _ in 0..3 {
                render(&mut app, &context, width, vec![]);
            }
            let automatic = effort_bounds(&context, None);
            let max = effort_bounds(&context, Some(ai::ReasoningEffort::Max));
            for value in [
                Some(ai::ReasoningEffort::Low),
                Some(ai::ReasoningEffort::Medium),
                Some(ai::ReasoningEffort::High),
                Some(ai::ReasoningEffort::Xhigh),
                Some(ai::ReasoningEffort::Max),
                None,
            ] {
                app.ai.effort_override = value.map_or(
                    ai::ReasoningPreference::Auto,
                    ai::ReasoningPreference::Level,
                );
                for frame_index in 0..3 {
                    render(&mut app, &context, width, vec![]);
                    assert_eq!(
                        control_bounds(&context, "model"),
                        anchor,
                        "anchor moved: dpi={dpi}, width={width}, effort={value:?}, frame={frame_index}"
                    );
                    assert_eq!(
                        effort_bounds(&context, None),
                        automatic,
                        "track moved: dpi={dpi}, width={width}, effort={value:?}, frame={frame_index}"
                    );
                    assert_eq!(
                        effort_bounds(&context, Some(ai::ReasoningEffort::Max)),
                        max,
                        "track resized: dpi={dpi}, width={width}, effort={value:?}, frame={frame_index}"
                    );
                }
            }
            // Repeat with real pointer dragging in both directions, including idle
            // frames while held, so feedback cannot bounce between adjacent steps.
            let pointer = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            render(
                &mut app,
                &context,
                width,
                vec![
                    egui::Event::PointerMoved(automatic.center()),
                    pointer(automatic.center(), true),
                ],
            );
            for value in [
                Some(ai::ReasoningEffort::Low),
                Some(ai::ReasoningEffort::Medium),
                Some(ai::ReasoningEffort::High),
                Some(ai::ReasoningEffort::Xhigh),
                Some(ai::ReasoningEffort::Max),
                Some(ai::ReasoningEffort::Low),
                None,
            ] {
                let target = effort_bounds(&context, value).center();
                render(
                    &mut app,
                    &context,
                    width,
                    vec![egui::Event::PointerMoved(target)],
                );
                for held_frame in 0..3 {
                    render(&mut app, &context, width, vec![]);
                    assert_eq!(
                        app.ai.effort_override.resolve(None),
                        value,
                        "drag value bounced: dpi={dpi}, width={width}, held_frame={held_frame}"
                    );
                    assert_eq!(control_bounds(&context, "model"), anchor);
                    assert_eq!(effort_bounds(&context, None), automatic);
                    assert_eq!(effort_bounds(&context, Some(ai::ReasoningEffort::Max)), max);
                }
            }
            render(
                &mut app,
                &context,
                width,
                vec![pointer(automatic.center(), false)],
            );
            assert!(egui::Popup::is_any_open(&context));
        }
    }
}
