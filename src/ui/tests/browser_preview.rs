use super::*;
use std::io::{Read, Write};

fn opened_url(output: &egui::FullOutput) -> String {
    output
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            egui::OutputCommand::OpenUrl(url) => {
                assert!(url.new_tab);
                Some(url.url.clone())
            }
            _ => None,
        })
        .expect("F5 should ask the desktop backend to open the default browser")
}

fn request(url: &str) -> String {
    let address = url.strip_prefix("http://").unwrap();
    let (host, path) = address.split_once('/').unwrap();
    let path = path.split('#').next().unwrap();
    let mut stream = std::net::TcpStream::connect(host).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(
        stream,
        "GET /{path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
    response
}

#[test]
fn f5_opens_current_page_in_browser_and_editor_remains_editable() {
    let mut app = fixture(Project::default());
    app.page = 1;
    app.sidebar_layout.active = Some(sidebar::LeftPane::Library);
    let original = app.project.clone();
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    let output = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::F5, egui::Modifiers::NONE)],
    );
    let url = opened_url(&output);
    assert!(url.starts_with("http://127.0.0.1:"));
    assert!(url.ends_with(&format!("#page-{}", original.pages[1].id)));
    assert!(request(&url).contains("<!doctype html>"));
    assert_eq!(app.project, original);
    assert_eq!(app.saved, original);
    assert!(app.undo.is_empty());
    let count = app.project.pages[1].widgets.len();
    let rectangle = text_center(&output, "矩形");
    click(&mut app, &ctx, rectangle);
    assert_eq!(app.project.pages[1].widgets.len(), count + 1);
    let id = app.project.pages[1].widgets.last().unwrap().id;
    assert!(request(&url).contains(&format!("id=\"w-{id}\"")));
    assert_eq!(app.undo.len(), 1);
}

#[test]
fn browser_refresh_tracks_unsaved_edits_undo_redo_and_reuses_the_session() {
    let mut app = fixture(Project::default());
    let ctx = egui::Context::default();
    let output = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::F5, egui::Modifiers::NONE)],
    );
    let first_url = opened_url(&output);
    let before = app.project.clone();
    app.project.pages[0].widgets[0].text = "browser refresh unsaved marker".into();
    app.remember(before.clone());
    assert!(request(&first_url).contains("browser refresh unsaved marker"));
    app.undo();
    assert!(!request(&first_url).contains("browser refresh unsaved marker"));
    app.redo();
    assert!(request(&first_url).contains("browser refresh unsaved marker"));
    app.page = 1;
    frame(&mut app, &ctx, vec![]);
    let output = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::F5, egui::Modifiers::NONE)],
    );
    let second_url = opened_url(&output);
    assert_eq!(first_url.split('#').next(), second_url.split('#').next());
    assert!(second_url.ends_with(&format!("#page-{}", app.project.pages[1].id)));
    assert_eq!(app.saved, before);
    assert_eq!(app.undo.len(), 1);
}

