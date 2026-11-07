use super::*;

fn page_and_viewport(output: &egui::FullOutput, app: &App) -> (Rect, Rect) {
    let page = &app.project.pages[app.page];
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.fill == Color32::WHITE
                    && (rect.rect.width() - page.width * app.zoom).abs() < 0.1
                    && (rect.rect.height() - page.height * app.zoom).abs() < 0.1 =>
            {
                let origin = rect.rect.min - app.view.pan;
                Some((rect.rect, Rect::from_min_max(origin, shape.clip_rect.max)))
            }
            _ => None,
        })
        .expect("the page and its canvas clipping rectangle must be painted")
}

fn ruler_strips(output: &egui::FullOutput) -> (Vec<Rect>, usize) {
    let strips: Vec<_> = output
        .shapes
        .iter()
        .enumerate()
        .filter_map(|(index, shape)| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(250, 250, 250) => {
                Some((rect.rect, index))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        strips.len(),
        3,
        "top, left, and corner ruler strips must be painted"
    );
    (strips.iter().map(|(rect, _)| *rect).collect(), strips[0].1)
}

fn ruler_workspace(output: &egui::FullOutput) -> Rect {
    let (strips, _) = ruler_strips(output);
    Rect::from_min_max(
        strips[2].min,
        Pos2::new(strips[0].right(), strips[1].bottom()),
    )
}

fn canvas_modes() -> [ingenaro::model::CanvasMode; 4] {
    use ingenaro::model::CanvasMode;
    [
        CanvasMode::Fixed,
        CanvasMode::Auto,
        CanvasMode::FixedWidth,
        CanvasMode::FixedHeight,
    ]
}

fn canvas_frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 600.0))),
        time: Some(0.1),
        events,
        ..Default::default()
    };
    ctx.run(input, |ctx| app.canvas(ctx))
}

fn wheel(unit: egui::MouseWheelUnit, delta: Vec2, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::MouseWheel {
        unit,
        delta,
        modifiers,
    }
}

fn pointer_button(pos: Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn release_key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn assert_navigation_only(app: &App, project: &Project, ctx: &egui::Context) {
    assert_eq!(
        &app.project, project,
        "navigation must not edit document data"
    );
    assert_eq!(
        app.project, app.saved,
        "navigation must not dirty a saved document"
    );
    assert!(app.undo.is_empty());
    assert!(app.redo.is_empty());
    assert!(
        (ctx.zoom_factor() - 1.0).abs() < 0.0001,
        "canvas zoom must not resize application controls"
    );
}

#[test]
fn control_wheel_zooms_about_pointer_without_document_or_application_zoom() {
    for (unit, delta) in [
        (egui::MouseWheelUnit::Line, 3.0),
        (egui::MouseWheelUnit::Point, 120.0),
    ] {
        let project = Project::blank();
        let mut app = fixture(project.clone());
        let ctx = egui::Context::default();
        let output = frame(&mut app, &ctx, Vec::new());
        let (page, viewport) = page_and_viewport(&output, &app);
        let pointer = page.min + vec2(220.0, 160.0);
        let document_point = (pointer - viewport.min - app.view.pan) / app.zoom;
        let output = frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(pointer),
                wheel(unit, vec2(0.0, delta), egui::Modifiers::CTRL),
            ],
        );
        assert!((app.zoom - 0.65).abs() < 0.0001);
        let (_, viewport_after) = page_and_viewport(&output, &app);
        let document_point_after = (pointer - viewport_after.min - app.view.pan) / app.zoom;
        assert!((document_point_after - document_point).length() < 0.001);
        assert_navigation_only(&app, &project, &ctx);
    }
}

