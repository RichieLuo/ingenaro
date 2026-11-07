use super::*;

#[test]
fn pointer_drag_across_multiple_frames_is_one_undoable_transaction() {
    let mut project = Project::blank();
    let id = add(&mut project, WidgetKind::Rectangle, 80.0, 80.0);
    let original = project.clone();
    let mut app = fixture(project);
    let ctx = egui::Context::default();
    let output = frame(&mut app, &ctx, Vec::new());
    let page = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::epaint::Shape::Rect(shape)
                if shape.fill == Color32::WHITE
                    && (shape.rect.width() - 640.0).abs() < 0.1
                    && (shape.rect.height() - 400.0).abs() < 0.1 =>
            {
                Some(shape.rect)
            }
            _ => None,
        })
        .expect("the document rectangle must be painted at the selected zoom");
    let start = page.min + vec2(50.0, 50.0);
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(start), pointer(start, true)],
    );
    let middle = start + vec2(40.0, 24.0);
    let _ = frame(&mut app, &ctx, vec![egui::Event::PointerMoved(middle)]);
    let end = start + vec2(48.0, 32.0);
    let _ = frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)]);
    assert!(
        app.undo.is_empty(),
        "intermediate drag frames must not fill history"
    );
    let _ = frame(&mut app, &ctx, vec![pointer(end, false)]);

    let widget = &app.project.pages[0].widgets[0];
    assert_eq!(widget.id, id);
    assert_eq!((widget.x, widget.y), (176.0, 144.0));
    assert_eq!(app.undo.len(), 1);
    assert!(app.drag.is_none());
    app.undo();
    assert_eq!(app.project, original);
    app.redo();
    assert_eq!(
        (
            app.project.pages[0].widgets[0].x,
            app.project.pages[0].widgets[0].y
        ),
        (176.0, 144.0)
    );
}

#[test]
fn line_rotation_handle_drag_is_visible_and_one_undoable_edit() {
    let mut project = Project::blank();
    let id = add(&mut project, WidgetKind::Line, 160.0, 160.0);
    let original = project.clone();
    let mut app = fixture(project);
    app.selected.insert(id);
    let ctx = egui::Context::default();
    let output = frame(&mut app, &ctx, Vec::new());
    let start = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Circle(circle)
                if circle.stroke.color == PURPLE && (circle.radius - 4.5).abs() < 0.01 =>
            {
                Some(circle.center)
            }
            _ => None,
        })
        .expect("selected lines must expose a rotation handle");
    let center = start + vec2(0.0, (24.0 + 1.0) * app.zoom);
    let end = center + vec2((24.0 + 1.0) * app.zoom, 0.0);
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(start), pointer(start, true)],
    );
    let _ = frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)]);
    assert!(app.undo.is_empty());
    let _ = frame(&mut app, &ctx, vec![pointer(end, false)]);
    let line = &app.project.pages[0].widgets[0];
    assert!(
        (line.rotation - 90.0).abs() < 0.01,
        "angle {}",
        line.rotation
    );
    assert_eq!(
        (line.x, line.y, line.width, line.height),
        (160.0, 160.0, 240.0, 2.0)
    );
    assert_eq!(app.undo.len(), 1);
    app.undo();
    assert_eq!(app.project, original);
    app.redo();
    assert!((app.project.pages[0].widgets[0].rotation - 90.0).abs() < 0.01);
}

#[test]
fn drag_constraints_preserve_relative_offsets_and_rotated_resize_anchor() {
    let mut project = Project::blank();
    let first = add(&mut project, WidgetKind::Rectangle, 13.0, 25.0);
    let second = add(&mut project, WidgetKind::Line, 248.0, 103.0);
    let before = project.pages[0].clone();
    let ids = BTreeSet::from([first, second]);
    apply_drag(
        &mut project.pages[0],
        &before,
        &ids,
        &DragMode::Move,
        [13.0, 7.0],
        true,
        false,
        true,
    );
    assert_eq!(
        (project.pages[0].widgets[0].x, project.pages[0].widgets[0].y),
        (29.0, 25.0)
    );
    assert_eq!(
        (project.pages[0].widgets[1].x, project.pages[0].widgets[1].y),
        (264.0, 103.0)
    );
    let mut old = before.clone();
    old.widgets[0].rotation = 37.0;
    let bounds = old.widgets[0].clone();
    let anchor = manipulation::handle_positions(&bounds)[0].1;
    let mut page = old.clone();
    apply_drag(
        &mut page,
        &old,
        &BTreeSet::from([first]),
        &DragMode::Resize {
            handle: Handle::SouthEast,
            bounds: Box::new(bounds),
        },
        [33.0, 29.0],
        true,
        false,
        false,
    );
    let resized = &page.widgets[0];
    let actual = manipulation::handle_positions(resized)[0].1;
    assert!((actual[0] - anchor[0]).abs() < 0.001 && (actual[1] - anchor[1]).abs() < 0.001);
    assert!(
        (resized.width / resized.height - old.widgets[0].width / old.widgets[0].height).abs()
            < 0.001
    );
}

