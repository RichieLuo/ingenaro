use super::*;
use crate::ui::tests::{fixture, frame};
use serde_json::json;

#[test]
fn agent_transaction_updates_live_document_undo_redo_and_save() {
    let mut app = fixture(Project::blank());
    let before = app.project.clone();
    let ctx = egui::Context::default();
    let call = automation::ToolCall {
        name: "rpshop_batch_edit".into(),
        arguments: json!({"operations":[{"op":"create_widget","kind":"Text","properties":{"x":48,"y":72,"text":"Agent 创建的设计标题"}}]}),
    };
    let result = app.execute_ai_tool(&ctx, &call);
    assert!(result.is_ok(), "{result:?}");
    assert_ne!(app.project, before);
    assert_eq!(app.undo.len(), 1);
    app.undo();
    assert_eq!(app.project, before);
    app.redo();
    assert_eq!(app.project.pages[0].widgets[0].text, "Agent 创建的设计标题");
    let path = std::env::temp_dir().join(format!("rpshop-ai-live-{}.ingenaro", uuid::Uuid::new_v4()));
    app.write_document(&path).unwrap();
    assert_eq!(Project::load(&path).unwrap(), app.project);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn invalid_agent_batch_never_changes_document_or_history() {
    let mut app = fixture(Project::blank());
    let before = app.project.clone();
    let ctx = egui::Context::default();
    let call = automation::ToolCall {
        name: "rpshop_batch_edit".into(),
        arguments: json!({"operations":[{"op":"create_widget","kind":"Text","properties":{"x":48,"y":72}},{"op":"update_widget","widget_id":99999,"properties":{"text":"bad"}}]}),
    };
    assert!(app.execute_ai_tool(&ctx, &call).is_err());
    assert_eq!(app.project, before);
    assert!(app.undo.is_empty());
}

#[test]
fn html_publication_is_not_a_native_document_edit() {
    let mut app = fixture(Project::blank());
    let before = app.project.clone();
    let ctx = egui::Context::default();
    let source = "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Sample</title></head><body><h1>Hello</h1></body></html>";
    app.execute_ai_tool(
        &ctx,
        &automation::ToolCall {
            name: "rpshop_publish_html".into(),
            arguments: json!({"source":source}),
        },
    )
    .unwrap();
    assert_eq!(app.project, before);
    assert!(app.undo.is_empty());
    assert_eq!(app.ai.kind, design::ArtifactKind::Html);
    assert_eq!(app.ai.html, source);
}

#[test]
fn switch_document_revokes_tasks_and_external_access() {
    let mut app = fixture(Project::blank());
    let task = app.ai.task_cancelled.clone();
    let external = app.ai.external_cancelled.clone();
    app.ai_document_replaced();
    assert!(task.load(Ordering::SeqCst));
    assert!(external.load(Ordering::SeqCst));
    assert!(app.ai.binding.is_none());
    assert!(app.ai.server.is_none());
}

#[test]
fn workbench_renders_headless_without_key_or_engine() {
    let mut app = fixture(Project::blank());
    app.ai.visible = true;
    let ctx = egui::Context::default();
    for _ in 0..2 {
        frame(&mut app, &ctx, vec![]);
    }
    assert_eq!(app.project, Project::blank());
}

#[test]
fn stale_queued_tool_request_is_rejected_after_project_switch() {
    let mut app = fixture(Project::blank());
    let before = app.project.clone();
    let key = app.ai_project_key();
    app.ai.last_project_key = key.clone();
    let flag = Arc::new(AtomicBool::new(false));
    let client = app.ai.client.scoped(key, flag.clone());
    let worker = std::thread::spawn(move || {
        client.call(automation::ToolCall {
            name: "rpshop_batch_edit".into(),
            arguments: json!({"operations":[{"op":"update_project","name":"must not apply"}]}),
        })
    });
    app.ai_document_replaced();
    let ctx = egui::Context::default();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !worker.is_finished() && Instant::now() < deadline {
        app.poll_ai(&ctx);
        std::thread::yield_now();
    }
    flag.store(true, Ordering::SeqCst);
    assert!(worker.join().unwrap().is_err());
    assert_eq!(app.project, before);
}