#[test]
fn ordinary_and_shift_wheel_pan_canvas_and_ignore_other_panels() {
    let project = Project::blank();
    let mut app = fixture(project.clone());
    let ctx = egui::Context::default();
    let output = frame(&mut app, &ctx, Vec::new());
    let (page, viewport) = page_and_viewport(&output, &app);
    let position = page.min + vec2(160.0, 160.0);
    let pan = app.view.pan;
    let _ = frame(
        &mut app,
        &ctx,
        vec![
            egui::Event::PointerMoved(position),
            wheel(
                egui::MouseWheelUnit::Point,
                vec2(0.0, -27.0),
                egui::Modifiers::NONE,
            ),
        ],
    );
    assert_eq!(app.view.pan, pan + vec2(0.0, -27.0));
    let _ = frame(
        &mut app,
        &ctx,
        vec![wheel(
            egui::MouseWheelUnit::Point,
            vec2(0.0, 33.0),
            egui::Modifiers::SHIFT,
        )],
    );
    assert_eq!(app.view.pan, pan + vec2(33.0, -27.0));
    assert_eq!(app.zoom, 0.5);
    let pan = app.view.pan;
    for position in [
        Pos2::new(30.0, viewport.center().y),
        Pos2::new(1490.0, viewport.center().y),
        Pos2::new(viewport.center().x, 20.0),
        Pos2::new(viewport.center().x, 945.0),
    ] {
        let _ = frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(position),
                wheel(
                    egui::MouseWheelUnit::Point,
                    vec2(0.0, 120.0),
                    egui::Modifiers::CTRL,
                ),
            ],
        );
        assert_eq!(
            app.view.pan, pan,
            "wheel outside canvas must not move the document view"
        );
        assert_eq!(
            app.zoom, 0.5,
            "wheel outside canvas must not zoom the document view"
        );
    }
    assert_navigation_only(&app, &project, &ctx);
}

#[test]
fn space_primary_and_middle_drag_pan_without_moving_or_selecting_widgets() {
    for button in [egui::PointerButton::Primary, egui::PointerButton::Middle] {
        let mut project = Project::blank();
        let id = add(&mut project, WidgetKind::Rectangle, 80.0, 80.0);
        let other = add(&mut project, WidgetKind::Ellipse, 400.0, 200.0);
        let mut app = fixture(project.clone());
        app.selected.insert(other);
        let selection = app.selected.clone();
        let ctx = egui::Context::default();
        let output = frame(&mut app, &ctx, Vec::new());
        let (page, _) = page_and_viewport(&output, &app);
        let start = page.min + vec2(100.0, 100.0) * app.zoom;
        let end = start + vec2(90.0, 55.0);
        let pan = app.view.pan;
        let mut events = vec![egui::Event::PointerMoved(start)];
        if button == egui::PointerButton::Primary {
            events.push(key(egui::Key::Space, egui::Modifiers::NONE));
        }
        events.push(pointer_button(start, button, true));
        let _ = frame(&mut app, &ctx, events);
        let _ = frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)]);
        assert_eq!(app.view.pan, pan + (end - start));
        assert_eq!(
            app.selected, selection,
            "panning over widget {id} must preserve selection"
        );
        let mut events = vec![pointer_button(end, button, false)];
        if button == egui::PointerButton::Primary {
            events.push(release_key(egui::Key::Space));
        }
        let _ = frame(&mut app, &ctx, events);
        assert!(app.view.panning.is_none());
        assert!(app.drag.is_none());
        assert_eq!(app.selected, selection);
        assert_navigation_only(&app, &project, &ctx);
    }
}

#[test]
fn zoom_and_center_shortcuts_preserve_document_and_center_selection() {
    let mut project = Project::blank();
    let id = add(&mut project, WidgetKind::Rectangle, 160.0, 120.0);
    let mut app = fixture(project.clone());
    app.selected.insert(id);
    let ctx = egui::Context::default();
    let _ = frame(&mut app, &ctx, Vec::new());
    let _ = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Equals, egui::Modifiers::CTRL)],
    );
    assert!((app.zoom - 0.6).abs() < 0.0001);
    let _ = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Minus, egui::Modifiers::CTRL)],
    );
    assert!((app.zoom - 0.5).abs() < 0.0001);
    let _ = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Num0, egui::Modifiers::CTRL)],
    );
    assert_eq!(app.zoom, 1.0);
    let output = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Num1, egui::Modifiers::CTRL)],
    );
    let (page, viewport) = page_and_viewport(&output, &app);
    assert!((page.center() - viewport::content_rect(viewport).center()).length() < 0.01);
    let output = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Num2, egui::Modifiers::CTRL)],
    );
    let (page, viewport) = page_and_viewport(&output, &app);
    let widget = &app.project.pages[0].widgets[0];
    let center = page.min
        + vec2(
            widget.x + widget.width / 2.0,
            widget.y + widget.height / 2.0,
        ) * app.zoom;
    assert!((center - viewport::content_rect(viewport).center()).length() < 0.01);
    let fitted_zoom = app.zoom;
    app.view.pan += vec2(75.0, -48.0);
    let output = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Num3, egui::Modifiers::CTRL)],
    );
    assert_eq!(app.zoom, fitted_zoom, "center selection preserves zoom");
    let (page, viewport) = page_and_viewport(&output, &app);
    let widget = &app.project.pages[0].widgets[0];
    let center = page.min
        + vec2(
            widget.x + widget.width / 2.0,
            widget.y + widget.height / 2.0,
        ) * app.zoom;
    assert!((center - viewport::content_rect(viewport).center()).length() < 0.01);
    assert_navigation_only(&app, &project, &ctx);
}