#[test]
fn opening_another_project_closes_the_previous_preview_session() {
    let mut app = fixture(Project::default());
    let ctx = egui::Context::default();
    let output = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::F5, egui::Modifiers::NONE)],
    );
    let url = opened_url(&output);
    let host = url
        .strip_prefix("http://")
        .unwrap()
        .split('/')
        .next()
        .unwrap()
        .to_owned();
    let path = std::env::temp_dir().join(format!(
        "rpshop-preview-switch-{}-{}.ingenaro",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let other = Project::blank();
    other.save(&path).unwrap();
    let original = std::fs::read(&path).unwrap();
    app.pending_open = Some(path.clone());
    app.request_replace("open-path");
    assert!(app.browser_preview.is_none());
    assert_eq!(app.project, other);
    assert!(std::net::TcpStream::connect(host).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn canceling_a_drag_refreshes_a_snapshot_published_during_the_drag() {
    let mut project = Project::blank();
    add(&mut project, WidgetKind::Rectangle, 80.0, 80.0);
    let mut app = fixture(project.clone());
    app.project.pages[0].widgets[0].x = 222.0;
    app.drag = Some(Drag {
        before: project.clone(),
        pointer: Pos2::new(100.0, 100.0),
        mode: DragMode::Move,
    });
    let ctx = egui::Context::default();
    app.start_preview(&ctx);
    let url = app
        .browser_preview
        .as_ref()
        .unwrap()
        .url(project.pages[0].id);
    assert!(request(&url).contains("left:222px"));
    frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    let response = request(&url);
    assert!(response.contains("left:80px"));
    assert!(!response.contains("left:222px"));
    assert_eq!(app.project, project);
    assert!(app.undo.is_empty());
}

#[test]
fn native_save_keeps_the_logical_browser_page_when_editor_ids_are_reallocated() {
    let Some((path, _, document)) = native_tree_upgrade_fixture() else {
        return;
    };
    assert!(document.project.pages.len() > 1);
    let mut app = fixture(document.project);
    app.native = document.native;
    app.path = Some(path.clone());
    app.page = app.project.pages.len() - 1;
    let old_page_id = app.project.pages[app.page].id;
    let source_key = app.preview_page_keys()[&old_page_id].clone();
    let ctx = egui::Context::default();
    let output = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::F5, egui::Modifiers::NONE)],
    );
    let url = opened_url(&output);
    assert!(url.contains("&key="));
    let first_source = app.native_source_snapshot().unwrap();
    let first_identity = app.native.as_ref().unwrap().preview_identity();
    assert_eq!(first_source.preview_identity(), first_identity);
    app.start_preview(&ctx);
    assert!(std::sync::Arc::ptr_eq(
        &first_source,
        &app.native_source_snapshot().unwrap()
    ));
    let before = app.project.clone();
    add(&mut app.project, WidgetKind::Rectangle, 40.0, 40.0);
    app.remember(before);
    assert!(
        std::sync::Arc::ptr_eq(&first_source, &app.native_source_snapshot().unwrap()),
        "editor changes must reuse the immutable native source Arc"
    );
    assert!(app.save_project(false), "{}", app.status);
    let saved_identity = app.native.as_ref().unwrap().preview_identity();
    let saved_source = app.native_source_snapshot().unwrap();
    assert_ne!(first_identity, saved_identity);
    assert_eq!(saved_source.preview_identity(), saved_identity);
    assert!(!std::sync::Arc::ptr_eq(&first_source, &saved_source));
    assert!(std::sync::Arc::ptr_eq(
        &saved_source,
        &app.native_source_snapshot().unwrap()
    ));
    let new_page_id = app.project.pages[app.page].id;
    assert_ne!(
        old_page_id, new_page_id,
        "this fixture must exercise native ID reallocation"
    );
    assert_eq!(app.preview_page_keys()[&new_page_id], source_key);
    let response = request(&url);
    let json = response
        .split("id=\"prototype-data\">")
        .nth(1)
        .unwrap()
        .split("</script>")
        .next()
        .unwrap();
    let data: serde_json::Value = serde_json::from_str(json).unwrap();
    let logical_page = data["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["key"] == source_key)
        .unwrap();
    assert_eq!(logical_page["id"], new_page_id.to_string());
    let state = app.native.as_ref().unwrap().session_state();
    app.native
        .as_mut()
        .unwrap()
        .restore_session_state(&state)
        .unwrap();
    let restored_source = app.native_source_snapshot().unwrap();
    assert_ne!(saved_identity, restored_source.preview_identity());
    assert!(!std::sync::Arc::ptr_eq(&saved_source, &restored_source));
    app.native = None;
    assert!(app.native_source_snapshot().is_none());
    assert!(app.cached_native_preview_source.is_none());
    std::fs::remove_file(path).unwrap();
}
