use super::*;
mod presentation;
mod stability;
use crate::ui::tests::{click, fixture, frame, text_center};

fn app() -> App {
    let mut app = fixture(Project::blank());
    app.ai.visible = true;
    app.ai.last_project_key = app.ai_project_key();
    let mut engine = ai::EngineConfig::new(ai::EngineKind::Claude);
    engine.model = "opus".into();
    let id = engine.id.clone();
    app.ai.selected_engine = id.clone();
    app.ai.settings.engines.push(engine);
    app.ai_grant_full_permission();
    let catalog = ai::ModelCatalog {
        models: vec![ai::ModelInfo {
            id: "opus".into(),
            name: "Opus 5.5 with an intentionally long configured display name".into(),
            description: String::new(),
            is_default: true,
            reasoning_efforts: vec![
                "low".into(),
                "medium".into(),
                "high".into(),
                "xhigh".into(),
                "max".into(),
            ],
        }],
        source: "fixture".into(),
        warning: None,
    };
    app.ai
        .catalogs
        .insert(id, models::CatalogState::ready(catalog));
    app
}
fn toolbar_frame(app: &mut App, ctx: &egui::Context, width: f32) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 500.0))),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.allocate_ui_with_layout(
                    vec2(width, CONTROL_HEIGHT),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        app.ai_composer_toolbar(ui);
                    },
                );
            });
        },
    )
}
#[test]
fn every_toolbar_width_is_one_row_and_all_controls_keep_nonoverlapping_targets() {
    let mut app = app();
    let context = egui::Context::default();
    for width in [210.0, 250.0, 280.0, 330.0, 370.0, 420.0, 520.0, 640.0] {
        let _ = toolbar_frame(&mut app, &context, width);
        let bounds =
            ["engine", "permission", "model", "send"].map(|key| control_bounds(&context, key));
        for rect in bounds {
            assert_eq!(rect.height(), CONTROL_HEIGHT);
            assert!(
                (rect.top() - bounds[0].top()).abs() < 0.01,
                "width={width}, rect={rect:?}"
            );
            assert!(rect.width() >= 30.0);
        }
        for pair in bounds.windows(2) {
            assert!(
                pair[1].left() >= pair[0].right() + 3.0,
                "width={width}, {pair:?}"
            );
        }
        assert!(
            bounds[3].right() <= bounds[0].left() + width + 0.1,
            "width={width}, row={bounds:?}"
        );
    }
}
#[test]
fn narrow_layout_is_icon_only_and_wide_layout_has_labels() {
    let narrow = ToolbarLayout::new(260.0, [40.0, 48.0, 340.0]);
    assert_eq!(narrow.labels, [false; 3]);
    assert!(narrow.used_width() <= 260.0);
    let wide = ToolbarLayout::new(800.0, [40.0, 48.0, 100.0]);
    assert_eq!(wide.labels, [true; 3]);
    assert!(wide.used_width() <= 800.0);
}
#[test]
fn model_control_opens_the_effort_slider() {
    let mut app = app();
    let context = egui::Context::default();
    // The default sidebar also uses these exact stable control IDs.
    let _ = frame(&mut app, &context, vec![]);
    let model = control_bounds(&context, "model").center();
    click(&mut app, &context, model);
    let _ = frame(&mut app, &context, vec![]);
    let high = effort_bounds(&context, Some(ai::ReasoningEffort::High)).center();
    click(&mut app, &context, high);
    assert_eq!(
        app.ai.effort_override.resolve(None),
        Some(ai::ReasoningEffort::High)
    );
}
fn open_modes(app: &mut App, context: &egui::Context, entry: &'static str) -> egui::FullOutput {
    let _ = frame(app, context, vec![]);
    click(app, context, control_bounds(context, entry).center());
    frame(app, context, vec![])
}

#[test]
fn permission_and_model_menus_keep_their_own_controls() {
    let mut app = app();
    let context = egui::Context::default();
    let output = open_modes(&mut app, &context, "permission");
    for text in [
        "权限模式",
        "每次修改设计前，先征求你的确认",
        "探索设计与讨论方案，不修改画布",
    ] {
        text_center(&output, text);
    }
    assert!(
        context
            .data(|data| data.get_temp::<Rect>(egui::Id::new(("ai-effort-step-test", "auto"))))
            .is_none()
    );
    egui::Popup::close_all(&context);
    let output = open_modes(&mut app, &context, "model");
    text_center(&output, "选择模型");
    text_center(&output, "思考（自动）");
    assert!(
        context
            .data(|data| data.get_temp::<Rect>(egui::Id::new(("ai-toolbar-test", "effort"))))
            .is_none()
    );
}