#[test]
fn focused_text_editor_keeps_canvas_zoom_shortcuts() {
    let mut app = fixture(Project::blank());
    let ctx = egui::Context::default();
    let id = egui::Id::new("navigation-focus");
    let mut text = String::from("zoom stays unchanged while editing text");
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add(egui::TextEdit::singleline(&mut text).id(id))
                .request_focus();
        });
    });
    let pan = app.view.pan;
    let events = vec![
        key(egui::Key::Num0, egui::Modifiers::CTRL),
        key(egui::Key::Num2, egui::Modifiers::CTRL),
        key(egui::Key::Equals, egui::Modifiers::CTRL),
    ];
    let _ = ctx.run(
        egui::RawInput {
            events: events.clone(),
            ..Default::default()
        },
        |ctx| {
            assert!(ctx.wants_keyboard_input());
            app.viewport_shortcuts(ctx);
            assert_eq!(ctx.input(|input| input.events.clone()), events);
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add(egui::TextEdit::singleline(&mut text).id(id));
            });
        },
    );
    assert_eq!(app.zoom, 0.5);
    assert_eq!(app.view.pan, pan);
    assert!(app.view.pending.is_none());
}

#[test]
fn moved_and_zoomed_canvas_still_hits_and_drags_in_document_coordinates() {
    let mut project = Project::blank();
    let id = add(&mut project, WidgetKind::Rectangle, 160.0, 160.0);
    let mut app = fixture(project.clone());
    app.snap = false;
    let ctx = egui::Context::default();
    let output = frame(&mut app, &ctx, Vec::new());
    let (page, _) = page_and_viewport(&output, &app);
    let position = page.min + vec2(220.0, 160.0);
    let _ = frame(
        &mut app,
        &ctx,
        vec![
            egui::Event::PointerMoved(position),
            wheel(
                egui::MouseWheelUnit::Point,
                vec2(0.0, -30.0),
                egui::Modifiers::NONE,
            ),
        ],
    );
    let output = frame(
        &mut app,
        &ctx,
        vec![wheel(
            egui::MouseWheelUnit::Line,
            vec2(0.0, 3.0),
            egui::Modifiers::CTRL,
        )],
    );
    let (page, _) = page_and_viewport(&output, &app);
    assert!((app.zoom - 0.65).abs() < 0.0001);
    assert_navigation_only(&app, &project, &ctx);
    let widget = &app.project.pages[0].widgets[0];
    let start = page.min
        + vec2(
            widget.x + widget.width / 2.0,
            widget.y + widget.height / 2.0,
        ) * app.zoom;
    let _ = click(&mut app, &ctx, start);
    assert_eq!(app.selected, BTreeSet::from([id]));
    let delta = vec2(26.0, 39.0);
    let end = start + delta;
    let _ = frame(&mut app, &ctx, vec![pointer(start, true)]);
    let _ = frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)]);
    let _ = frame(&mut app, &ctx, vec![pointer(end, false)]);
    let widget = &app.project.pages[0].widgets[0];
    assert!((widget.x - 200.0).abs() < 0.001);
    assert!((widget.y - 220.0).abs() < 0.001);
    assert_eq!(app.undo.len(), 1);
    app.undo();
    assert_eq!(app.project, project);
}

