//! Mock API -> runtime -> permission UI -> live editor, with no real credentials/model.
use super::*;
use crate::ui::tests::{click, fixture, frame, text_center};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

#[test]
fn mock_model_updates_real_canvas_after_ui_approval_and_is_undoable() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let mock = std::thread::spawn(move || {
        for turn in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header == "\r\n" {
                    break;
                }
                if let Some((key, value)) = header.split_once(':')
                    && key.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse::<usize>().unwrap();
                }
            }
            let mut bytes = vec![0; length];
            reader.read_exact(&mut bytes).unwrap();
            let request: Value = serde_json::from_slice(&bytes).unwrap();
            assert!(
                request["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|tool| tool["function"]["name"] == "rpshop_read_layers")
            );
            let chunk = if turn == 0 {
                let arguments = json!({"operations":[
                    {"op":"create_widget","id":2,"kind":"Text","properties":{"text":"从对话到真实画布","x":80,"y":90}},
                    {"op":"create_widget","id":3,"kind":"Rectangle","properties":{"name":"背景","x":40,"y":60}},
                    {"op":"group","widget_ids":[2,3],"name":"页头"},
                    {"op":"update_widget","widget_id":2,"properties":{"name":"页面标题"}}
                ]}).to_string();
                json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"live-edit","type":"function","function":{"name":"rpshop_batch_edit","arguments":arguments}}]},"finish_reason":"tool_calls"}]})
            } else {
                assert!(
                    request["messages"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|message| message["role"] == "tool")
                );
                json!({"choices":[{"delta":{"content":"设计已更新，可以在左侧画布继续编辑。"},"finish_reason":"stop"}]})
            };
            let body = format!("data: {chunk}\n\ndata: [DONE]\n\n");
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            stream.flush().unwrap();
        }
    });
    let mut app = fixture(Project::blank());
    let original = app.project.clone();
    let mut config = ai::EngineConfig::new(ai::EngineKind::OpenAiCompatible);
    config.model = "local-mock".into();
    config.api_base = Some(format!("http://{address}/v1"));
    app.ai.selected_engine = config.id.clone();
    app.ai.settings.engines.push(config);
    app.ai.last_project_key = app.ai_project_key();
    app.ai.visible = true;
    app.ai.prompt = "在当前页面增加一个标题".into();
    app.ai_start();
    let context = egui::Context::default();
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.ai.pending_tool.is_none() && Instant::now() < deadline {
        frame(&mut app, &context, vec![]);
        std::thread::yield_now();
    }
    assert!(app.ai.pending_tool.is_some(), "{}", app.ai.state);
    assert_eq!(app.project, original, "Tool cannot mutate before consent");
    let output = frame(&mut app, &context, vec![]);
    let approve = text_center(&output, "允许本次 RPShop 工具操作");
    click(&mut app, &context, approve);
    while app.ai.running.is_some() && Instant::now() < deadline {
        frame(&mut app, &context, vec![]);
        std::thread::yield_now();
    }
    assert!(app.ai.running.is_none(), "{}", app.ai.state);
    assert_eq!(app.project.pages[0].widgets[0].text, "从对话到真实画布");
    let layers = app
        .execute_ai_tool(
            &context,
            &automation::ToolCall {
                name: "rpshop_read_layers".into(),
                arguments: json!({}),
            },
        )
        .unwrap();
    assert_eq!(layers["layers"][0]["name"], "页头");
    assert_eq!(layers["layers"][0]["children"][1]["name"], "页面标题");
    assert_eq!(app.undo.len(), 1);
    app.undo();
    assert_eq!(app.project, original);
    app.redo();
    assert_eq!(app.project.pages[0].widgets[0].text, "从对话到真实画布");
    mock.join().unwrap();
}

#[test]
fn active_turn_secrets_are_rejected_before_document_transaction() {
    let mut app = fixture(Project::blank());
    let before = app.project.clone();
    app.ai.protected_literals.push("unit-secret".into());
    let call = automation::ToolCall {
        name: "rpshop_batch_edit".into(),
        arguments: json!({"operations":[{"op":"update_project","name":"unit-secret"}]}),
    };
    assert!(
        app.execute_ai_tool(&egui::Context::default(), &call)
            .is_err()
    );
    assert_eq!(app.project, before);
    assert!(app.undo.is_empty());
}

#[test]
fn ai_sidebar_keeps_existing_page_tree_and_editable_canvas() {
    let mut app = fixture(Project::default());
    app.ai.visible = true;
    let context = egui::Context::default();
    let output = frame(&mut app, &context, vec![]);
    let heading = text_center(&output, "AI 设计");
    assert!(heading.x > 1000.0);
    assert!(text_center(&output, "页面").x < 224.0);
    assert!(!app.ai.artifact_open);
}