#[test]
fn selecting_full_mode_still_requires_explicit_confirmation() {
    let mut app = app();
    app.ai_change_permission(permissions::PermissionMode::Ask);
    let context = egui::Context::default();
    let output = open_modes(&mut app, &context, "permission");
    click(&mut app, &context, text_center(&output, "完全授权"));
    assert!(app.ai.authorization.confirm_full);
    assert_eq!(
        app.ai.authorization.mode(),
        permissions::PermissionMode::Ask
    );
    assert!(!app.ai_has_full_permission());
    assert!(app.undo.is_empty());
}

#[test]
fn selecting_read_only_revokes_the_existing_grant() {
    let mut app = app();
    let context = egui::Context::default();
    let output = open_modes(&mut app, &context, "permission");
    click(&mut app, &context, text_center(&output, "只读讨论"));
    assert_eq!(
        app.ai.authorization.mode(),
        permissions::PermissionMode::ReadOnly
    );
    assert!(app.ai.discuss_only);
    assert!(!app.ai_has_full_permission());
    assert!(!egui::Popup::is_any_open(&context));
}

#[test]
fn effort_selection_keeps_popup_open_and_auto_overrides_engine_default() {
    let mut app = app();
    app.ai.settings.engines[0].reasoning_effort = Some(ai::ReasoningEffort::High);
    let context = egui::Context::default();
    open_modes(&mut app, &context, "model");
    let high = effort_bounds(&context, Some(ai::ReasoningEffort::High)).center();
    let automatic = effort_bounds(&context, None).center();
    assert!(automatic.x < high.x);
    click(&mut app, &context, automatic);
    assert_eq!(app.ai.effort_override, ai::ReasoningPreference::Auto);
    assert_eq!(
        app.ai.settings.engines[0].reasoning_effort,
        Some(ai::ReasoningEffort::High)
    );
    assert!(egui::Popup::is_any_open(&context));
    let output = frame(&mut app, &context, vec![]);
    text_center(&output, "思考（自动）");
    app.ai.prompt = "test request".into();
    assert!(
        app.ai_capture_submission()
            .unwrap()
            .engine
            .reasoning_effort
            .is_none()
    );
    assert!(app.project.pages[0].widgets.is_empty());
    assert!(app.undo.is_empty());
}

#[test]
fn effort_slider_supports_keyboard_and_escape_closes_the_popup() {
    let mut app = app();
    let context = egui::Context::default();
    open_modes(&mut app, &context, "model");
    let low = effort_bounds(&context, Some(ai::ReasoningEffort::Low)).center();
    click(&mut app, &context, low);
    assert_eq!(
        app.ai.effort_override.resolve(None),
        Some(ai::ReasoningEffort::Low)
    );
    let press = |key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&mut app, &context, vec![press(egui::Key::ArrowRight)]);
    assert_eq!(
        app.ai.effort_override.resolve(None),
        Some(ai::ReasoningEffort::Medium)
    );
    frame(&mut app, &context, vec![press(egui::Key::End)]);
    assert_eq!(
        app.ai.effort_override.resolve(None),
        Some(ai::ReasoningEffort::Max)
    );
    frame(&mut app, &context, vec![press(egui::Key::Home)]);
    assert_eq!(app.ai.effort_override, ai::ReasoningPreference::Auto);
    frame(&mut app, &context, vec![press(egui::Key::Escape)]);
    assert!(!egui::Popup::is_any_open(&context));
}

#[test]
fn effort_slider_drag_uses_only_the_supported_discrete_steps() {
    let mut app = app();
    let context = egui::Context::default();
    open_modes(&mut app, &context, "model");
    let low = effort_bounds(&context, Some(ai::ReasoningEffort::Low)).center();
    let max = effort_bounds(&context, Some(ai::ReasoningEffort::Max)).center();
    let pointer = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        &mut app,
        &context,
        vec![egui::Event::PointerMoved(low), pointer(low, true)],
    );
    frame(&mut app, &context, vec![egui::Event::PointerMoved(max)]);
    assert_eq!(
        app.ai.effort_override.resolve(None),
        Some(ai::ReasoningEffort::Max)
    );
    frame(&mut app, &context, vec![pointer(max, false)]);
    assert!(egui::Popup::is_any_open(&context));
}

#[test]
fn unknown_model_efforts_leave_permissions_available_but_slider_disabled() {
    let mut app = app();
    let id = app.ai.selected_engine.clone();
    app.ai.catalogs.get_mut(&id).unwrap().catalog.models[0].reasoning_efforts =
        vec!["unknown".into()];
    let context = egui::Context::default();
    let output = open_modes(&mut app, &context, "model");
    text_center(&output, "选择模型");
    text_center(&output, "思考（自动）");
    click(&mut app, &context, effort_bounds(&context, None).center());
    assert_eq!(app.ai.effort_override, ai::ReasoningPreference::Engine);
    assert!(egui::Popup::is_any_open(&context));
    assert!(app.ai_has_full_permission());
}

