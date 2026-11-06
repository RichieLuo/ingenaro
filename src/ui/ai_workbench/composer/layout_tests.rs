use super::*;
use crate::ui::tests::{fixture, frame};
use eframe::App as _;

#[test]
fn queued_images_keep_footer_and_single_row_controls_inside_short_windows() {
    for height in [600.0, 720.0, 950.0] {
        for width in [900.0, 1200.0, 1500.0] {
            let mut app = fixture(Project::default());
            app.ai.visible = true;
            let engine = ai::EngineConfig::new(ai::EngineKind::Claude);
            app.ai.selected_engine = engine.id.clone();
            app.ai.settings.engines.push(engine);
            app.ai.last_project_key = app.ai_project_key();
            for index in 0..16 {
                app.ai.prompt = format!("排队需求{index}");
                let entry = app.ai_capture_submission().unwrap();
                app.ai.queue.push(entry).unwrap();
            }
            app.ai.queue.paused = true;
            app.ai
                .images
                .push(ai::ImageAttachment::from_rgba("图片", 1, 1, &[0; 4]).unwrap());
            let context = egui::Context::default();
            crate::ui::setup_style(&context);
            for _ in 0..3 {
                let output = context.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, height))),
                        ..Default::default()
                    },
                    |ctx| app.update(ctx, &mut eframe::Frame::_new_kittest()),
                );
                let send = toolbar::control_bounds(&context, "send");
                let footer =
                    crate::ui::tests::text_center(&output, "Enter 发送 · Shift+Enter 换行");
                assert!(
                    footer.y < height - 24.0,
                    "{width}x{height}: footer={footer:?}"
                );
                assert!(
                    send.max.y < height - 24.0,
                    "{width}x{height}: send={send:?}"
                );
                for name in ["engine", "permission", "model"] {
                    let rect = toolbar::control_bounds(&context, name);
                    assert!((rect.center().y - send.center().y).abs() < 1.0);
                }
            }
        }
    }
}

#[test]
fn composer_contents_do_not_move_on_hover_focus_or_blur() {
    fn render(
        app: &mut App,
        context: &egui::Context,
        width: f32,
        events: Vec<egui::Event>,
    ) -> (Pos2, Rect, [Rect; 4], Pos2) {
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 500.0))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.set_width(width);
                    app.ai_composer(ui);
                });
            },
        );
        (
            crate::ui::tests::text_center(&output, "描述你的设计，选中元件会自动引用，Enter 发送"),
            context.data(|data| {
                data.get_temp(egui::Id::new("ai-reference-remove-test"))
                    .expect("reference remove button rendered")
            }),
            ["engine", "permission", "model", "send"]
                .map(|key| toolbar::control_bounds(context, key)),
            crate::ui::tests::text_center(&output, "Enter 发送 · Shift+Enter 换行"),
        )
    }

    for pixels_per_point in [1.0, 1.25, 1.5, 2.0] {
        let mut app = fixture(Project::blank());
        let context = egui::Context::default();
        crate::ui::setup_style(&context);
        context.set_pixels_per_point(pixels_per_point);
        // Include the compact toolbar breakpoint as well as normal sidebar widths.
        for width in [260.0, 332.0, 396.0, 656.0] {
            render(&mut app, &context, width, vec![egui::Event::PointerGone]);
            let idle = render(&mut app, &context, width, vec![]);
            let editor_id = app.ai.prompt_editor_id.unwrap();
            for (state, focused, hovered) in [
                ("hover", false, true),
                ("focus", true, true),
                ("focus without hover", true, false),
                ("blur", false, false),
            ] {
                context.memory_mut(|memory| {
                    if focused {
                        memory.request_focus(editor_id);
                    } else {
                        memory.surrender_focus(editor_id);
                    }
                });
                let event = if hovered {
                    egui::Event::PointerMoved(idle.0)
                } else {
                    egui::Event::PointerGone
                };
                for frame in 0..3 {
                    let geometry = render(&mut app, &context, width, vec![event.clone()]);
                    assert_eq!(
                        context.memory(|memory| memory.has_focus(editor_id)),
                        focused
                    );
                    assert_eq!(
                        geometry, idle,
                        "{state}, frame={frame}, width={width}, pixels_per_point={pixels_per_point}"
                    );
                }
            }
        }
    }
}

#[test]
fn composer_height_stays_stable_across_idle_frames_and_footer_is_visible() {
    let mut app = fixture(Project::default());
    app.ai.visible = true;
    let context = egui::Context::default();
    for _ in 0..20 {
        frame(&mut app, &context, vec![]);
    }
    assert_eq!(app.ai.composer_height, 222.0);
    let output = frame(&mut app, &context, vec![]);
    let footer = crate::ui::tests::text_center(&output, "Enter 发送 · Shift+Enter 换行");
    assert!(footer.y < 908.0 && footer.y > 650.0, "footer: {footer:?}");
}

#[test]
fn composer_resize_tracks_the_pointer_in_both_directions() {
    for height in [720.0, 950.0] {
        let mut app = fixture(Project::blank());
        app.ai.visible = true;
        let context = egui::Context::default();
        crate::ui::setup_style(&context);
        let mut render = |events| {
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1500.0, height))),
                    events,
                    ..Default::default()
                },
                |ctx| app.update(ctx, &mut eframe::Frame::_new_kittest()),
            );
            let grip = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Rect(shape) if shape.rect.size() == RESIZE_GRIP_SIZE => {
                        Some(shape.rect.center())
                    }
                    _ => None,
                })
                .expect("composer resize grip was painted");
            let footer = crate::ui::tests::text_center(&output, "Enter 发送 · Shift+Enter 换行");
            let send = toolbar::control_bounds(&context, "send");
            (grip, footer.y, send.center().y)
        };
        let (grip, footer_y, send_y) = render(vec![]);
        let grab = grip + vec2(0.0, 2.0);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        render(vec![egui::Event::PointerMoved(grab)]);
        render(vec![button(grab, true)]);

        for offset in [-64.0, -350.0, -44.0, 32.0] {
            let (moved_grip, moved_footer_y, moved_send_y) =
                render(vec![egui::Event::PointerMoved(grab + vec2(0.0, offset))]);
            if offset == -350.0 {
                assert!(moved_grip.y > grip.y + offset && moved_grip.y < grip.y - 64.0);
            } else {
                assert!(
                    (moved_grip.y - grip.y - offset).abs() < 1.0,
                    "window height {height}, pointer offset {offset}, grip moved {}",
                    moved_grip.y - grip.y,
                );
            }
            assert!((moved_footer_y - footer_y).abs() < 1.0);
            assert!((moved_send_y - send_y).abs() < 1.0);
        }

        let released = render(vec![button(grab + vec2(0.0, 32.0), false)]);
        let idle = render(vec![egui::Event::PointerMoved(grab)]);
        assert_eq!(released, idle);
    }
}
