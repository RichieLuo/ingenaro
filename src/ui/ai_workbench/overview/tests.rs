use super::*;
use crate::ui::tests::fixture;
use serde_json::json;

fn overview_app(count: usize) -> App {
    let mut project = Project::blank();
    project.pages[0].name = "设计页面 1".into();
    for index in 1..count {
        let mut page = project.pages[0].clone();
        page.id = project.allocate_id();
        page.name = format!("设计页面 {}", index + 1);
        project.pages.push(page);
    }
    let mut app = fixture(project);
    let updates = app
        .project
        .pages
        .iter()
        .enumerate()
        .map(|(index, page)| {
            json!({"page_id":page.id,"status":"working","label":format!("真实任务 {}", index + 1)})
        })
        .collect::<Vec<_>>();
    app.ai
        .page_activity
        .apply(
            &app.project,
            page_activity::Owner::Builtin,
            &json!({"updates":updates}),
        )
        .unwrap();
    app
}

fn overview_frame(
    app: &mut App,
    ctx: &egui::Context,
    size: egui::Vec2,
    time: f64,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            time: Some(time),
            events,
            ..Default::default()
        },
        |ctx| app.ai_page_overview(ctx),
    )
}

fn text_bounds(output: &egui::FullOutput, text: &str) -> Vec<Rect> {
    fn find(shape: &egui::Shape, text: &str, found: &mut Vec<Rect>) {
        match shape {
            egui::Shape::Text(shape) if shape.galley.text() == text => {
                found.push(Rect::from_min_size(shape.pos, shape.galley.size()));
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    find(shape, text, found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for shape in &output.shapes {
        find(&shape.shape, text, &mut found);
    }
    found
}

fn complete(app: &mut App, id: u64) {
    app.ai
        .page_activity
        .apply(
            &app.project,
            page_activity::Owner::Builtin,
            &json!({"updates":[{"page_id":id,"status":"completed"}]}),
        )
        .unwrap();
    app.ai_sync_activity_view();
}

#[test]
fn three_active_pages_paint_exactly_three_cards_with_real_work_labels() {
    let mut app = overview_app(3);
    let mut inactive = app.project.pages[0].clone();
    inactive.id = app.project.allocate_id();
    inactive.name = "不在任务中的页面".into();
    app.project.pages.push(inactive);
    let before = app.project.clone();
    let context = egui::Context::default();
    let output = overview_frame(&mut app, &context, vec2(1440.0, 960.0), 0.0, vec![]);
    for index in 1..=3 {
        assert_eq!(text_bounds(&output, &format!("设计页面 {index}")).len(), 1);
        assert_eq!(text_bounds(&output, &format!("真实任务 {index}")).len(), 1);
    }
    assert_eq!(text_bounds(&output, "进行中 3 个 · 已完成 0 个").len(), 1);
    assert!(text_bounds(&output, "不在任务中的页面").is_empty());
    assert_eq!(app.project, before);
    assert!(app.undo.is_empty());
    assert!(app.redo.is_empty());
}

#[test]
fn waiting_pages_keep_requesting_animation_frame_repaints() {
    let mut app = overview_app(2);
    let context = egui::Context::default();
    // Let egui's startup repaints settle before checking the requested delay.
    for index in 0..5 {
        let time = 2.0 + f64::from(index) / 60.0;
        let output = overview_frame(&mut app, &context, vec2(1440.0, 960.0), time, vec![]);
        if index == 4 {
            assert!(!app.ai_reveal.is_active(time));
            assert!(
                output.viewport_output[&egui::ViewportId::ROOT].repaint_delay
                    <= Duration::from_millis(16),
                "waiting for new page content must not slow down the activity indicators"
            );
        }
    }
}

#[test]
fn header_and_every_card_rotate_between_frames_without_widget_changes() {
    fn activity_arcs(output: &egui::FullOutput) -> Vec<Vec<Pos2>> {
        fn collect(shape: &egui::Shape, arcs: &mut Vec<Vec<Pos2>>) {
            match shape {
                egui::Shape::Path(path)
                    if !path.closed
                        && path.points.len() == 12
                        && path.stroke == Stroke::new(1.3, PURPLE).into() =>
                {
                    arcs.push(path.points.clone());
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, arcs);
                    }
                }
                _ => {}
            }
        }
        let mut arcs = Vec::new();
        for shape in &output.shapes {
            collect(&shape.shape, &mut arcs);
        }
        arcs
    }

    let mut app = overview_app(2);
    let before = app.project.clone();
    let context = egui::Context::default();
    let first = overview_frame(&mut app, &context, vec2(1440.0, 960.0), 2.1, vec![]);
    let second = overview_frame(
        &mut app,
        &context,
        vec2(1440.0, 960.0),
        2.1 + 1.0 / 60.0,
        vec![],
    );
    let first_arcs = activity_arcs(&first);
    let second_arcs = activity_arcs(&second);
    assert_eq!(first_arcs.len(), 3, "one header and two card indicators");
    assert_eq!(second_arcs.len(), first_arcs.len());
    for (first, second) in first_arcs.iter().zip(&second_arcs) {
        assert_ne!(
            first, second,
            "each indicator must move within the same second"
        );
    }
    assert_eq!(app.project, before);
    assert!(app.undo.is_empty());
    assert!(app.redo.is_empty());
}

#[test]
fn completion_preserves_card_positions_and_keeps_all_results_visible() {
    let mut app = overview_app(3);
    let ids = app.ai.page_activity.ids();
    let before = app.project.clone();
    let context = egui::Context::default();
    let size = vec2(1440.0, 960.0);
    let initial = overview_frame(&mut app, &context, size, 0.0, vec![]);
    for (finished, id) in [ids[1], ids[0], ids[2]].into_iter().enumerate() {
        complete(&mut app, id);
        assert!(app.ai_page_overview_visible());
        let output = overview_frame(&mut app, &context, size, finished as f64 + 1.0, vec![]);
        for index in 1..=3 {
            let name = format!("设计页面 {index}");
            assert_eq!(text_bounds(&output, &name), text_bounds(&initial, &name));
        }
        assert_eq!(text_bounds(&output, "已完成").len(), finished + 1);
        assert_eq!(
            text_bounds(&output, "多页设计已完成").len(),
            usize::from(finished == 2),
        );
    }
    assert_eq!(app.project, before);
    assert!(app.undo.is_empty());
}

#[test]
fn clicking_finished_thumbnail_opens_its_canvas_without_interrupting_other_work() {
    let mut app = overview_app(3);
    app.selected.insert(1234);
    let before = app.project.clone();
    let saved = app.saved.clone();
    let ids = app.ai.page_activity.ids();
    complete(&mut app, ids[2]);
    let context = egui::Context::default();
    let output = overview_frame(&mut app, &context, vec2(1440.0, 960.0), 0.0, vec![]);
    let position = text_bounds(&output, "设计页面 3")[0].center();
    for (time, pressed) in [(0.1, true), (0.2, false)] {
        let _ = overview_frame(
            &mut app,
            &context,
            vec2(1440.0, 960.0),
            time,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    assert_eq!(app.project.pages[app.page].id, ids[2]);
    assert_eq!(app.ai.page_activity.last_focus(), Some(ids[2]));
    assert_eq!(app.ai.page_activity.ids(), ids);
    assert!(!app.ai_page_overview_visible());
    assert!(app.ai.page_activity.has_work());
    for &id in &ids[..2] {
        complete(&mut app, id);
    }
    assert!(!app.ai_page_overview_visible());
    assert_eq!(app.project.pages[app.page].id, ids[2]);
    assert!(app.selected.is_empty());
    assert_eq!(app.project, before);
    assert_eq!(app.saved, saved);
    assert!(app.undo.is_empty());
    assert!(app.redo.is_empty());

    use crate::ui::tests::{click, frame, text_center};
    let context = egui::Context::default();
    let output = frame(&mut app, &context, vec![]);
    let overview = text_center(&output, "多页总览");
    click(&mut app, &context, overview);
    assert!(app.ai_page_overview_visible());
}

#[test]
fn landscape_mobile_and_auto_previews_keep_native_output_ratio_and_origin() {
    use ingenaro::model::CanvasMode;
    let mut page = Project::blank().pages.remove(0);
    let stage = Rect::from_min_size(Pos2::new(12.0, 42.0), vec2(320.0, 220.0));
    for (width, height) in [(1440.0, 900.0), (390.0, 844.0)] {
        page.width = width;
        page.height = height;
        let (rect, zoom) = thumbnail_geometry(&page, stage);
        assert!((rect.width() / rect.height() - width / height).abs() < 0.0001);
        assert!((rect.width() - width * zoom).abs() < 0.001);
        assert!(stage.contains_rect(rect));
        assert_eq!(rect.center(), stage.center());
    }
    page.canvas = CanvasMode::Auto;
    let mut negative = Widget::new(2, WidgetKind::Rectangle, -200.0, -100.0);
    negative.width = 50.0;
    negative.height = 50.0;
    let mut visible = Widget::new(3, WidgetKind::Rectangle, 200.0, 100.0);
    visible.width = 400.0;
    visible.height = 300.0;
    page.widgets = vec![negative, visible];
    let (rect, zoom) = thumbnail_geometry(&page, stage);
    assert_eq!(page.size_for_output(), [600.0, 400.0]);
    assert!((rect.width() / rect.height() - 1.5).abs() < 0.0001);
    // Negative coordinates stay before the page origin, like native output.
    assert!((rect.min + vec2(-200.0, -100.0) * zoom).x < rect.left());
}

#[test]
fn decoded_images_and_reveal_are_clipped_to_their_own_page() {
    let mut app = overview_app(3);
    app.project.pages[0].width = 390.0;
    app.project.pages[0].height = 844.0;
    let image_id = app.project.allocate_id();
    let mut image = Widget::new(image_id, WidgetKind::Image, -100.0, -100.0);
    image.width = 1000.0;
    image.height = 2000.0;
    app.project.pages[0].widgets.push(image);
    let context = egui::Context::default();
    let texture = context.load_texture(
        "overview-clip-image",
        egui::ColorImage::filled([2, 2], Color32::from_rgb(18, 130, 171)),
        egui::TextureOptions::LINEAR,
    );
    let texture_id = texture.id();
    app.textures.insert(image_id, texture);
    let before = app.project.clone();
    let output = overview_frame(&mut app, &context, vec2(1440.0, 960.0), 0.0, vec![]);
    let clips = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id == texture_id => Some(shape.clip_rect),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(clips.len(), 1, "the native image should be painted once");
    let clip = clips[0];
    assert!((clip.width() / clip.height() - 390.0 / 844.0).abs() < 0.0001);
    let title = text_bounds(&output, "设计页面 1")[0];
    let status = text_bounds(&output, "真实任务 1")[0];
    assert!(clip.top() > title.bottom());
    assert!(clip.bottom() < status.top());
    assert_eq!(app.project, before);
    assert!(app.undo.is_empty());

    let mut empty = before.clone();
    empty.pages[0].widgets.clear();
    app.ai_reveal.record(&empty, &before, 1.0);
    let animated = overview_frame(&mut app, &context, vec2(1440.0, 960.0), 1.26, vec![]);
    let image = animated.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Mesh(mesh) if mesh.texture_id == texture_id => {
            Some((shape.clip_rect, &mesh.vertices))
        }
        _ => None,
    });
    let (animated_clip, vertices) =
        image.expect("the image should reveal through the native renderer");
    assert_eq!(animated_clip, clip);
    assert!(
        vertices
            .iter()
            .all(|vertex| (1..255).contains(&vertex.color.a()))
    );
    assert!(app.ai_reveal.is_active(1.26));
    assert_eq!(app.project, before);
    assert!(app.undo.is_empty());
}

#[test]
fn responsive_grid_represents_up_to_128_pages_without_horizontal_overflow() {
    for (width, expected_columns) in [(360.0, 1), (720.0, 2), (1200.0, 3)] {
        let layout = OverviewLayout::new(vec2(width, 740.0), 128);
        assert_eq!(layout.columns, expected_columns);
        assert_eq!(layout.rows, 128_usize.div_ceil(expected_columns));
        let grid = Rect::from_min_size(Pos2::ZERO, vec2(width, layout.height()));
        for index in 0..128 {
            assert!(
                grid.expand(0.001)
                    .contains_rect(layout.card(Pos2::ZERO, index))
            );
        }
        assert!(layout.height() > 740.0, "large work sets must scroll");
    }
}
