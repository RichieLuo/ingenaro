//! Local Node app-server double -> Codex adapter -> queue -> host consent -> document history.
//! No provider API, installed Codex, credentials, or persistent user settings are used.
use super::*;
use crate::ui::tests::{click, fixture, frame, text_center};
use serde_json::{Value, json};
use std::path::PathBuf;

const FIRST_PROMPT: &str = "先处理第一条，等待我补充图片";
const SECOND_PROMPT: &str = "第二条请看图片修改选中引用元件";
const FIRST_REPLY: &str = "CLI 第一轮完成";
const SECOND_REPLY: &str = "CLI 图片和固定引用已验证，真实元件已更新";
const EDITED_TEXT: &str = "由排队的本地 CLI 工具事务更新";
const REFERENCE_CSS: &str = ":root { --accent: #12805c; }";

struct CliFixture {
    app: App,
    dir: PathBuf,
    workspace: PathBuf,
}

impl CliFixture {
    fn new() -> Option<Self> {
        match std::process::Command::new("node").arg("--version").output() {
            Ok(output) => assert!(output.status.success(), "node --version failed"),
            Err(error) => {
                eprintln!(
                    "SKIP cli_queue: Node.js is unavailable ({error}); the local Codex app-server regression was not exercised"
                );
                return None;
            }
        }
        let document_id = format!("rpshop-cli-queue-{}", uuid::Uuid::new_v4());
        let dir = std::env::temp_dir().join(&document_id);
        std::fs::create_dir(&dir).unwrap();
        let mut app = fixture(Project::default());
        app.ai.document_id = document_id.clone();
        let mut engine = ai::EngineConfig::new(ai::EngineKind::Codex);
        engine.executable = Some(dir.join("mock.js"));
        app.ai.selected_engine = engine.id.clone();
        app.ai.settings.engines.push(engine);
        app.ai.last_project_key = app.ai_project_key();
        app.ai.visible = true;
        let fixture = Self {
            app,
            dir,
            workspace: std::env::temp_dir()
                .join("rpshop-agent-workspaces")
                .join(document_id),
        };
        std::fs::write(fixture.dir.join("mock.js"), CODEX_QUEUE).unwrap();
        Some(fixture)
    }

    fn drive_until(&mut self, ctx: &egui::Context, phase: &str, done: impl Fn(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let failure = std::fs::read_to_string(self.dir.join("failure.txt")).unwrap_or_default();
            assert!(failure.is_empty(), "{phase}: Node app-server: {failure}");
            assert!(
                self.app.ai.task_error.is_none(),
                "{phase}: {}",
                self.app.ai.state
            );
            if done(&self.app) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{phase} timed out: {}; changes: {:?}",
                self.app.ai.state,
                self.app.ai.response.steps
            );
            frame(&mut self.app, ctx, vec![]);
            std::thread::park_timeout(Duration::from_millis(2));
        }
    }

    fn read(&self, name: &str) -> Value {
        serde_json::from_slice(&std::fs::read(self.dir.join(name)).unwrap()).unwrap()
    }

    fn cleanup(&mut self) -> Result<(), String> {
        let control = self.app.ai.running.as_ref().map(|s| s.control.clone());
        self.app.ai.cancel();
        let deadline = Instant::now() + Duration::from_secs(5);
        while control.as_ref().is_some_and(|c| !c.is_finished()) {
            if Instant::now() >= deadline {
                return Err("local CLI did not stop before fixture cleanup".into());
            }
            std::thread::park_timeout(Duration::from_millis(5));
        }
        for path in [&self.workspace, &self.dir] {
            match std::fs::remove_dir_all(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("{}: {error}", path.display())),
            }
        }
        Ok(())
    }
}

impl Drop for CliFixture {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            eprintln!("cli_queue fixture cleanup: {error}");
        }
    }
}

