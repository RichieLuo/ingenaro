use super::*;

fn rail_button(ctx: &egui::Context, right: bool, index: usize) -> Pos2 {
    let id = if right {
        "right-activity-bar"
    } else {
        "left-activity-bar"
    };
    let rect = egui::containers::panel::PanelState::load(ctx, egui::Id::new(id))
        .unwrap()
        .rect;
    Pos2::new(rect.center().x, rect.top() + 30.0 + index as f32 * 47.0)
}

#[test]
fn activity_icons_switch_and_collapse_without_document_changes() {
    let mut app = fixture(Project::blank());
    let project = app.project.clone();
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    for (index, pane) in [
        (1, sidebar::LeftPane::Library),
        (2, sidebar::LeftPane::Layers),
        (0, sidebar::LeftPane::Pages),
    ] {
        let button = rail_button(&ctx, false, index);
        click(&mut app, &ctx, button);
        assert_eq!(app.sidebar_layout.active, Some(pane));
        click(&mut app, &ctx, button);
        assert_eq!(app.sidebar_layout.active, None);
    }
    assert_eq!(app.project, project);
    assert!(app.undo.is_empty());
}

#[test]
fn right_activity_bar_switches_and_collapses() {
    let mut app = fixture(Project::blank());
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    let ai = rail_button(&ctx, true, 1);
    click(&mut app, &ctx, ai);
    assert!(app.ai.visible);
    click(&mut app, &ctx, ai);
    assert!(!app.ai.visible);
    assert!(!app.sidebar_layout.inspector_open);
    let properties = rail_button(&ctx, true, 0);
    click(&mut app, &ctx, properties);
    assert!(app.sidebar_layout.inspector_open);
    click(&mut app, &ctx, properties);
    assert!(!app.sidebar_layout.inspector_open);
}

#[test]
fn workspace_preferences_persist_and_old_splitters_migrate() {
    let mut app = fixture(Project::blank());
    app.sidebar_layout.active = Some(sidebar::LeftPane::Layers);
    app.sidebar_layout.inspector_open = false;
    app.screenshot_mode = false;
    let mut storage = MemoryStorage::default();
    app.save(&mut storage);
    let restored: sidebar::SidebarLayout =
        eframe::get_value(&storage, sidebar::STORAGE_KEY).unwrap();
    assert_eq!(restored, app.sidebar_layout);
    let migrated: sidebar::SidebarLayout =
        serde_json::from_str(r#"{"first":0.3,"second":0.7}"#).unwrap();
    assert_eq!(migrated, sidebar::SidebarLayout::default());
}

#[test]
fn library_search_adds_controls_and_undo_preserves_geometry() {
    let mut app = fixture(Project::blank());
    app.sidebar_layout.active = Some(sidebar::LeftPane::Library);
    let ctx = egui::Context::default();
    app.library_query = "多行".into();
    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_center(&output, "文本域"));
    assert_eq!(app.project.pages[0].widgets[0].kind, WidgetKind::TextArea);
    app.library_query = "flow decision".into();
    frame(&mut app, &ctx, vec![]);
    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_center(&output, "判断"));
    let geometry = app.project.pages[0].widgets[1].vector_points.clone();
    assert!(geometry.is_some());
    app.undo();
    assert_eq!(app.project.pages[0].widgets.len(), 1);
    app.redo();
    assert_eq!(app.project.pages[0].widgets[1].vector_points, geometry);
}

