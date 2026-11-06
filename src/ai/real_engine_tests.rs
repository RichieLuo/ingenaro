//! Opt-in paid/network smoke checks. Never run by the default test suite.
//! Only an ephemeral blank in-memory prototype is exposed; no save/export tools.
use super::*;
use crate::automation::{self, ToolHost};
use crate::model::Project;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

fn smoke(kind: EngineKind) {
    let mut config = EngineConfig::new(kind);
    config.timeout_seconds = 120;
    if kind == EngineKind::Claude {
        config.max_budget_usd = Some(0.10);
    }
    let diag = config.diagnostics().expect("diagnostics failed");
    assert!(
        diag.available,
        "CLI must be installed, compatible, and already logged in; {}",
        diag.action_hint
    );
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/ai-smoke")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&dir).unwrap();
    let scope = format!("smoke-{}", uuid::Uuid::new_v4());
    let cancelled = Arc::new(AtomicBool::new(false));
    let (host, tools) = ToolHost::new();
    let server = if kind == EngineKind::Claude {
        Some(automation::start(tools.scoped(scope.clone(), cancelled.clone())).unwrap())
    } else {
        None
    };
    let mcp = server.as_ref().map(|s| McpEndpoint {
        endpoint: s.endpoint().into(),
        token: s.token().into(),
    });
    let mut project = Project::blank();
    let running=AiRuntime::start(StartRequest {
        config,key:None,prompt:"This is a bounded integration smoke test. Do exactly these two tool calls, no other tools: first rpshop_read_pages with {}, then rpshop_batch_edit with {\"operations\":[{\"op\":\"create_widget\",\"kind\":\"Text\",\"properties\":{\"name\":\"AI smoke\",\"text\":\"AI smoke\",\"x\":20,\"y\":20,\"width\":160,\"height\":40}}]}. Then reply OK. Do not inspect files, save, export, use a shell, or call any other tools.".into(),
        context:"Fresh blank prototype. Current page is the first page; omitting page_id targets it. All operations are in memory only. User explicitly authorizes the two calls above for this test.".into(),images:vec![],working_dir:dir.clone(),scope:scope.clone(),resume:None,tools,mcp,cancelled:cancelled.clone(),
    }).unwrap();
    let deadline = Instant::now() + Duration::from_secs(130);
    let mut reads = 0;
    let mut edits = 0;
    let mut errors = Vec::new();
    loop {
        assert!(
            Instant::now() < deadline,
            "real engine smoke exceeded deadline"
        );
        while let Some(request) = host.try_recv() {
            let allowed = request.scope == scope
                && !request.is_cancelled()
                && match request.call.name.as_str() {
                    "rpshop_read_pages" => reads == 0 && edits == 0,
                    "rpshop_batch_edit" => reads == 1 && edits == 0,
                    _ => false,
                };
            let result = if allowed {
                if request.call.name == "rpshop_read_pages" {
                    reads += 1;
                } else {
                    edits += 1;
                }
                let mut candidate = project.clone();
                let result = automation::execute(&mut candidate, false, 0, &[], &request.call);
                if result.is_ok() && !request.is_cancelled() {
                    project = candidate;
                }
                result
            } else {
                Err("Smoke permits only one pages read followed by one widget edit".into())
            };
            let _ = request.reply.send(result);
        }
        match running.events.recv_timeout(Duration::from_millis(20)) {
            Ok(AiEvent::ApprovalRequired { id, tool, .. }) => {
                let allowed = matches!(
                    tool.as_str(),
                    "mcp__rpshop__rpshop_read_pages" | "mcp__rpshop__rpshop_batch_edit"
                );
                running.control.approve(id, allowed).unwrap();
            }
            Ok(AiEvent::Error { message }) => errors.push(message),
            Ok(AiEvent::Complete { cancelled }) => {
                assert!(!cancelled);
                break;
            }
            Ok(_) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => panic!("real engine event channel closed"),
        }
    }
    assert!(cancelled.load(Ordering::Acquire));
    drop(running);
    drop(server);
    let _ = std::fs::remove_dir_all(dir);
    assert!(errors.is_empty(), "real engine error: {errors:?}");
    assert_eq!(
        (reads, edits),
        (1, 1),
        "engine did not execute the bounded tool plan"
    );
    assert!(
        project.pages[0]
            .widgets
            .iter()
            .any(|w| w.text == "AI smoke")
    );
}
#[test]
#[ignore = "explicit opt-in: real logged-in Codex model call, two in-memory tools only"]
fn codex_logged_in_design_smoke() {
    smoke(EngineKind::Codex);
}
#[test]
#[ignore = "explicit opt-in: real logged-in Claude model call, max budget USD 0.10"]
fn claude_logged_in_design_smoke() {
    smoke(EngineKind::Claude);
}
