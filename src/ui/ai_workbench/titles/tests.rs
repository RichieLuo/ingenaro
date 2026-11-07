use super::*;
use crate::ui::tests::{click, fixture, frame, text_center};

#[test]
fn sidebar_history_opens_a_conversation_from_its_request_excerpt() {
    let directory = std::env::temp_dir().join(format!("rpshop-history-{}", uuid::Uuid::new_v4()));
    let mut app = fixture(Project::blank());
    app.ai.visible = true;
    app.ai.last_project_key = app.ai_project_key();
    app.ai.selected_engine = "history-fixture".into();
    let scope = app.ai.scope(&app.ai_project_key()).unwrap();
    let mut store = design::Store::open(&directory).unwrap();
    let id = store
        .create_session(&scope, design::ArtifactKind::Native, UNTITLED)
        .unwrap();
    let prompt = "给订单页加一个退款入口";
    store
        .append_message(&scope, id, design::ChatRole::User, prompt)
        .unwrap();
    app.ai.store = Some(store);

    let ctx = egui::Context::default();
    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_center(&output, "历史"));
    assert!(app.ai.history_open);
    let output = frame(&mut app, &ctx, vec![]);
    let row = text_center(&output, prompt);
    let sidebar = ctx
        .data(|data| data.get_temp::<Rect>(egui::Id::new("ai-chat-panel-test")))
        .unwrap();
    assert!(sidebar.contains(row));
    click(&mut app, &ctx, row);
    assert!(!app.ai.history_open);
    assert_eq!(app.ai.session_id, Some(id));
    assert_eq!(app.ai.messages[0].text, prompt);

    drop(app);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn local_model_titles_new_and_legacy_sessions_without_adding_chat_messages() {
    let Ok(node) = std::process::Command::new("node").arg("--version").output() else {
        eprintln!("Skipping mock Codex title test because Node.js is unavailable");
        return;
    };
    assert!(node.status.success());
    let document_id = format!("rpshop-title-test-{}", uuid::Uuid::new_v4());
    let directory = std::env::temp_dir().join(&document_id);
    let workspace = std::env::temp_dir()
        .join("rpshop-agent-workspaces")
        .join(&document_id);
    let mut store = design::Store::open(&directory).unwrap();
    let script = directory.join("codex.js");
    std::fs::write(&script, CODEX_TITLE_MOCK).unwrap();
    let mut app = fixture(Project::blank());
    app.ai.document_id = document_id.clone();
    app.ai.last_project_key = document_id;
    app.ai.visible = true;
    app.ai.kind = design::ArtifactKind::Html;
    let mut engine = ai::EngineConfig::new(ai::EngineKind::Codex);
    engine.executable = Some(script);
    engine.model = "test-title-model".into();
    app.ai.selected_engine = engine.id.clone();
    app.ai.settings.engines.push(engine);
    let scope = app.ai.scope(&app.ai_project_key()).unwrap();
    let legacy = store
        .create_session(&scope, design::ArtifactKind::Native, UNTITLED)
        .unwrap();
    store
        .append_message(&scope, legacy, design::ChatRole::User, "调整库存列表的排序")
        .unwrap();
    app.ai.store = Some(store);

    app.ai.prompt = "给订单页加一个退款入口".into();
    app.ai_start();
    let new = app.ai.session_id.unwrap();
    let ctx = egui::Context::default();
    drive_until_titled(&mut app, &ctx, &scope, new, "订单退款入口");
    assert_eq!(
        app.ai
            .messages
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>(),
        ["给订单页加一个退款入口", "已完成本轮回复"]
    );
    assert_eq!(
        app.ai
            .store
            .as_ref()
            .unwrap()
            .session(&scope, legacy)
            .unwrap()
            .title,
        UNTITLED
    );

    app.ai.history_open = true;
    drive_until_titled(&mut app, &ctx, &scope, legacy, "库存列表排序");
    assert_eq!(app.ai.session_id, Some(new));
    drop(app);

    let reopened = design::Store::open(&directory).unwrap();
    assert_eq!(reopened.session(&scope, new).unwrap().title, "订单退款入口");
    assert_eq!(
        reopened.session(&scope, legacy).unwrap().title,
        "库存列表排序"
    );
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
    std::fs::remove_dir_all(workspace).unwrap();
}

fn drive_until_titled(
    app: &mut App,
    ctx: &egui::Context,
    scope: &design::Scope,
    id: u64,
    title: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.poll_ai(ctx);
        let session = app.ai.store.as_ref().unwrap().session(scope, id).unwrap();
        if session.title == title && app.ai.running.is_none() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "title: {:?}, chat state: {}",
            session.title,
            app.ai.state
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

const CODEX_TITLE_MOCK: &str = r#"
const assert = require('node:assert/strict');
const readline = require('node:readline');
const send = frame => console.log(JSON.stringify(frame));
let titleRun = false;
readline.createInterface({input: process.stdin}).on('line', line => {
  const m = JSON.parse(line);
  if (m.method === 'initialize') send({id: m.id, result: {}});
  if (m.method === 'mcpServerStatus/list') {
    send({id: m.id, result: {data: [{name: 'inherited'}], nextCursor: null}});
  }
  if (m.method === 'thread/start') {
    titleRun = m.params.ephemeral === true;
    assert.equal(m.params.model, 'test-title-model');
    if (titleRun) {
      assert.deepEqual(m.params.dynamicTools, []);
      assert.equal(m.params.config['mcp_servers.inherited.enabled'], false);
      assert.match(m.params.developerInstructions, /会话标题/);
    }
    send({id: m.id, result: {thread: {id: 'mock-thread'}}});
  }
  if (m.method === 'turn/start') {
    const input = m.params.input[0].text;
    const inventory = input.includes('调整库存列表的排序');
    assert(inventory || input.includes('给订单页加一个退款入口'));
    assert.equal(input.includes('仅输出一行标题'), titleRun);
    const text = titleRun ? (inventory ? '库存列表排序' : '「订单退款入口」') : '已完成本轮回复';
    const notify = (method, params) => send({method, params: {threadId: 'mock-thread', ...params}});
    notify('turn/started', {turn: {id: 'mock-turn'}});
    notify('item/agentMessage/delta', {itemId: 'text', delta: text});
    notify('turn/completed', {turn: {status: 'completed'}});
  }
});
"#;