#[test]
fn library_filter_clear_and_drag_insert_an_editable_group() {
    let project = Project::blank();
    let mut app = fixture(project.clone());
    app.sidebar_layout.active = Some(sidebar::LeftPane::Library);
    let ctx = egui::Context::default();
    setup_style(&ctx);
    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_center(&output, "组合"));
    assert_eq!(app.library_category, Some(Category::Patterns));

    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_center(&output, "搜索组件…"));
    let output = frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("missing-widget".into())],
    );
    assert_eq!(app.library_query, "missing-widget");
    text_center(&output, "没有匹配的组件");
    let clear = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "×" => {
                Some(text.pos + text.galley.size() / 2.0)
            }
            _ => None,
        })
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .expect("a nonempty search has a clear button below the pane header");
    click(&mut app, &ctx, clear);
    assert!(app.library_query.is_empty());
    assert_eq!(app.library_category, Some(Category::Patterns));
    assert_eq!(app.project, project);

    let output = frame(&mut app, &ctx, vec![]);
    let tile = text_center(&output, "登录表单");
    let canvas = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(shape)
                if shape.fill == Color32::WHITE && shape.rect.size() == vec2(640.0, 400.0) =>
            {
                Some(shape.rect)
            }
            _ => None,
        })
        .expect("the fixed-size blank artboard is visible");
    let destination = canvas.min + vec2(48.0, 64.0);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(tile), pointer(tile, true)],
    );
    let output = frame(&mut app, &ctx, vec![egui::Event::PointerMoved(destination)]);
    let title = text_center(&output, "邮箱登录");
    assert!(Rect::from_min_size(destination, vec2(160.0, 134.0)).contains(title));
    assert_eq!(output.platform_output.cursor_icon, egui::CursorIcon::Copy);
    assert_eq!(
        app.pending_widget.as_ref().unwrap().item.id,
        "pattern-login"
    );
    assert_eq!(app.project, project);
    assert!(app.undo.is_empty());

    let delta = vec2(48.0, 24.0);
    let destination = destination + delta;
    let output = frame(&mut app, &ctx, vec![egui::Event::PointerMoved(destination)]);
    let moved_title = text_center(&output, "邮箱登录");
    assert!(moved_title.distance(title + delta) < 1.0);
    assert_eq!(app.project, project);
    let output = frame(&mut app, &ctx, vec![pointer(destination, false)]);
    assert!(text_center(&output, "邮箱登录").distance(moved_title) < 1.0);

    let widgets = &app.project.pages[0].widgets;
    assert_eq!((widgets[0].x, widgets[0].y), (192.0, 176.0));
    assert_eq!(widgets[0].groups.len(), 1);
    assert!(
        widgets
            .iter()
            .all(|widget| widget.groups == widgets[0].groups)
    );
    assert_eq!(
        widgets
            .iter()
            .filter(|widget| widget.kind == WidgetKind::Input)
            .count(),
        2
    );
    assert_eq!(app.undo.len(), 1);
    app.undo();
    assert_eq!(app.project, project);
}

#[test]
fn escape_cancels_a_library_drag_before_release() {
    let project = Project::blank();
    let mut app = fixture(project.clone());
    app.sidebar_layout.active = Some(sidebar::LeftPane::Library);
    app.library_query = "pattern-login".into();
    let ctx = egui::Context::default();
    setup_style(&ctx);
    let output = frame(&mut app, &ctx, vec![]);
    let tile = text_center(&output, "登录表单");
    let destination = Pos2::new(500.0, 330.0);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(tile), pointer(tile, true)],
    );
    let output = frame(&mut app, &ctx, vec![egui::Event::PointerMoved(destination)]);
    assert_eq!(output.platform_output.cursor_icon, egui::CursorIcon::Copy);
    let output = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(app.pending_widget.is_none());
    assert_ne!(output.platform_output.cursor_icon, egui::CursorIcon::Copy);
    frame(&mut app, &ctx, vec![pointer(destination, false)]);
    assert_eq!(app.project, project);
    assert!(app.undo.is_empty());
}

#[test]
fn resizing_the_library_fits_a_third_column_without_changing_the_document() {
    let project = Project::blank();
    let mut app = fixture(project.clone());
    app.sidebar_layout.active = Some(sidebar::LeftPane::Library);
    let ctx = egui::Context::default();
    setup_style(&ctx);
    let output = frame(&mut app, &ctx, vec![]);
    assert!(text_center(&output, "文本").y > text_center(&output, "矩形").y);

    let panel = egui::containers::panel::PanelState::load(&ctx, egui::Id::new("navigator"))
        .unwrap()
        .rect;
    let grip = Pos2::new(panel.right() - 2.0, panel.center().y);
    let destination = grip + vec2(75.0, 0.0);
    frame(&mut app, &ctx, vec![egui::Event::PointerMoved(grip)]);
    frame(&mut app, &ctx, vec![pointer(grip, true)]);
    frame(&mut app, &ctx, vec![egui::Event::PointerMoved(destination)]);
    frame(&mut app, &ctx, vec![]);
    let output = frame(&mut app, &ctx, vec![pointer(destination, false)]);
    let first = text_center(&output, "矩形");
    let third = text_center(&output, "文本");
    assert_eq!(first.y, third.y);
    assert!(third.x > first.x);
    assert_eq!(app.project, project);
    assert!(app.undo.is_empty());
}

