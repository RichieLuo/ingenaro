use super::*;
use crate::ui::tests::{click, fixture, frame, text_center};
use serde_json::json;

fn key(modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: Some(egui::Key::Enter),
        pressed: true,
        repeat: false,
        modifiers,
    }
}
fn app() -> App {
    let mut app = fixture(Project::blank());
    app.ai.visible = true;
    app.ai.last_project_key = app.ai_project_key();
    app
}
fn focus_prompt(app: &mut App, context: &egui::Context) {
    let output = frame(app, context, vec![]);
    let point = text_center(&output, "描述你的设计，选中元件会自动引用，Enter 发送");
    click(app, context, point);
}

#[test]
fn enter_sends_but_shift_enter_inserts_newline() {
    let mut app = app();
    let context = egui::Context::default();
    focus_prompt(&mut app, &context);
    frame(
        &mut app,
        &context,
        vec![egui::Event::Text("先保留布局".into())],
    );
    frame(&mut app, &context, vec![key(egui::Modifiers::SHIFT)]);
    assert!(
        app.ai.prompt.contains('\n'),
        "prompt={:?}, settings={}",
        app.ai.prompt,
        app.ai.settings_open
    );
    assert!(!app.ai.settings_open);
    frame(&mut app, &context, vec![key(egui::Modifiers::NONE)]);
    // No configured engine: the send action opens settings, never consumes the draft.
    assert!(app.ai.settings_open);
    assert!(app.ai.prompt.contains("先保留布局"));
}

#[test]
fn ime_confirmation_never_submits_the_draft() {
    let mut app = app();
    let context = egui::Context::default();
    focus_prompt(&mut app, &context);
    frame(
        &mut app,
        &context,
        vec![
            egui::Event::Ime(egui::ImeEvent::Enabled),
            egui::Event::Ime(egui::ImeEvent::Preedit("peise".into())),
        ],
    );
    frame(
        &mut app,
        &context,
        vec![
            egui::Event::Ime(egui::ImeEvent::Commit("配色".into())),
            key(egui::Modifiers::NONE),
        ],
    );
    assert!(!app.ai.settings_open);
    assert!(app.ai.messages.is_empty());
}

#[test]
fn read_only_mode_blocks_actual_document_mutation() {
    let mut app = app();
    app.ai.discuss_only = true;
    let before = app.project.clone();
    let context = egui::Context::default();
    let call = automation::ToolCall {
        name: "rpshop_batch_edit".into(),
        arguments: json!({"operations":[{"op":"update_project","name":"不应修改"}]}),
    };
    assert!(app.execute_ai_tool(&context, &call).is_err());
    assert_eq!(app.project, before);
    assert!(app.undo.is_empty());
    let call = automation::ToolCall {
        name: "rpshop_read_pages".into(),
        arguments: json!({}),
    };
    assert!(app.execute_ai_tool(&context, &call).is_ok());
}

#[test]
fn unicode_model_labels_are_safely_shortened() {
    assert_eq!(short_model("中文模型名字很长", 5), "中文模型…");
    assert_eq!(short_model("opus", 22), "opus");
}

#[test]
fn ai_edit_reveals_but_undo_and_stop_immediately_clear_animation() {
    let mut app = app();
    let context = egui::Context::default();
    frame(&mut app, &context, vec![]);
    let call = automation::ToolCall {
        name: "rpshop_batch_edit".into(),
        arguments: json!({"operations":[{"op":"create_widget","kind":"Text","properties":{"text":"正在绘制"}}]}),
    };
    app.execute_ai_tool(&context, &call).unwrap();
    let time = context.input(|input| input.time);
    assert!(app.ai_reveal.is_active(time));
    assert_eq!(app.undo.len(), 1);
    app.undo();
    assert!(!app.ai_reveal.is_active(time));
    assert!(app.project.pages[0].widgets.is_empty());
    app.redo();
    assert!(!app.ai_reveal.is_active(time));
    let before = Project::blank();
    app.ai_reveal.record(&before, &app.project, time);
    app.ai_stop_design();
    assert!(!app.ai_reveal.is_active(time));
}
