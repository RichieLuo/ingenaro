//! Native child events and scoped writes inside a single CLI protocol stream.
use super::*;
use crate::ui::tests::{click, fixture, frame, text_center};
use std::path::PathBuf;

const FOLLOW_UP: &str = "接着在同一个会话里说明刚才的结果";

struct CliFixture {
    app: App,
    dir: PathBuf,
    workspace: PathBuf,
}

impl CliFixture {
    fn new(kind: ai::EngineKind, prompt: &str) -> Option<Self> {
        let Ok(node) = std::process::Command::new("node").arg("--version").output() else {
            eprintln!("SKIP parallel design CLI regression: Node.js is unavailable");
            return None;
        };
        assert!(node.status.success());
        let document_id = format!("rpshop-parallel-test-{}", uuid::Uuid::new_v4());
        let dir = std::env::temp_dir().join(&document_id);
        std::fs::create_dir(&dir).unwrap();
        let executable = dir.join("mock.js");
        std::fs::write(&executable, include_str!("mock_cli.js")).unwrap();
        let mut app = fixture(Project::blank());
        app.ai.document_id = document_id.clone();
        let mut engine = ai::EngineConfig::new(kind);
        engine.executable = Some(executable);
        engine.model = "mock-design-model".into();
        app.ai.selected_engine = engine.id.clone();
        app.ai.settings.engines.push(engine);
        app.ai.last_project_key = app.ai_project_key();
        app.ai.visible = true;
        app.ai_grant_full_permission();
        app.ai.prompt = prompt.into();
        app.ai_start();
        assert!(app.ai.running.is_some(), "{}", app.ai.state);
        Some(Self {
            app,
            dir,
            workspace: std::env::temp_dir()
                .join("rpshop-agent-workspaces")
                .join(document_id),
        })
    }

    fn drive(&mut self, ctx: &egui::Context, phase: &str, done: impl Fn(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let failure = std::fs::read_to_string(self.dir.join("failure.txt")).unwrap_or_default();
            assert!(failure.is_empty(), "{phase}: {failure}");
            if done(&self.app) {
                return;
            }
            assert!(
                self.app.ai.task_error.is_none(),
                "{phase}: {}",
                self.app.ai.state
            );
            assert!(
                Instant::now() < deadline,
                "{phase}: {} · {:?}",
                self.app.ai.state,
                self.app.ai.response.steps
            );
            frame(&mut self.app, ctx, vec![]);
            std::thread::park_timeout(Duration::from_millis(2));
        }
    }

    fn first_edits(&mut self, ctx: &egui::Context, page_count: usize) -> Vec<u64> {
        self.drive(ctx, "early write waits for the other groups", |app| {
            !app.ai.page_agents.waiting_edits.is_empty()
        });
        assert_eq!(self.app.project.pages.len(), page_count + 1);
        assert!(
            self.app
                .project
                .pages
                .iter()
                .all(|page| page.widgets.is_empty())
        );
        assert_eq!(self.app.ai_progress_text(0.0), "正在启动子代理 · 1/3");
        std::fs::write(self.dir.join("launch-rest"), "").unwrap();
        let dir = self.dir.clone();
        self.drive(
            ctx,
            "all groups apply and acknowledge their first edits",
            |app| {
                app.project.pages.len() == page_count + 1
                    && app.project.pages[1..]
                        .iter()
                        .all(|page| !page.widgets.is_empty())
                    && (0..3).all(|slot| dir.join(format!("edited-{slot}")).is_file())
            },
        );
        let pages = &self.app.project.pages;
        let ids: Vec<_> = pages[1..].iter().map(|page| page.id).collect();
        assert!(pages[0].widgets.is_empty());
        for (slot, page) in pages[1..].iter().enumerate() {
            assert_eq!(page.widgets.len(), 1);
            assert_eq!(page.widgets[0].text, format!("页面 {slot}"));
        }
        assert_eq!(self.app.ai.page_agents.progress(), (3, 3));
        assert_eq!(self.app.ai_activity_page_ids(), ids);
        assert!(self.app.ai_page_overview_visible());
        assert_eq!(self.app.undo.len(), page_count + 1);
        ids
    }

    fn release(&self, slot: usize) {
        std::fs::write(self.dir.join(format!("release-{slot}")), "").unwrap();
    }
}

impl Drop for CliFixture {
    fn drop(&mut self) {
        self.app.ai.cancel();
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        let _ = std::fs::remove_dir(&self.dir);
        let _ = std::fs::remove_dir(&self.workspace);
    }
}