// Hand-written JSON-RPC app-server, spawned through EngineConfig.executable (.js).
// The release file is an explicit barrier, not a delay guessed to outlast the GUI.
const CODEX_QUEUE: &str = r#"
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const file = name => path.join(__dirname, name);
const record = (name, value) => fs.writeFileSync(file(name), JSON.stringify(value));
const send = value => console.log(JSON.stringify(value));
const fail = error => {
  fs.writeFileSync(file('failure.txt'), error.stack || String(error));
  process.exit(1);
};
const check = action => { try { action(); } catch (error) { fail(error); } };
const session = 'cli-queue-session';
let round = 0;
let requestedEdit = false;
setTimeout(() => fail(new Error('mock app-server deadline exceeded')), 30000).unref();
check(() => assert.deepEqual(process.argv.slice(2, 5), ['app-server', '--listen', 'stdio://']));
readline.createInterface({input: process.stdin}).on('line', line => check(() => {
  const m = JSON.parse(line);
  if (m.method === 'initialize') {
    assert.equal(m.params.capabilities.experimentalApi, true);
    send({id: m.id, result: {userAgent: 'rpshop-cli-queue-mock'}});
  } else if (m.method === 'initialized') {
    // Notification, no response.
  } else if (m.method === 'mcpServerStatus/list') {
    send({id: m.id, result: {data: [{name: 'inherited-queue-mock'}], nextCursor: null}});
  } else if (m.method === 'thread/start' || m.method === 'thread/resume') {
    assert.equal(m.params.sandbox, 'read-only');
    assert.equal(m.params.config['mcp_servers.inherited-queue-mock.enabled'], false);
    if (m.method === 'thread/start') {
      round = 1;
      assert(!fs.existsSync(file('first-turn.json')), 'queue must resume its original CLI session');
      const edit = m.params.dynamicTools.find(tool => tool.name === 'rpshop_batch_edit');
      assert.equal(edit.type, 'function');
      assert.equal(edit.inputSchema.type, 'object');
      assert(m.params.dynamicTools.some(tool => tool.name === 'rpshop_read_reference_file'));
    } else {
      round = 2;
      assert.equal(m.params.threadId, session);
      assert.equal(m.params.excludeTurns, true);
      assert(fs.existsSync(file('first-completed.json')), 'follow-up started before first completion');
    }
    record(`thread-${round}.json`, m);
    send({id: m.id, result: {thread: {id: session}}});
  } else if (m.method === 'turn/start') {
    assert.equal(m.params.threadId, session);
    assert.equal(m.params.sandboxPolicy.type, 'readOnly');
    const expected = JSON.parse(fs.readFileSync(file('expected.json'), 'utf8'));
    const input = m.params.input;
    assert.equal(input[0].type, 'text');
    if (round === 1) {
      assert.equal(input.length, 1, 'first turn must not steal the queued image');
      assert(input[0].text.endsWith('\n\nUser request:\n' + expected.firstPrompt));
      assert(!input[0].text.includes('admin.css'));
      send({id: m.id, result: {turn: {id: 'turn-1'}}});
      record('first-turn.json', m);
      const barrier = setInterval(() => check(() => {
        if (!fs.existsSync(file('release-first'))) return;
        clearInterval(barrier);
        record('first-completed.json', {completed: true});
        send({method: 'item/agentMessage/delta', params: {threadId: session, delta: expected.firstReply}});
        send({method: 'turn/completed', params: {threadId: session, turn: {id: 'turn-1', status: 'completed'}}});
      }), 10);
    } else {
      assert.equal(round, 2);
      assert.equal(input.length, 2, 'follow-up must contain exactly its own image and text');
      assert.equal(input[1].type, 'image');
      assert.equal(Object.hasOwn(input[1], 'path'), false, 'use image.url, not localImage.path');
      assert.equal(input[1].url, expected.imageUrl);
      const prefix = 'data:image/png;base64,';
      assert(input[1].url.startsWith(prefix));
      const encoded = input[1].url.slice(prefix.length);
      const png = Buffer.from(encoded, 'base64');
      assert.equal(png.toString('base64'), encoded);
      assert.equal(png.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
      assert.equal(png.readUInt32BE(16), 1);
      assert.equal(png.readUInt32BE(20), 1);
      const text = input[0].text;
      assert(text.endsWith('\n\nUser request:\n' + expected.secondPrompt));
      assert(text.includes('admin.css'));
      assert(!text.includes(expected.referenceCss), 'source is sent only when the model reads it');
      const frozenPrefix = '这条需求发送时固定的引用上下文：';
      const currentPrefix = '\n执行时当前工程页面摘要：';
      const start = text.indexOf(frozenPrefix);
      const end = text.indexOf(currentPrefix, start);
      assert(start >= 0 && end > start, 'fixed context must be present');
      const frozen = JSON.parse(text.slice(start + frozenPrefix.length, end));
      assert.deepEqual(frozen, expected.context, 'selection and widget properties must be frozen at enqueue');
      const summaryEnd = text.indexOf('。排队期间工程可能变化，', end);
      const current = JSON.parse(text.slice(end + currentPrefix.length, summaryEnd));
      assert.equal(current.page_names.find(page => page.id === expected.pageId).name, expected.laterPageName);
      record('second-turn.json', m);
      send({id: m.id, result: {turn: {id: 'turn-2'}}});
      send({id: 38, method: 'item/tool/call', params: {
        threadId: session, turnId: 'turn-2', callId: 'reference-list', tool: 'rpshop_read_reference_file',
        arguments: {}
      }});
    }
  } else if (m.id === 38 && m.result) {
    assert.equal(m.result.success, true);
    const result = JSON.parse(m.result.contentItems[0].text);
    assert.deepEqual(result.files.map(file => [file.file_id, file.name]), [[1, 'admin.css']]);
    send({id: 39, method: 'item/tool/call', params: {
      threadId: session, turnId: 'turn-2', callId: 'reference-read', tool: 'rpshop_read_reference_file',
      arguments: {file_id: 1}
    }});
  } else if (m.id === 39 && m.result) {
    assert.equal(m.result.success, true);
    const result = JSON.parse(m.result.contentItems[0].text);
    const expected = JSON.parse(fs.readFileSync(file('expected.json'), 'utf8'));
    assert.equal(result.text, expected.referenceCss);
    assert.equal(result.next_offset, null);
    send({id: 37, method: 'item/tool/call', params: {
      threadId: session, turnId: 'turn-2', callId: 'source-from-chat', tool: 'rpshop_read_reference_file',
      arguments: {local_path: expected.sourceFolder, path: 'admin.css'}
    }});
  } else if (m.id === 37 && m.result) {
    assert.equal(m.result.success, true);
    const result = JSON.parse(m.result.contentItems[0].text);
    const expected = JSON.parse(fs.readFileSync(file('expected.json'), 'utf8'));
    assert.equal(result.text, expected.referenceCss);
    requestedEdit = true;
    send({id: 40, method: 'item/tool/call', params: {
      threadId: session, turnId: 'turn-2', callId: 'queued-edit', tool: 'rpshop_batch_edit',
      arguments: {operations: [{op: 'update_widget', page_id: expected.pageId,
        widget_id: expected.widgetId, properties: {text: expected.editedText}}]}
    }});
  } else if (m.id === 40 && m.result) {
    assert(requestedEdit, 'unexpected dynamic tool response');
    assert.equal(m.result.success, true, 'host must execute the real document transaction');
    assert.equal(m.result.contentItems.length, 1);
    assert.equal(m.result.contentItems[0].type, 'inputText');
    const result = JSON.parse(m.result.contentItems[0].text);
    const expected = JSON.parse(fs.readFileSync(file('expected.json'), 'utf8'));
    assert.equal(result.changed, true);
    assert.deepEqual(result.results, [{op: 'update_widget', widget_id: expected.widgetId}]);
    record('tool-result.json', m.result);
    send({method: 'item/agentMessage/delta', params: {threadId: session, delta: expected.secondReply}});
    send({method: 'turn/completed', params: {threadId: session, turn: {id: 'turn-2', status: 'completed'}}});
  } else if (m.method === 'turn/interrupt') {
    process.exit(0);
  } else {
    throw new Error('unexpected client frame: ' + line);
  }
}));
"#;

#[test]
fn cli_queue_codex_attachments_and_selection_reach_document_transaction_and_undo_redo() {
    let Some(mut f) = CliFixture::new() else {
        return;
    };
    let ctx = egui::Context::default();
    let target = f.app.project.pages[0].widgets[0].clone();
    let other_id = f.app.project.pages[0].widgets[1].id;
    let page_id = f.app.project.pages[0].id;
    let selected = json!({
        "page_id": page_id,
        "page_name": f.app.project.pages[0].name,
        "widget_ids": [target.id],
        "widgets": [target],
    });
    let frozen_context = json!({"attached_context": selected, "selected_widgets": selected});
    let image =
        ai::ImageAttachment::from_rgba("queued-reference.png", 1, 1, &[23, 145, 211, 255]).unwrap();
    let image_id = image.id().to_owned();
    let later_page_name = "入队后由用户重命名的页面";
    let source_folder = f.dir.join("admin source");
    std::fs::create_dir(&source_folder).unwrap();
    std::fs::write(source_folder.join("admin.css"), REFERENCE_CSS).unwrap();
    let second_prompt = format!(
        "{SECOND_PROMPT}\n同时参考 `{}` 中的配色。",
        source_folder.display()
    );
    std::fs::write(
        f.dir.join("expected.json"),
        serde_json::to_vec(&json!({
            "firstPrompt": FIRST_PROMPT, "secondPrompt": second_prompt,
            "firstReply": FIRST_REPLY, "secondReply": SECOND_REPLY,
            "imageUrl": image.data_url(), "context": frozen_context,
            "referenceCss": REFERENCE_CSS,
            "sourceFolder": source_folder,
            "pageId": page_id, "widgetId": target.id,
            "laterPageName": later_page_name, "editedText": EDITED_TEXT,
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(!f.app.ai.persist);
    assert!(f.app.ai.store.is_none());
    f.app.ai.prompt = FIRST_PROMPT.into();
    f.app.ai_submit();
    assert!(f.app.ai.running.is_some(), "{}", f.app.ai.state);
    let first_control = f.app.ai.running.as_ref().unwrap().control.clone();
    let first_turn = f.dir.join("first-turn.json");
    f.drive_until(&ctx, "first CLI turn blocked", |app| {
        first_turn.is_file() && app.ai.binding.is_some()
    });
    assert_eq!(
        f.app.ai.binding.as_ref().unwrap().session_id,
        "cli-queue-session"
    );
    assert!(!first_control.is_finished());

    // No '@' syntax and no manual reference construction: ai_submit captures selection.
    f.app.selected.insert(target.id);
    f.app.ai.images.push(image);
    let path = f.dir.join("admin.css");
    std::fs::write(&path, REFERENCE_CSS).unwrap();
    f.app
        .ai
        .reference_files
        .files
        .push(automation::ReferenceFile::open(&path).unwrap());
    f.app.ai.prompt = second_prompt.clone();
    f.app.ai_submit();
    assert!(f.app.ai.prompt.is_empty());
    assert!(f.app.ai.images.is_empty());
    assert!(f.app.ai.reference_files.files.is_empty());
    assert_eq!(f.app.ai.queue.len(), 1);
    let queued = f.app.ai.queue.entries().next().unwrap();
    assert_eq!(queued.context_mode, 2);
    assert_eq!(queued.context, frozen_context);
    assert_eq!(queued.images[0].id(), image_id);
    assert_eq!(queued.reference_files[0].name, "admin.css");
    assert_eq!(f.app.ai.messages.len(), 1, "queued input is not sent yet");
    assert!(!f.dir.join("second-turn.json").exists());

    // A real intervening user edit and another selection must not refresh the queued snapshot.
    let before_user_edit = f.app.project.clone();
    f.app.project.pages[0].name = later_page_name.into();
    f.app.project.pages[0].widgets[0].name = "用户已改名，不能被旧快照覆盖".into();
    f.app.project.pages[0].widgets[0].text = "用户在排队期间编辑的内容".into();
    f.app.selected = BTreeSet::from([other_id]);
    f.app.remember(before_user_edit);
    let later_image =
        ai::ImageAttachment::from_rgba("later-unsent.png", 1, 1, &[255, 0, 0, 255]).unwrap();
    let later_image_id = later_image.id().to_owned();
    f.app.ai.images.push(later_image);
    std::fs::write(&path, "new draft CSS").unwrap();
    f.app
        .ai
        .reference_files
        .files
        .push(automation::ReferenceFile::open(&path).unwrap());
    for _ in 0..3 {
        frame(&mut f.app, &ctx, vec![]);
    }
    assert_eq!(f.app.ai.references.value()["widget_ids"], json!([other_id]));
    assert_eq!(f.app.ai.queue.len(), 1);
    assert!(!first_control.is_finished());
    assert!(!f.dir.join("thread-2.json").exists());
    assert!(!f.dir.join("second-turn.json").exists());
    let before_transaction = f.app.project.clone();
    let undo_before = f.app.undo.len();
    assert_eq!(undo_before, 1);

    std::fs::write(f.dir.join("release-first"), b"release").unwrap();
    f.drive_until(
        &ctx,
        "queued CLI dynamic tool reaches host consent",
        |app| app.ai.pending_tool.is_some(),
    );
    assert!(first_control.is_finished());
    assert_eq!(f.app.ai.queue.len(), 0);
    assert_eq!(f.read("thread-1.json")["method"], "thread/start");
    assert_eq!(f.read("thread-2.json")["method"], "thread/resume");
    assert!(f.dir.join("second-turn.json").is_file());
    let pending = f.app.ai.pending_tool.as_ref().unwrap();
    assert_eq!(pending.call.name, "rpshop_batch_edit");
    assert_eq!(
        pending.call.arguments["operations"][0]["widget_id"],
        target.id
    );
    assert_eq!(pending.scope, f.app.ai_project_key());
    assert!(!pending.is_cancelled());
    assert_eq!(f.app.ai.task_context, 2);
    assert_eq!(
        f.app.project, before_transaction,
        "no mutation before consent"
    );
    assert_eq!(f.app.undo.len(), undo_before);
    assert!(!f.dir.join("tool-result.json").exists());

    // Exercise the actual host gate, not execute_ai_tool or a synthetic ToolHost reply.
    let output = frame(&mut f.app, &ctx, vec![]);
    let approve = text_center(&output, "允许本次 Ingenaro 工具操作");
    click(&mut f.app, &ctx, approve);
    f.drive_until(&ctx, "queued CLI turn completes after tool result", |app| {
        app.ai.running.is_none() && app.ai.queue.len() == 0
    });
    assert!(f.app.ai.task_error.is_none());
    assert!(!f.app.ai.queue.paused, "{}", f.app.ai.state);
    assert!(f.app.ai.pending_tool.is_none());
    assert!(f.app.ai.agent_server.is_none());
    assert_eq!(f.read("tool-result.json")["success"], true);
    assert_eq!(
        f.app
            .ai
            .messages
            .iter()
            .map(|message| (message.role, message.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (design::ChatRole::User, FIRST_PROMPT),
            (design::ChatRole::Assistant, FIRST_REPLY),
            (
                design::ChatRole::User,
                &format!(
                    "{second_prompt}\n[参考图片：queued-reference.png]\n[参考文件：admin.css]"
                )
            ),
            (design::ChatRole::Assistant, SECOND_REPLY),
        ]
    );
    assert!(f.app.ai.messages[1].steps.is_empty());
    assert!(f.app.ai.messages[3].steps.iter().any(|step| matches!(
        &step.activity,
        design::ChatActivity::Tool { name, outcome: Some(true) } if name == "rpshop_batch_edit"
    )));
    assert_eq!(f.app.ai.images.len(), 1);
    assert_eq!(f.app.ai.images[0].id(), later_image_id);
    assert_eq!(
        f.app.ai.reference_files.files[0].byte_len(),
        "new draft CSS".len()
    );
    let mut expected_project = before_transaction.clone();
    expected_project.pages[0].widgets[0].text = EDITED_TEXT.into();
    assert_eq!(
        f.app.project, expected_project,
        "only the requested property changes"
    );
    assert_eq!(
        f.app.undo.len(),
        undo_before + 1,
        "one host document transaction"
    );
    assert!(f.app.redo.is_empty());
    f.app.undo();
    assert_eq!(
        f.app.project, before_transaction,
        "undo preserves the intervening user edits"
    );
    assert_eq!(f.app.undo.len(), undo_before);
    assert_eq!(f.app.redo.len(), 1);
    f.app.redo();
    assert_eq!(f.app.project, expected_project);
    assert_eq!(f.app.undo.len(), undo_before + 1);
    assert!(f.app.redo.is_empty());

    f.cleanup().unwrap();
    assert!(!f.dir.exists());
    assert!(!f.workspace.exists());
}