#[test]
fn auto_canvas_opens_at_full_scale_and_can_reach_negative_and_far_content() {
    let mut project = Project::blank();
    project.pages[0].canvas = ingenaro::model::CanvasMode::Auto;
    project.pages[0].width = 64.0;
    project.pages[0].height = 64.0;
    let id = add(&mut project, WidgetKind::Text, 1600.0, -50.0);
    project.pages[0].widgets[0].text = "outside the original bounds".into();
    project.pages[0].widgets[0].text_layout = Some(ingenaro::model::TextLayout {
        horizontal: 0,
        vertical: 0,
        padding: [0.0; 4],
        wrap: false,
        bold: false,
    });
    let mut app = fixture(project.clone());
    app.fit = true;
    let ctx = egui::Context::default();
    let _ = frame(&mut app, &ctx, Vec::new());
    assert_eq!(app.zoom, 1.0, "Auto must not fit an invented fixed page");
    assert_eq!(
        app.view.pan,
        vec2(viewport::RULER_SIZE, viewport::RULER_SIZE)
    );
    app.view.pan = vec2(-1300.0, 150.0);
    let output = frame(&mut app, &ctx, Vec::new());
    let shape = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) if text.galley.text() == "outside the original bounds" => {
                Some((text, clipped.clip_rect))
            }
            _ => None,
        })
        .expect("Auto content beyond fallback width and above zero must be painted");
    assert!(shape.1.contains(shape.0.pos));
    assert!(shape.1.contains(shape.0.pos + shape.0.galley.size()));
    let position = shape.0.pos + vec2(30.0, 8.0);
    let _ = click(&mut app, &ctx, position);
    assert_eq!(app.selected, BTreeSet::from([id]));
    assert_navigation_only(&app, &project, &ctx);
}

#[test]
fn all_canvas_modes_keep_rulers_fixed_and_every_canvas_pass_below_them() {
    for mode in canvas_modes() {
        let mut before = Project::blank();
        before.pages[0].canvas = mode;
        let id = add(&mut before, WidgetKind::Rectangle, 0.0, 0.0);
        before.pages[0].widgets[0].fill = "#E12D48".into();
        before.pages[0].widgets[0].text = "Ruler clipping sentinel".into();
        let mut project = before.clone();
        project.pages[0].widgets[0].width += 20.0;
        let mut app = fixture(project.clone());
        app.selected.insert(id);
        app.grid = true;
        let ctx = egui::Context::default();
        let mut expected_strips = None;
        for zoom in [0.1, 0.65, 1.0, 10.0] {
            for pan in [vec2(10.0, 10.0), vec2(-60.0, -50.0), vec2(240.0, 210.0)] {
                app.zoom = zoom;
                app.view.pan = pan;
                let output = canvas_frame(&mut app, &ctx, Vec::new());
                let (strips, first_ruler) = ruler_strips(&output);
                assert_eq!(expected_strips.get_or_insert(strips.clone()), &strips);
                let content = viewport::content_rect(ruler_workspace(&output));
                let mut canvas_shapes = 0;
                for clipped in &output.shapes[..first_ruler] {
                    // The panel background and fixed-page breadcrumb are UI, not canvas passes.
                    if matches!(clipped.shape, egui::Shape::Noop) {
                        continue;
                    }
                    if let egui::Shape::Text(text) = &clipped.shape
                        && text.galley.text() != "Ruler clipping sentinel"
                    {
                        continue;
                    }
                    if let egui::Shape::Rect(rect) = &clipped.shape
                        && rect.fill == Color32::from_rgb(243, 244, 248)
                    {
                        continue;
                    }
                    canvas_shapes += 1;
                    assert!(
                        !clipped.clip_rect.is_positive()
                            || content.contains_rect(clipped.clip_rect),
                        "{mode:?}: canvas clip {:?} must stay inside {content:?}",
                        clipped.clip_rect
                    );
                }
                assert!(
                    canvas_shapes > 1,
                    "background, grid, and selection must be exercised"
                );
                assert_eq!(
                    app.view.pan, pan,
                    "reserving rulers must not change the document origin"
                );
            }
        }
        app.zoom = 1.0;
        app.view.pan = vec2(10.0, 10.0);
        app.ai_reveal.record(&before, &project, 0.0);
        let output = canvas_frame(&mut app, &ctx, Vec::new());
        let content = viewport::content_rect(ruler_workspace(&output));
        let (_, first_ruler) = ruler_strips(&output);
        let status = output
            .shapes
            .iter()
            .enumerate()
            .find_map(|(index, clipped)| match &clipped.shape {
                egui::Shape::Text(text) if text.galley.text() == "正在绘制..." => {
                    Some((index, clipped))
                }
                _ => None,
            })
            .expect("AI reveal status must remain visible");
        assert!(
            status.0 < first_ruler,
            "rulers must be painted after AI reveal"
        );
        assert!(content.contains_rect(status.1.clip_rect));
        assert!(app.ai_reveal.is_active(0.1));
        assert_navigation_only(&app, &project, &ctx);
    }
}

