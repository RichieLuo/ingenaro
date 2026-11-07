use super::*;

#[test]
fn finishing_screenshot_capture_does_not_overwrite_the_real_user_session() {
    let mut storage = MemoryStorage::default();
    let mut user_project = Project::blank();
    user_project.name = "用户现有工程".into();
    eframe::set_value(
        &mut storage,
        "rpshop-session",
        &Session {
            ai_document_id: "test-document".into(),
            project: user_project.clone(),
            saved: user_project,
            path: None,
            import_warnings: Vec::new(),
            native_state: None,
        },
    );
    let original = storage.values.clone();
    let original_writes = storage.writes;
    let mut app = fixture(Project::default());
    app.screenshot = Some(PathBuf::from("capture.png"));
    // This is the transition performed when Event::Screenshot consumes its output path.
    let _ = app.screenshot.take();
    app.save(&mut storage);
    assert_eq!(storage.values, original);
    assert_eq!(storage.writes, original_writes);
}

#[test]
fn session_roundtrip_restores_unsaved_edits_and_repairs_transient_blank_names() {
    let mut project = Project::blank();
    add(&mut project, WidgetKind::Button, 123.0, 234.0);
    let mut app = fixture(project);
    app.screenshot_mode = false;
    app.project.name.clear();
    app.project.pages[0].name = "  ".into();
    app.project.pages[0].widgets[0].text = "未保存的修改".into();
    app.path = Some(PathBuf::from("existing.ingenaro"));
    let saved = app.saved.clone();
    let mut storage = MemoryStorage::default();
    app.save(&mut storage);

    let ctx = egui::Context::default();
    let mut creation = eframe::CreationContext::_new_kittest(ctx);
    creation.storage = Some(&storage);
    let restored = App::new(&creation, None);
    assert_eq!(restored.project.pages[0].widgets[0].text, "未保存的修改");
    assert_eq!(
        (
            restored.project.pages[0].widgets[0].x,
            restored.project.pages[0].widgets[0].y
        ),
        (123.0, 234.0)
    );
    assert!(!restored.project.name.trim().is_empty());
    assert!(!restored.project.pages[0].name.trim().is_empty());
    assert_eq!(restored.saved, saved);
    assert_ne!(
        restored.project, restored.saved,
        "recovery must preserve the dirty state"
    );
    assert_eq!(restored.path, Some(PathBuf::from("existing.ingenaro")));
    restored.project.validate().unwrap();
}