fn check_parallel_design(kind: ai::EngineKind, prompt: &str, page_count: usize) {
    let Some(mut test) = CliFixture::new(kind, prompt) else {
        return;
    };
    let ctx = egui::Context::default();
    let ids = test.first_edits(&ctx, page_count);
    let output = frame(&mut test.app, &ctx, vec![]);
    let indicator = text_center(&output, "子代理 3/3");
    click(&mut test.app, &ctx, indicator);
    let output = frame(&mut test.app, &ctx, vec![]);
    text_center(&output, "页面子代理 · 3 个运行中");
    text_center(&output, "设计任务 0");
    text_center(
        &output,
        &format!("设计中 · 已应用 {} 批", if page_count == 9 { 3 } else { 2 }),
    );
    click(&mut test.app, &ctx, indicator);

    test.release(1);
    test.drive(
        &ctx,
        "one child completes without ending the main turn",
        |app| app.ai.page_agents.progress() == (2, 3),
    );
    assert!(test.app.ai.running.is_some());
    assert_eq!(test.app.ai_activity_page_ids(), ids);
    let output = frame(&mut test.app, &ctx, vec![]);
    text_center(&output, "子代理 2/3");
    text_center(&output, "已完成");

    test.release(0);
    test.release(2);
    test.drive(&ctx, "main turn completes", |app| app.ai.running.is_none());
    assert!(test.app.ai.task_error.is_none());
    assert_eq!(test.app.project.pages.len(), page_count + 1);
    assert_eq!(test.app.ai.page_agents.progress(), (0, 3));
    assert!(
        test.app
            .ai
            .page_agents
            .rows
            .iter()
            .all(|row| row.state == SubagentState::Completed)
    );
    assert_eq!(
        test.app.ai.binding.as_ref().unwrap().session_id,
        "one-main-session"
    );
    assert_eq!(test.app.ai.messages.len(), 2);
    assert_eq!(test.app.ai.messages[1].text, "本轮完成");
    assert_eq!(test.app.ai_activity_page_ids(), ids);
    assert!(test.app.ai_page_overview_visible());
    let output = frame(&mut test.app, &ctx, vec![]);
    text_center(&output, "子代理 0/3");
    text_center(&output, "多页设计已完成");
    test.app.project.validate().unwrap();

    test.app.ai.prompt = FOLLOW_UP.into();
    test.app.ai_start();
    test.drive(&ctx, "follow-up resumes the parent", |app| {
        app.ai.running.is_none()
    });
    assert_eq!(
        test.app.ai.binding.as_ref().unwrap().session_id,
        "one-main-session"
    );
    assert_eq!(test.app.ai.messages.last().unwrap().text, "仍在原会话");
}

#[test]
fn claude_delegates_unsized_request_and_resumes_one_parent() {
    check_parallel_design(
        ai::EngineKind::Claude,
        "设计登录、注册、首页、订单和设置页面",
        5,
    );
}

#[test]
fn codex_launches_all_three_groups_of_nine_pages_before_writing() {
    check_parallel_design(ai::EngineKind::Codex, "设计九个独立页面", 9);
}

#[test]
fn finishing_the_main_turn_with_unlaunched_groups_reports_failure() {
    let Some(mut test) = CliFixture::new(ai::EngineKind::Codex, "验证分工未全部启动")
    else {
        return;
    };
    let ctx = egui::Context::default();
    test.drive(&ctx, "main turn ends with missing groups", |app| {
        app.ai.running.is_none()
    });
    assert_eq!(
        test.app.ai.task_error.as_deref(),
        Some("本轮还有 2 组页面子代理未启动")
    );
    assert!(test.app.ai.queue.paused);
}

#[test]
fn stopping_native_agents_revokes_the_parent_tool_capability() {
    let Some(mut test) = CliFixture::new(ai::EngineKind::Claude, "设计一个相互连贯的多页系统")
    else {
        return;
    };
    let ctx = egui::Context::default();
    test.first_edits(&ctx, 5);
    let before = test.app.project.clone();
    let control = test.app.ai.running.as_ref().unwrap().control.clone();
    test.app.ai_stop_design();
    assert!(control.cancelled().load(Ordering::Acquire));
    assert!(test.app.ai_activity_page_ids().is_empty());
    assert_eq!(test.app.ai.page_agents.progress(), (0, 3));
    assert!(
        test.app
            .ai
            .page_agents
            .rows
            .iter()
            .all(|row| row.state == SubagentState::Stopped)
    );
    test.drive(&ctx, "parent CLI shuts down", |_| control.is_finished());
    assert_eq!(test.app.project, before);
}