#[test]
fn ruler_pointer_gestures_cannot_select_move_draw_or_drop_widgets() {
    for mode in canvas_modes() {
        let mut project = Project::blank();
        project.pages[0].canvas = mode;
        let overlapped = add(&mut project, WidgetKind::Rectangle, 0.0, 0.0);
        let selected = add(&mut project, WidgetKind::Ellipse, 320.0, 220.0);
        let mut app = fixture(project.clone());
        app.zoom = 1.0;
        app.view.pan = vec2(0.0, 0.0);
        app.selected.insert(selected);
        let selection = app.selected.clone();
        let ctx = egui::Context::default();
        let output = frame(&mut app, &ctx, Vec::new());
        let workspace = ruler_workspace(&output);
        let positions = [
            workspace.min + vec2(60.0, 9.0),
            workspace.min + vec2(9.0, 60.0),
            workspace.min + vec2(9.0, 9.0),
        ];
        for position in positions {
            let clicked = click(&mut app, &ctx, position);
            assert_eq!(
                app.selected,
                selection,
                "{mode:?}: ruler at {position:?} in {workspace:?} (now {:?}) must not select overlapped widget {overlapped}",
                ruler_workspace(&clicked)
            );
            let _ = frame(
                &mut app,
                &ctx,
                vec![pointer_button(
                    position,
                    egui::PointerButton::Secondary,
                    true,
                )],
            );
            let _ = frame(
                &mut app,
                &ctx,
                vec![pointer_button(
                    position,
                    egui::PointerButton::Secondary,
                    false,
                )],
            );
            assert_eq!(app.selected, selection);
            assert!(app.editing.is_none());
            for drawing in [false, true] {
                if drawing {
                    app.tool = Some(WidgetKind::Rectangle);
                }
                let _ = frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::PointerMoved(position), pointer(position, true)],
                );
                let end = position + vec2(90.0, 90.0);
                let _ = frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)]);
                assert!(app.drag.is_none());
                let _ = frame(&mut app, &ctx, vec![pointer(end, false)]);
                assert_eq!(app.selected, selection);
                assert_navigation_only(&app, &project, &ctx);
                if drawing {
                    let _ = click(&mut app, &ctx, position);
                    assert_eq!(
                        app.tool,
                        Some(WidgetKind::Rectangle),
                        "clicking a ruler must not consume the drawing tool"
                    );
                    app.tool = None;
                }
            }
            let _ = frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(position), pointer(position, true)],
            );
            app.pending_widget = Some(library::LibraryDrag::new(widget_library::ITEMS[0]));
            let _ = frame(&mut app, &ctx, vec![pointer(position, false)]);
            assert!(
                app.pending_widget.is_none(),
                "a canceled ruler drop must clear the drag preview"
            );
            assert_navigation_only(&app, &project, &ctx);
        }
    }
}

