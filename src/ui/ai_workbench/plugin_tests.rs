use super::*;
use crate::ui::tests::{click, fixture, frame, text_center};
use serde_json::json;

fn node_available() -> bool {
    std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn submit(app: &mut App, prompt: &str) {
    app.ai.last_project_key = app.ai_project_key();
    app.ai.visible = true;
    app.ai.prompt = prompt.into();
    app.ai_start();
    assert!(app.ai.running.is_some(), "{}", app.ai.state);
}

fn poll_until(app: &mut App, context: &egui::Context, condition: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition(app) && Instant::now() < deadline {
        frame(app, context, vec![]);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(condition(app), "{}", app.ai.state);
}

#[test]
fn selected_plugin_runs_through_consent_denial_and_undo() {
    if !node_available() {
        return;
    }
    let mut app = fixture(Project::blank());
    let plugin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/ai-plugin.mjs");
    app.ai_add_local_engine(&plugin).unwrap();
    let id = app.ai.selected_engine.clone();
    app.ai_add_local_engine(&plugin).unwrap();
    assert_eq!(app.ai.selected_engine, id);
    assert_eq!(app.ai.settings.engines.len(), 1);
    let context = egui::Context::default();
    let original = app.project.clone();

    for (allow, label) in [(false, "拒绝"), (true, "允许本次 Ingenaro 工具操作")] {
        submit(&mut app, "添加示例标题");
        poll_until(&mut app, &context, |app| app.ai.pending_tool.is_some());
        assert_eq!(app.project, original);
        assert!(app.ai.agent_server.is_none());
        let approval = text_center(&frame(&mut app, &context, vec![]), label);
        click(&mut app, &context, approval);
        poll_until(&mut app, &context, |app| app.ai.running.is_none());
        assert!(app.ai.task_error.is_none(), "{:?}", app.ai.task_error);
        if allow {
            assert_eq!(app.project.pages[0].widgets[0].text, "来自自定义引擎的标题");
            assert_eq!(app.undo.len(), 1);
            app.undo();
        } else {
            assert!(app.undo.is_empty());
        }
        assert_eq!(app.project, original);
    }

    app.ai_change_permission(permissions::PermissionMode::ReadOnly);
    submit(&mut app, "添加示例标题");
    poll_until(&mut app, &context, |app| app.ai.pending_tool.is_some());
    let approval = text_center(
        &frame(&mut app, &context, vec![]),
        "允许本次 Ingenaro 工具操作",
    );
    click(&mut app, &context, approval);
    poll_until(&mut app, &context, |app| app.ai.running.is_none());
    assert_eq!(app.project, original);
}

#[test]
fn mcp_plugin_manager_requires_approval_and_reports_the_catalog() {
    let mut app = fixture(Project::blank());
    let context = egui::Context::default();
    app.plugins = Default::default();
    app.ai.last_project_key = app.ai_project_key();
    let server = automation::start(
        app.ai
            .client
            .scoped(app.ai_project_key(), app.ai.external_cancelled.clone()),
    )
    .unwrap();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let rpc = |method: &str, params: serde_json::Value| {
        client
            .post(server.endpoint())
            .bearer_auth(server.token())
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
    };
    let tool = automation::OPEN_PLUGIN_MANAGER_TOOL;
    let inventory: serde_json::Value = rpc("tools/list", json!({})).send().unwrap().json().unwrap();
    assert!(
        inventory["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == tool)
    );

    let catalog = rpc(
        "tools/call",
        json!({"name":automation::LIST_PLUGINS_TOOL,"arguments":{}}),
    );
    let response =
        std::thread::spawn(move || catalog.send().unwrap().json::<serde_json::Value>().unwrap());
    poll_until(&mut app, &context, |_| response.is_finished());
    let listed = response.join().unwrap();
    let plugin = &listed["result"]["structuredContent"]["plugins"][0];
    assert_eq!(plugin["id"], "ai_tool");
    assert_eq!(plugin["installed"], false);
    assert!(app.ai.pending_tool.is_none());

    for (arguments, is_error) in [
        (json!({"path":"/untrusted/plugin"}), true),
        (json!({}), false),
    ] {
        let request = rpc("tools/call", json!({"name":tool,"arguments":arguments}));
        let response = std::thread::spawn(move || {
            request.send().unwrap().json::<serde_json::Value>().unwrap()
        });
        poll_until(&mut app, &context, |app| app.ai.pending_tool.is_some());
        assert!(!app.plugins.open);
        let approve = text_center(
            &frame(&mut app, &context, vec![]),
            "允许本次 Ingenaro 工具操作",
        );
        click(&mut app, &context, approve);
        assert_eq!(response.join().unwrap()["result"]["isError"], is_error);
        assert_eq!(app.plugins.open, !is_error);
        assert!(!app.ai.visible);
        assert!(app.ai.settings.engines.is_empty());
    }
}

#[test]
fn incompatible_plugin_cannot_receive_turn_data() {
    if !node_available() {
        return;
    }
    let directory =
        std::env::temp_dir().join(format!("ingenaro-plugin-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let executable = directory.join("incompatible.mjs");
    let received = directory.join("unexpected-start");
    std::fs::write(
        &executable,
        r#"import { createInterface } from 'node:readline';
import { writeFileSync } from 'node:fs';
for await (const line of createInterface({input:process.stdin})) {
  if (JSON.parse(line).type === 'hello') {
    console.log(JSON.stringify({type:'hello',protocol:'ai-design-engine',version:999}));
  } else {
    writeFileSync(new URL('./unexpected-start', import.meta.url), 'data received');
  }
}
"#,
    )
    .unwrap();
    let mut app = fixture(Project::blank());
    app.ai_add_local_engine(&executable).unwrap();
    let context = egui::Context::default();
    submit(&mut app, "添加示例标题");
    poll_until(&mut app, &context, |app| app.ai.running.is_none());
    assert!(
        app.ai
            .task_error
            .as_deref()
            .is_some_and(|error| error.contains("版本不兼容"))
    );
    assert!(!received.exists());
    assert!(app.project.pages[0].widgets.is_empty());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn plugin_edits_are_atomic_and_pending_tools_cannot_cross_documents() {
    if !node_available() {
        return;
    }
    let directory =
        std::env::temp_dir().join(format!("ingenaro-plugin-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let executable = directory.join("invalid-batch.mjs");
    std::fs::write(
        &executable,
        r#"import { createInterface } from 'node:readline';
const send = (frame) => console.log(JSON.stringify(frame));
for await (const line of createInterface({input:process.stdin})) {
  const frame = JSON.parse(line);
  if (frame.type === 'hello') {
    send(frame);
  } else if (frame.type === 'start') {
    send({ type:'tool_call', id:'edit', name:'rpshop_batch_edit', arguments:{ operations:[
      {op:'create_widget', kind:'Text', properties:{text:'must not remain'}},
      {op:'update_widget', widget_id:9999, properties:{text:'invalid target'}}
    ] } });
  } else if (frame.type === 'tool_result') {
    send(frame.is_error ? {type:'done'} : {type:'error',message:'invalid batch succeeded'});
    break;
  }
}
"#,
    )
    .unwrap();
    let mut app = fixture(Project::blank());
    let original = app.project.clone();
    let context = egui::Context::default();
    app.ai_add_local_engine(&executable).unwrap();

    submit(&mut app, "测试失败批次");
    poll_until(&mut app, &context, |app| app.ai.pending_tool.is_some());
    let approval = text_center(
        &frame(&mut app, &context, vec![]),
        "允许本次 Ingenaro 工具操作",
    );
    click(&mut app, &context, approval);
    poll_until(&mut app, &context, |app| app.ai.running.is_none());
    assert!(app.ai.task_error.is_none());
    assert_eq!(app.project, original);
    assert!(app.undo.is_empty());

    submit(&mut app, "测试工程切换");
    poll_until(&mut app, &context, |app| app.ai.pending_tool.is_some());
    let cancelled = app.ai.task_cancelled.clone();
    app.ai_document_replaced();
    assert!(cancelled.load(Ordering::Acquire));
    assert!(app.ai.pending_tool.is_none());
    assert!(app.ai.running.is_none());
    assert_eq!(app.project, original);
    std::fs::remove_dir_all(directory).unwrap();
}