#[test]
fn layer_pane_creates_collapsible_named_groups_and_renames_a_child() {
    let mut project = Project::blank();
    let first = add(&mut project, WidgetKind::Text, 80.0, 80.0);
    let second = add(&mut project, WidgetKind::Text, 160.0, 80.0);
    project.pages[0].widgets[0].name = "First".into();
    project.pages[0].widgets[1].name = "Second".into();
    let mut app = fixture(project.clone());
    app.sidebar_layout.active = Some(sidebar::LeftPane::Layers);
    app.selected = BTreeSet::from([first, second]);
    let ctx = egui::Context::default();
    setup_style(&ctx);
    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_center(&output, "新建分组"));
    let output = frame(&mut app, &ctx, vec![egui::Event::Text("Header".into())]);
    click(&mut app, &ctx, text_center(&output, "创建"));
    let group = app.project.pages[0].widgets[0].groups[0];
    assert_eq!(app.project.pages[0].group_name(group), "Header");
    assert_eq!(app.undo, vec![project]);
    assert!(!app.expanded_layer_groups.contains(&group));

    let disclosure = |output: &egui::FullOutput, open: bool| {
        let label = text_center(output, "Header (2)");
        let [base_a, base_b, tip] = output
            .shapes
            .iter()
            .find_map(|clipped| {
                let egui::Shape::Path(path) = &clipped.shape else {
                    return None;
                };
                let [base_a, base_b, tip] = path.points.as_slice() else {
                    return None;
                };
                (path.closed
                    && path.fill != Color32::TRANSPARENT
                    && clipped.clip_rect.contains(*tip)
                    && tip.x < label.x
                    && (tip.y - label.y).abs() < 6.0)
                    .then_some([*base_a, *base_b, *tip])
            })
            .expect("group row paints a visible disclosure triangle");
        if open {
            assert_eq!(base_a.y, base_b.y);
            assert!(tip.y > base_a.y);
        } else {
            assert_eq!(base_a.x, base_b.x);
            assert!(tip.x > base_a.x);
        }
        Rect::from_points(&[base_a, base_b, tip]).center()
    };
    let output = frame(&mut app, &ctx, vec![]);
    let arrow = disclosure(&output, false);
    click(&mut app, &ctx, arrow);
    assert!(app.expanded_layer_groups.contains(&group));
    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, disclosure(&output, true));
    assert!(!app.expanded_layer_groups.contains(&group));
    click(&mut app, &ctx, arrow);
    assert!(app.expanded_layer_groups.contains(&group));
    assert_eq!(app.undo.len(), 1, "folding a group is a view change");

    let output = frame(&mut app, &ctx, vec![]);
    let child = text_center(&output, "T First");
    click(&mut app, &ctx, child);
    assert_eq!(app.selected, BTreeSet::from([first]));
    click(&mut app, &ctx, child);
    let output = frame(&mut app, &ctx, vec![egui::Event::Text("Logo".into())]);
    click(&mut app, &ctx, text_center(&output, "保存"));
    assert_eq!(app.project.pages[0].widgets[0].name, "Logo");
    app.undo();
    assert_eq!(app.project.pages[0].widgets[0].name, "First");
    app.redo();
    assert_eq!(app.project.pages[0].widgets[0].name, "Logo");

    let output = frame(&mut app, &ctx, vec![]);
    let group_row = text_center(&output, "Header (2)");
    click(&mut app, &ctx, group_row);
    assert_eq!(app.selected, BTreeSet::from([first, second]));
    click(&mut app, &ctx, group_row);
    frame(&mut app, &ctx, vec![egui::Event::Text("Navigation".into())]);
    frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    assert_eq!(app.project.pages[0].group_name(group), "Navigation");
}
