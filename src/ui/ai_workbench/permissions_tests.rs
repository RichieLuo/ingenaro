use super::*;
use crate::ui::tests::fixture;
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

struct IdleAgent {
    directory: PathBuf,
    listener: std::thread::JoinHandle<()>,
}
impl IdleAgent {
    fn start(app: &mut App) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let directory =
            std::env::temp_dir().join(format!("rpshop-full-permission-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let mut config = ai::EngineConfig::new(ai::EngineKind::OpenAiCompatible);
        config.model = "mock".into();
        config.api_base = Some(format!("http://{address}/v1"));
        config.timeout_seconds = 10;
        app.ai.selected_engine = config.id.clone();
        app.ai.settings.engines.push(config.clone());
        let cancelled = Arc::new(AtomicBool::new(false));
        app.ai.task_cancelled = cancelled.clone();
        let flag = cancelled.clone();
        listener.set_nonblocking(true).unwrap();
        let listener = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(3);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if flag.load(Ordering::Acquire) || Instant::now() >= deadline {
                            return;
                        }
                        std::thread::park_timeout(Duration::from_millis(5));
                    }
                    Err(_) => return,
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some((key, value)) = line.split_once(':')
                    && key.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse::<usize>().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            while !flag.load(Ordering::Acquire) {
                std::thread::park_timeout(Duration::from_millis(5));
            }
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 14\r\n\r\ndata: [DONE]\n\n"
            );
        });
        let running = ai::AiRuntime::start(ai::StartRequest {
            config,
            key: None,
            prompt: "mock".into(),
            images: Vec::new(),
            context: String::new(),
            working_dir: directory.clone(),
            scope: app.ai_project_key(),
            resume: None,
            tools: app.ai.client.clone(),
            mcp: None,
            cancelled,
        })
        .unwrap();
        app.ai.running = Some(running);
        // Cleanup is owned explicitly by the test, without detached server threads.
        IdleAgent {
            directory,
            listener,
        }
    }
}
fn finish(app: &mut App, fixture: IdleAgent) {
    app.ai.cancel();
    fixture.listener.join().unwrap();
    let _ = std::fs::remove_dir_all(fixture.directory);
}
fn edit() -> automation::ToolCall {
    automation::ToolCall {
        name: "rpshop_batch_edit".into(),
        arguments: json!({"operations":[{"op":"create_widget","kind":"Text","properties":{"text":"无需重复授权"}}]}),
    }
}
fn next_request(
    client: automation::ToolClient,
) -> std::thread::JoinHandle<Result<serde_json::Value, String>> {
    std::thread::spawn(move || client.call(edit()))
}

#[test]
fn full_grant_automatically_applies_only_current_builtin_task_and_supports_undo() {
    let mut app = fixture(Project::blank());
    let fixture = IdleAgent::start(&mut app);
    app.ai_grant_full_permission();
    let before = app.project.clone();
    let context = egui::Context::default();
    let client = app
        .ai
        .client
        .scoped(app.ai_project_key(), app.ai.task_cancelled.clone());
    let worker = next_request(client);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !worker.is_finished() && Instant::now() < deadline {
        app.poll_ai_tools(&context);
        std::thread::yield_now();
    }
    assert!(worker.is_finished());
    assert!(worker.join().unwrap().is_ok());
    assert!(app.ai.pending_tool.is_none());
    assert_eq!(app.undo.len(), 1);
    app.undo();
    assert_eq!(app.project, before);
    finish(&mut app, fixture);
}
#[test]
fn full_grant_never_applies_to_external_mcp() {
    let mut app = fixture(Project::blank());
    let fixture = IdleAgent::start(&mut app);
    app.ai_grant_full_permission();
    let client = app
        .ai
        .client
        .scoped(app.ai_project_key(), Arc::new(AtomicBool::new(false)));
    let worker = next_request(client);
    let context = egui::Context::default();
    let deadline = Instant::now() + Duration::from_secs(2);
    while app.ai.pending_tool.is_none() && Instant::now() < deadline {
        app.poll_ai_tools(&context);
        std::thread::yield_now();
    }
    assert!(app.ai.pending_tool.is_some());
    assert!(app.project.pages[0].widgets.is_empty());
    app.ai.cancel();
    assert!(worker.join().unwrap().is_err());
    finish(&mut app, fixture);
}
#[test]
fn running_task_has_no_status_stop_button_and_footer_stop_still_cancels() {
    let mut app = fixture(Project::blank());
    let agent = IdleAgent::start(&mut app);
    app.ai.visible = true;
    app.ai.last_project_key = app.ai_project_key();
    let context = egui::Context::default();
    let output = crate::ui::tests::frame(&mut app, &context, vec![]);
    fn has_stop_text(shape: &egui::epaint::Shape) -> bool {
        match shape {
            egui::epaint::Shape::Text(text) => text.galley.text() == "停止",
            egui::epaint::Shape::Vec(shapes) => shapes.iter().any(has_stop_text),
            _ => false,
        }
    }
    assert!(
        !output
            .shapes
            .iter()
            .any(|shape| has_stop_text(&shape.shape))
    );
    assert!(app.ai.running.is_some());
    let stop = composer::toolbar::control_bounds(&context, "send").center();
    crate::ui::tests::click(&mut app, &context, stop);
    assert!(app.ai.running.is_none());
    assert!(app.ai.task_cancelled.load(Ordering::Acquire));
    finish(&mut app, agent);
}

#[test]
fn selecting_full_is_confirmation_only_and_grants_do_not_cross_engine_or_generation() {
    let mut app = fixture(Project::blank());
    app.ai_change_permission(permissions::PermissionMode::Full);
    assert!(app.ai.authorization.confirm_full);
    assert!(!app.ai_has_full_permission());
    app.ai_grant_full_permission();
    assert!(app.ai_has_full_permission());
    app.ai.selected_engine = "different".into();
    assert!(!app.ai_has_full_permission());
    app.ai_grant_full_permission();
    app.ai.generation = "another-chat".into();
    assert!(!app.ai_has_full_permission());
    app.ai_grant_full_permission();
    app.ai_document_replaced();
    assert!(!app.ai_has_full_permission());
}