#[test]
fn reflected_line_geometry_matches_canvas_hits_and_both_exports() {
    let mut project = Project::blank();
    let id = add(&mut project, WidgetKind::Line, 10.0, 20.0);
    let line = &mut project.pages[0].widgets[0];
    line.width = 120.0;
    line.height = 60.0;
    line.vector_points = Some(
        serde_json::json!([[{"Path":{"X":0,"Y":0},"HigherSegmentExists":true},{"Path":{"X":1,"Y":1},"HigherSegmentExists":false}]]),
    );
    let original = line.clone();
    let mut app = fixture(project);
    app.selected.insert(id);
    app.flip_selection(true);
    let line = &app.project.pages[0].widgets[0];
    assert!(manipulation::hit_test(line, [130.0, 20.0], 1.0));
    assert!(!manipulation::hit_test(line, [10.0, 20.0], 1.0));
    let html = ingenaro::export::html(&app.project).unwrap();
    let svg = ingenaro::export::svg(&app.project.pages[0]).unwrap();
    assert!(html.contains("x1=\"120\" y1=\"0\" x2=\"0\" y2=\"60\""));
    assert!(svg.contains("x1=\"130\" y1=\"20\" x2=\"10\" y2=\"80\""));
    assert_eq!(app.undo.len(), 1);
    app.undo();
    assert_eq!(app.project.pages[0].widgets[0], original);
}

#[test]
fn drawing_tool_drag_uses_document_coordinates_and_escape_cancels_creation() {
    let mut app = fixture(Project::blank());
    app.snap = false;
    app.tool = Some(WidgetKind::Rectangle);
    let ctx = egui::Context::default();
    let output = frame(&mut app, &ctx, Vec::new());
    let page = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(shape)
                if shape.fill == Color32::WHITE
                    && (shape.rect.width() - 640.0).abs() < 0.1
                    && (shape.rect.height() - 400.0).abs() < 0.1 =>
            {
                Some(shape.rect)
            }
            _ => None,
        })
        .unwrap();
    let start = page.min + vec2(50.0, 40.0);
    let end = start + vec2(75.0, 50.0);
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(start), pointer(start, true)],
    );
    let _ = frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)]);
    assert!(app.undo.is_empty());
    let _ = frame(&mut app, &ctx, vec![pointer(end, false)]);
    let widget = &app.project.pages[0].widgets[0];
    assert_eq!(
        (widget.x, widget.y, widget.width, widget.height),
        (100.0, 80.0, 150.0, 100.0)
    );
    assert_eq!(app.undo.len(), 1);
    let before = app.project.clone();
    app.tool = Some(WidgetKind::Line);
    let start = page.min + vec2(200.0, 160.0);
    let end = start + vec2(50.0, -30.0);
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(start), pointer(start, true)],
    );
    let _ = frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)]);
    assert_eq!(app.project.pages[0].widgets.len(), 2);
    let line = app.project.pages[0].widgets.last().unwrap();
    assert_eq!(
        ingenaro::vector_edit::line_endpoints(line),
        [[400.0, 320.0], [500.0, 260.0]]
    );
    let _ = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    let _ = frame(&mut app, &ctx, vec![pointer(end, false)]);
    assert_eq!(app.project, before);
    assert_eq!(app.undo.len(), 1);
}

#[test]
fn escape_clears_the_active_drawing_tool_without_closing_browser_preview() {
    let mut app = fixture(Project::blank());
    app.tool = Some(WidgetKind::Rectangle);
    let ctx = egui::Context::default();
    app.start_preview(&ctx);
    let _ = frame(&mut app, &ctx, Vec::new());
    let _ = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(app.browser_preview.is_some());
    assert!(app.tool.is_none());
}