#[test]
fn composer_choices_follow_new_chats_and_reload_with_their_own_history() {
    use ai::{PermissionMode, ReasoningEffort};

    let mut app = app();
    let engine_id = app.ai.selected_engine.clone();
    app.ai.settings.engines[0].reasoning_effort = Some(ReasoningEffort::High);
    app.ai
        .catalogs
        .get_mut(&engine_id)
        .unwrap()
        .catalog
        .models
        .push(ai::ModelInfo {
            id: "quick".into(),
            name: "Quick".into(),
            description: String::new(),
            is_default: false,
            reasoning_efforts: vec!["high".into()],
        });
    let directory = std::env::temp_dir().join(format!("rpshop-composer-{}", uuid::Uuid::new_v4()));
    app.ai.store = Some(design::Store::open(&directory).unwrap());
    let scope = app.ai.scope(&app.ai_project_key()).unwrap();
    app.ai_record_version(
        "first",
        design::ArtifactContent::Native(Project::blank()),
        false,
    );
    let first = app.ai.session_id.unwrap();

    let context = egui::Context::default();
    let output = open_modes(&mut app, &context, "model");
    click(
        &mut app,
        &context,
        text_center(&output, "沿用 CLI 默认模型"),
    );
    frame(&mut app, &context, vec![]);
    let low = effort_bounds(&context, Some(ReasoningEffort::Low)).center();
    click(&mut app, &context, low);
    egui::Popup::close_all(&context);
    let output = open_modes(&mut app, &context, "permission");
    click(&mut app, &context, text_center(&output, "只读讨论"));

    app.ai_new_conversation();
    let engine = app.ai_submission_engine().unwrap();
    assert!(engine.model.is_empty());
    assert_eq!(engine.reasoning_effort, Some(ReasoningEffort::Low));
    assert!(app.ai.discuss_only);

    let legacy = app
        .ai
        .store
        .as_mut()
        .unwrap()
        .create_session(&scope, design::ArtifactKind::Native, "旧会话")
        .unwrap();
    app.ai_load_conversation(scope.clone(), legacy).unwrap();
    assert!(app.ai.discuss_only);
    assert_eq!(
        app.ai_submission_engine().unwrap().reasoning_effort,
        Some(ReasoningEffort::Low)
    );

    app.ai_new_conversation();
    let output = open_modes(&mut app, &context, "model");
    click(&mut app, &context, text_center(&output, "Quick"));
    frame(&mut app, &context, vec![]);
    click(&mut app, &context, effort_bounds(&context, None).center());
    egui::Popup::close_all(&context);
    app.ai_change_permission(PermissionMode::Ask);
    app.ai_record_version(
        "second",
        design::ArtifactContent::Native(Project::blank()),
        false,
    );
    let second = app.ai.session_id.unwrap();
    drop(app.ai.store.take());
    app.ai.store = Some(design::Store::open(&directory).unwrap());

    app.ai_load_conversation(scope.clone(), first).unwrap();
    assert_eq!(app.ai.selected_engine, engine_id);
    assert!(app.ai.discuss_only);
    let engine = app.ai_submission_engine().unwrap();
    assert!(engine.model.is_empty());
    assert_eq!(engine.reasoning_effort, Some(ReasoningEffort::Low));
    app.ai_load_conversation(scope, second).unwrap();
    assert_eq!(app.ai.authorization.mode(), PermissionMode::Ask);
    assert!(!app.ai.discuss_only);
    let engine = app.ai_submission_engine().unwrap();
    assert_eq!(engine.model, "quick");
    assert_eq!(engine.reasoning_effort, None);

    drop(app);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn remembered_full_mode_confirms_before_sending_in_a_new_chat() {
    let mut app = app();
    assert!(app.ai_has_full_permission());
    app.ai_new_conversation();
    assert_eq!(app.ai.authorization.mode(), ai::PermissionMode::Full);
    assert!(!app.ai_has_full_permission());
    assert!(!app.ai.authorization.confirm_full);

    app.ai.prompt = "设计页面".into();
    app.ai_submit();
    assert!(app.ai.authorization.confirm_full);
    assert!(!app.ai_has_full_permission());
    assert!(app.ai.running.is_none());
    assert_eq!(app.ai.prompt, "设计页面");
}

#[test]
fn effort_labels_include_wire_spellings_without_changing_supported_values() {
    for (value, text) in [
        (None, "自动（auto）"),
        (Some(ai::ReasoningEffort::Minimal), "最小（minimal）"),
        (Some(ai::ReasoningEffort::Low), "低（low）"),
        (Some(ai::ReasoningEffort::Medium), "中（medium）"),
        (Some(ai::ReasoningEffort::High), "高（high）"),
        (Some(ai::ReasoningEffort::Xhigh), "很高（xhigh）"),
        (Some(ai::ReasoningEffort::Max), "最高（max）"),
    ] {
        assert_eq!(effort_label(value), text);
    }
}
