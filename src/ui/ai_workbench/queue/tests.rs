use super::*;
use crate::ui::tests::{fixture, frame};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

fn configured() -> App {
    let mut app = fixture(Project::default());
    app.ai.visible = true;
    let config = ai::EngineConfig::new(ai::EngineKind::Claude);
    app.ai.selected_engine = config.id.clone();
    app.ai.settings.engines.push(config);
    app.ai.last_project_key = app.ai_project_key();
    app
}
fn capture(app: &mut App, text: &str) -> Submission {
    app.ai.prompt = text.into();
    app.ai_capture_submission().unwrap()
}
#[test]
fn queue_is_fifo_and_captures_selection_and_images_before_other_edits() {
    let mut app = configured();
    let id = app.project.pages[0].widgets[0].id;
    app.selected.insert(id);
    app.ai
        .images
        .push(ai::ImageAttachment::from_rgba("reference", 1, 1, &[50, 100, 150, 255]).unwrap());
    let first = capture(&mut app, "改这个元件");
    let original = first.context.clone();
    let image = first.images[0].id().to_owned();
    app.project.pages[0].widgets[0].text = "后来修改".into();
    app.selected.clear();
    app.ai.images.clear();
    let second = capture(&mut app, "下一步");
    let mut queue = MessageQueue::default();
    let first_id = queue.push(first).unwrap();
    let second_id = queue.push(second).unwrap();
    assert_ne!(first_id, second_id);
    let first = queue.pop().unwrap();
    assert_eq!(first.context, original);
    assert_eq!(first.images[0].id(), image);
    assert_eq!(first.context_mode, 2);
    assert_eq!(queue.pop().unwrap().prompt, "下一步");
    assert_eq!(queue.len(), 0);
}
#[test]
fn queue_pause_remove_limits_and_project_switch_do_not_send_anything() {
    let mut app = configured();
    for index in 0..16 {
        let item = capture(&mut app, &format!("任务{index}"));
        app.ai.queue.push(item).unwrap();
    }
    let extra = capture(&mut app, "超限");
    assert!(app.ai.queue.push(extra).is_err());
    app.ai.queue.paused = true;
    assert!(app.ai.queue.pop().is_none());
    let id = app.ai.queue.entries().next().unwrap().id;
    app.ai.queue.remove(id);
    assert_eq!(app.ai.queue.len(), 15);
    app.ai_document_replaced();
    assert_eq!(app.ai.queue.len(), 0);
    assert!(app.ai.running.is_none());
    assert!(app.ai.images.is_empty());
}
fn read_request(stream: &TcpStream) -> Value {
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
            length = value.trim().parse().unwrap();
        }
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
fn respond(stream: &mut TcpStream, text: &str) {
    let chunk = json!({"choices":[{"delta":{"content":text},"finish_reason":"stop"}]});
    let body = format!("data: {chunk}\n\ndata: [DONE]\n\n");
    write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    stream.flush().unwrap();
}
#[test]
fn running_followup_waits_then_executes_with_its_own_image_and_frozen_reference() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let (observed_tx, observed) = mpsc::channel();
    let (release_tx, release) = mpsc::channel();
    let server = std::thread::spawn(move || {
        for index in 0..2 {
            let deadline = Instant::now() + Duration::from_secs(10);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline);
                        std::thread::park_timeout(Duration::from_millis(2));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let request = read_request(&stream);
            observed_tx.send(request).unwrap();
            if index == 0 {
                release.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            respond(
                &mut stream,
                if index == 0 {
                    "第一轮完成"
                } else {
                    "第二轮完成"
                },
            );
        }
    });
    let mut app = fixture(Project::default());
    let mut engine = ai::EngineConfig::new(ai::EngineKind::OpenAiCompatible);
    engine.model = "mock".into();
    engine.api_base = Some(format!("http://{address}/v1"));
    app.ai.selected_engine = engine.id.clone();
    app.ai.settings.engines.push(engine);
    app.ai.last_project_key = app.ai_project_key();
    app.ai.prompt = "先处理第一条".into();
    app.ai_submit();
    assert!(app.ai.running.is_some());
    observed.recv_timeout(Duration::from_secs(3)).unwrap();
    let id = app.project.pages[0].widgets[0].id;
    app.selected.insert(id);
    let selected_name = app.project.pages[0].widgets[0].name.clone();
    app.ai
        .images
        .push(ai::ImageAttachment::from_rgba("queued-image", 1, 1, &[255, 0, 0, 255]).unwrap());
    app.ai.prompt = "第二条看图修改引用元件".into();
    app.ai_submit();
    assert_eq!(app.ai.queue.len(), 1);
    assert!(app.ai.images.is_empty());
    assert!(observed.try_recv().is_err());
    app.selected.clear();
    app.project.pages[0].widgets[0].name = "别的名字".into();
    release_tx.send(()).unwrap();
    let context = egui::Context::default();
    let deadline = Instant::now() + Duration::from_secs(8);
    while (app.ai.running.is_some() || app.ai.queue.len() > 0) && Instant::now() < deadline {
        frame(&mut app, &context, vec![]);
        std::thread::yield_now();
    }
    assert!(app.ai.running.is_none(), "{}", app.ai.state);
    assert_eq!(app.ai.queue.len(), 0);
    let request = observed.recv_timeout(Duration::from_secs(2)).unwrap();
    let content = request["messages"][0]["content"].as_array().unwrap();
    assert!(content.iter().any(|part| {
        part["type"] == "image_url"
            && part["image_url"]["url"]
                .as_str()
                .is_some_and(|url| url.starts_with("data:image/png;base64,"))
    }));
    assert!(
        content
            .iter()
            .any(|part| part["text"].as_str().is_some_and(|text| text
                .contains("第二条看图修改引用元件")
                && text.contains(&selected_name)))
    );
    assert_eq!(
        app.ai
            .messages
            .iter()
            .filter(|(role, _)| role == "你")
            .count(),
        2
    );
    server.join().unwrap();
}
#[test]
fn failed_start_retry_does_not_duplicate_chat_or_persist_a_message() {
    let mut app = configured();
    let directory =
        std::env::temp_dir().join(format!("rpshop-queue-start-{}", uuid::Uuid::new_v4()));
    app.ai.store = Some(design::Store::open(&directory).unwrap());
    // Runtime rejects a resume capability bound to another engine, after preparation.
    app.ai.active_scope = app.ai.scope(&app.ai_project_key());
    app.ai.binding = Some(ai::SessionBinding {
        session_id: "invalid-resume".into(),
        engine_id: "another-engine".into(),
        scope: app.ai_project_key(),
        tool_revision: ai::TOOL_PROTOCOL_REVISION,
    });
    let item = capture(&mut app, "启动失败后保留这条需求");
    app.ai.queue.push(item).unwrap();
    for _ in 0..2 {
        app.ai.queue.paused = false;
        app.ai_poll_queue();
        assert!(
            app.ai.state.contains("another project or engine"),
            "{}",
            app.ai.state
        );
        assert!(app.ai.queue.paused);
        assert_eq!(app.ai.queue.len(), 1);
        assert!(app.ai.running.is_none());
        assert!(app.ai.messages.is_empty());
        assert!(app.ai.agent_server.is_none());
        assert!(app.ai.task_started.is_none());
        assert!(app.ai.task_cancelled.load(Ordering::Acquire));
        let session = app
            .ai
            .store
            .as_ref()
            .unwrap()
            .session(
                app.ai.active_scope.as_ref().unwrap(),
                app.ai.session_id.unwrap(),
            )
            .unwrap();
        assert!(session.messages.is_empty());
    }
    drop(app);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn history_and_new_conversation_clear_queued_drafts_and_old_generated_state() {
    let mut app = configured();
    let directory =
        std::env::temp_dir().join(format!("rpshop-queue-history-{}", uuid::Uuid::new_v4()));
    let mut store = design::Store::open(&directory).unwrap();
    let scope = app.ai.scope(&app.ai_project_key()).unwrap();
    let id = store
        .create_session(&scope, design::ArtifactKind::Native, "历史")
        .unwrap();
    store
        .append_message(&scope, id, design::ChatRole::User, "旧需求")
        .unwrap();
    app.ai.store = Some(store);
    for history in [true, false] {
        let generation = app.ai.generation.clone();
        let item = capture(&mut app, "待执行");
        app.ai.queue.push(item).unwrap();
        app.ai
            .images
            .push(ai::ImageAttachment::from_rgba("草稿图", 1, 1, &[0; 4]).unwrap());
        app.ai.reasoning = "旧摘要".into();
        app.ai.streamed = "旧输出".into();
        app.ai.html = "旧产物".into();
        app.ai_grant_full_permission();
        if history {
            app.ai_load_conversation(scope.clone(), id).unwrap();
        } else {
            app.ai_new_conversation();
        }
        assert_ne!(app.ai.generation, generation);
        assert_eq!(app.ai.queue.len(), 0);
        assert!(app.ai.images.is_empty());
        assert!(app.ai.reasoning.is_empty());
        assert!(app.ai.streamed.is_empty());
        assert!(app.ai.html.is_empty());
        assert!(!app.ai_has_full_permission());
        assert_eq!(app.ai.messages.len(), usize::from(history));
    }
    drop(app);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn queued_read_only_submission_stays_read_only_after_a_later_full_grant() {
    let mut app = configured();
    app.ai_change_permission(permissions::PermissionMode::ReadOnly);
    let readonly = capture(&mut app, "先讨论");
    app.ai_grant_full_permission();
    assert!(readonly.read_only);
    let edit = capture(&mut app, "再编辑");
    app.ai_change_permission(permissions::PermissionMode::ReadOnly);
    assert!(!edit.read_only);
    let call = automation::ToolCall {
        name: "rpshop_batch_edit".into(),
        arguments: json!({"operations":[{"op":"update_project","name":"不允许"}]}),
    };
    assert!(
        app.execute_ai_tool(&egui::Context::default(), &call)
            .is_err()
    );
}

#[test]
fn cancel_pauses_followups_instead_of_automatically_launching_them() {
    let mut app = configured();
    let item = capture(&mut app, "稍后执行");
    app.ai.queue.push(item).unwrap();
    app.ai_stop_design();
    assert!(app.ai.queue.paused);
    app.ai_poll_queue();
    assert!(app.ai.running.is_none());
    assert_eq!(app.ai.queue.len(), 1);
}

#[test]
fn editing_pending_message_preserves_images_and_does_not_overwrite_a_draft() {
    let mut app = configured();
    let image = ai::ImageAttachment::from_rgba("参考", 1, 1, &[20, 40, 60, 255]).unwrap();
    app.ai.images.push(image.clone());
    let entry = capture(&mut app, "稍后调整卡片");
    let id = app.ai.queue.push(entry).unwrap();
    app.ai.images.clear();
    app.ai.prompt = "尚未发送的草稿".into();
    app.ai_edit_queued(id);
    assert_eq!(app.ai.prompt, "尚未发送的草稿");
    assert_eq!(app.ai.queue.len(), 1);
    app.ai.prompt.clear();
    app.ai_edit_queued(id);
    assert_eq!(app.ai.prompt, "稍后调整卡片");
    assert_eq!(app.ai.images[0].id(), image.id());
    assert_eq!(app.ai.queue.len(), 0);
}

#[test]
fn editing_pending_message_rejects_a_stale_generation() {
    let mut app = configured();
    let entry = capture(&mut app, "旧会话消息");
    let id = app.ai.queue.push(entry).unwrap();
    app.ai.prompt.clear();
    app.ai.generation = "new-generation".into();
    app.ai_edit_queued(id);
    assert!(app.ai.prompt.is_empty());
    assert_eq!(app.ai.queue.len(), 1);
}

#[test]
fn compact_pending_message_remove_control_retracts_only_that_message() {
    use crate::ui::tests::click;
    let mut app = configured();
    let entry = capture(&mut app, "撤回这条");
    let id = app.ai.queue.push(entry).unwrap();
    let entry = capture(&mut app, "保留这条");
    app.ai.queue.push(entry).unwrap();
    app.ai.queue.paused = true;
    let ctx = egui::Context::default();
    crate::ui::setup_style(&ctx);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    let rect = ctx
        .data(|data| data.get_temp::<Rect>(egui::Id::new(("ai-queue-remove", id))))
        .unwrap();
    click(&mut app, &ctx, rect.center());
    assert_eq!(app.ai.queue.len(), 1);
    assert_eq!(app.ai.queue.entries().next().unwrap().prompt, "保留这条");
}
