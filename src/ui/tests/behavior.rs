use super::*;

#[test]
fn layer_icons_toggle_visibility_and_lock_without_selecting_the_row() {
    let mut project = Project::blank();
    add(&mut project, WidgetKind::Text, 80.0, 80.0);
    project.pages[0].widgets[0].name = "layer".into();
    let mut app = fixture(project);
    app.sidebar_layout.active = Some(sidebar::LeftPane::Layers);
    let ctx = egui::Context::default();
    let output = frame(&mut app, &ctx, Vec::new());
    let row_y = text_center(&output, "T layer").y;
    // The library also contains a Flow item named 显示. Only the visible layer
    // header supplies the column coordinate; offscreen library rows do not.
    let header_x = |label: &str| {
        output
            .shapes
            .iter()
            .find_map(|clipped| {
                if let egui::epaint::Shape::Text(shape) = &clipped.shape {
                    let center = shape.pos + shape.galley.size() / 2.0;
                    (shape.galley.text() == label && clipped.clip_rect.contains(center))
                        .then_some(center.x)
                } else {
                    None
                }
            })
            .unwrap()
    };
    let visible = Pos2::new(header_x("显示"), row_y);
    let locked = Pos2::new(header_x("锁定"), row_y);

    let _ = click(&mut app, &ctx, visible);
    assert!(app.project.pages[0].widgets[0].hidden);
    assert!(!app.project.pages[0].widgets[0].locked);
    assert!(app.selected.is_empty());

    let _ = click(&mut app, &ctx, locked);
    assert!(app.project.pages[0].widgets[0].hidden);
    assert!(app.project.pages[0].widgets[0].locked);
    assert!(app.selected.is_empty());
    assert_eq!(app.undo.len(), 2, "each control click is one edit");

    let _ = click(&mut app, &ctx, visible);
    let _ = click(&mut app, &ctx, locked);
    assert!(!app.project.pages[0].widgets[0].hidden);
    assert!(!app.project.pages[0].widgets[0].locked);
}

#[test]
fn many_long_layer_names_do_not_expand_the_default_navigation_panel() {
    let mut project = Project::blank();
    for index in 0..46 {
        add(&mut project, WidgetKind::Text, 80.0, 80.0);
        project.pages[0].widgets[index].name = format!(
            "{index} a very long layer name that must be truncated in the navigation panel"
        );
    }
    let mut app = fixture(project);
    app.sidebar_layout.active = Some(sidebar::LeftPane::Layers);
    let ctx = egui::Context::default();
    for _ in 0..10 {
        let output = frame(&mut app, &ctx, Vec::new());
        let _ = text_center(&output, "显示");
        let _ = text_center(&output, "锁定");
        let _ = text_center(&output, "双击名称可重命名");
        let panel = egui::containers::panel::PanelState::load(&ctx, egui::Id::new("navigator"))
            .expect("the navigation panel must be laid out");
        assert!(
            (panel.rect.width() - 225.0).abs() < 1.0,
            "layer content must not resize the default panel: {}",
            panel.rect.width(),
        );
    }
}