#[test]
fn ruler_wheel_and_pan_starts_are_excluded_but_content_anchors_stay_exact() {
    for mode in canvas_modes() {
        let mut project = Project::blank();
        project.pages[0].canvas = mode;
        let mut app = fixture(project.clone());
        let ctx = egui::Context::default();
        let output = frame(&mut app, &ctx, Vec::new());
        let workspace = ruler_workspace(&output);
        let content = viewport::content_rect(workspace);
        for position in [
            workspace.min + vec2(60.0, 9.0),
            workspace.min + vec2(9.0, 60.0),
        ] {
            let pan = app.view.pan;
            let zoom = app.zoom;
            for modifiers in [
                egui::Modifiers::NONE,
                egui::Modifiers::SHIFT,
                egui::Modifiers::CTRL,
            ] {
                let _ = frame(
                    &mut app,
                    &ctx,
                    vec![
                        egui::Event::PointerMoved(position),
                        wheel(egui::MouseWheelUnit::Point, vec2(0.0, 120.0), modifiers),
                    ],
                );
                assert_eq!(app.view.pan, pan);
                assert_eq!(app.zoom, zoom);
            }
            let _ = frame(&mut app, &ctx, vec![egui::Event::Zoom(1.5)]);
            assert_eq!(app.zoom, zoom);
            for button in [egui::PointerButton::Primary, egui::PointerButton::Middle] {
                let mut events = vec![egui::Event::PointerMoved(position)];
                if button == egui::PointerButton::Primary {
                    events.push(key(egui::Key::Space, egui::Modifiers::NONE));
                }
                events.push(pointer_button(position, button, true));
                let _ = frame(&mut app, &ctx, events);
                let end = content.min + vec2(150.0, 120.0);
                let _ = frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)]);
                assert!(app.view.panning.is_none());
                assert_eq!(app.view.pan, pan);
                let mut events = vec![pointer_button(end, button, false)];
                if button == egui::PointerButton::Primary {
                    events.push(release_key(egui::Key::Space));
                }
                let _ = frame(&mut app, &ctx, events);
            }
        }
        let pointer = content.min + vec2(10.0, 15.0);
        let point = (pointer - workspace.min - app.view.pan) / app.zoom;
        let _ = frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(pointer),
                wheel(
                    egui::MouseWheelUnit::Line,
                    vec2(0.0, 3.0),
                    egui::Modifiers::CTRL,
                ),
            ],
        );
        assert!(
            (app.zoom - 0.65).abs() < 0.0001,
            "{mode:?}: zoom {} at {pointer:?}, workspace {workspace:?}",
            app.zoom
        );
        assert!(((pointer - workspace.min - app.view.pan) / app.zoom - point).length() < 0.001);
        let pan = app.view.pan;
        let _ = frame(
            &mut app,
            &ctx,
            vec![wheel(
                egui::MouseWheelUnit::Page,
                vec2(0.0, -1.0),
                egui::Modifiers::NONE,
            )],
        );
        assert_eq!(app.view.pan, pan - vec2(0.0, content.height()));
        assert_navigation_only(&app, &project, &ctx);
    }
}

#[test]
fn caller_selected_native_auto_title_opens_without_cropping_or_source_changes() {
    let Some(path) = std::env::var_os("RPSHOP_NATIVE_FIXTURE") else {
        return;
    };
    let path = PathBuf::from(path);
    let original = std::fs::read(&path).unwrap();
    let native = ingenaro::rp_format::open(&path).unwrap();
    let (page_index, widget_id) = native
        .project
        .pages
        .iter()
        .enumerate()
        .find_map(|(index, page)| {
            if page.canvas != ingenaro::model::CanvasMode::Auto {
                return None;
            }
            page.widgets
                .iter()
                .find(|widget| {
                    widget.height <= 24.0
                        && widget.text.len() > 80
                        && widget.text_layout.as_ref().is_some_and(|layout| {
                            !layout.wrap
                                && layout.horizontal == 0
                                && layout.vertical == 0
                                && layout.padding == [0.0; 4]
                        })
                })
                .map(|widget| (index, widget.id))
        })
        .expect("fixture must include an Auto page with a short-height, auto-width native title");
    let project = native.project.clone();
    let mut app = fixture(project.clone());
    app.native = Some(native);
    app.page = page_index;
    app.fit = true;
    let ctx = egui::Context::default();
    setup_style(&ctx);
    let output = frame(&mut app, &ctx, Vec::new());
    assert_eq!(app.zoom, 1.0);
    let widget = app.project.pages[page_index]
        .widgets
        .iter()
        .find(|widget| widget.id == widget_id)
        .unwrap();
    let (text, clip) = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == widget.text => {
                Some((text, shape.clip_rect))
            }
            _ => None,
        })
        .expect("source title must be painted");
    assert_eq!(
        text.galley.rows.len(),
        1,
        "source auto-width title must keep one row"
    );
    assert!(
        clip.contains(text.pos),
        "source title top must remain visible"
    );
    assert!(
        clip.contains(text.pos + text.galley.size()),
        "source title must remain fully visible"
    );
    assert_navigation_only(&app, &project, &ctx);
    assert_eq!(std::fs::read(path).unwrap(), original);
}
