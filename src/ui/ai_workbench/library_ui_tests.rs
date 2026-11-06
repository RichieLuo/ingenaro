use super::*;
use crate::ui::tests::{fixture, frame};
use serde_json::json;

#[test]
fn agent_can_query_and_create_real_table_in_current_editor_then_undo() {
    let mut app = fixture(Project::blank());
    let before = app.project.clone();
    let context = egui::Context::default();
    let library = app
        .execute_ai_tool(
            &context,
            &automation::ToolCall {
                name: "rpshop_read_library".into(),
                arguments: json!({"item_id":"table"}),
            },
        )
        .unwrap();
    assert!(library.to_string().contains("table"));
    let output=app.execute_ai_tool(&context,&automation::ToolCall{name:"rpshop_batch_edit".into(),arguments:json!({"operations":[{"op":"create_library_item","item_id":"table","x":80,"y":96,"width":800,"height":240,"texts":["名称","类型","状态"]}]})}).unwrap();
    assert_eq!(app.project.pages[0].widgets.len(), 12);
    assert_eq!(app.project.pages[0].widgets[0].text, "名称");
    assert_eq!(app.undo.len(), 1);
    assert_eq!(
        output["results"][0]["widget_ids"].as_array().unwrap().len(),
        12
    );
    app.project.validate().unwrap();
    frame(&mut app, &context, vec![]);
    app.undo();
    assert_eq!(app.project, before);
    app.redo();
    assert_eq!(app.project.pages[0].widgets.len(), 12);
}
