use super::*;
use crate::ui::tests::{click, fixture, frame, text_center};

fn app() -> App {
    let mut app = fixture(Project::blank());
    app.ai.visible = true;
    let config = ai::EngineConfig::new(ai::EngineKind::Claude);
    let id = config.id.clone();
    app.ai.selected_engine = id.clone();
    app.ai.settings.engines.push(config);
    app.ai.last_project_key = app.ai_project_key();
    let catalog = ai::ModelCatalog {
        models: vec![ai::ModelInfo {
            id: "live-alias".into(),
            name: "真实模型 A".into(),
            description: "CLI 返回".into(),
            is_default: true,
            reasoning_efforts: vec![],
        }],
        source: "单元测试实时目录".into(),
        warning: None,
    };
    app.ai
        .catalogs
        .insert(id, models::CatalogState::ready(catalog));
    app
}
#[test]
fn model_picker_selects_actual_row_without_manual_entry() {
    let mut app = app();
    let context = egui::Context::default();
    let _ = frame(&mut app, &context, vec![]);
    let button = composer::toolbar::control_bounds(&context, "model").center();
    click(&mut app, &context, button);
    let output = frame(&mut app, &context, vec![]);
    let row = text_center(&output, "真实模型 A  ·  CLI 推荐");
    click(&mut app, &context, row);
    assert_eq!(app.ai.model_override.as_deref(), Some("live-alias"));
}
#[test]
fn reopening_model_picker_clears_previous_search_filter() {
    let mut app = app();
    let context = egui::Context::default();
    frame(&mut app, &context, vec![]);
    app.ai.model_query = "no matching model".into();
    let model_button = composer::toolbar::control_bounds(&context, "model").center();
    click(&mut app, &context, model_button);
    let output = frame(&mut app, &context, vec![]);
    assert!(app.ai.model_query.is_empty());
    text_center(&output, "真实模型 A  ·  CLI 推荐");
}

#[test]
fn ai_sidebar_width_changes_when_dragging_its_left_edge() {
    let mut app = app();
    let context = egui::Context::default();
    for _ in 0..3 {
        frame(&mut app, &context, vec![]);
    }
    let bounds = |context: &egui::Context| {
        context
            .data(|data| data.get_temp::<Rect>(egui::Id::new("ai-chat-panel-test")))
            .unwrap()
    };
    let original = bounds(&context);
    // Drag just inside the divider, not the canvas's inclusive right boundary.
    let from = original.left_center() + vec2(2.0, 0.0);
    let to = from - vec2(90.0, 0.0);
    let pointer = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&mut app, &context, vec![egui::Event::PointerMoved(from)]);
    frame(&mut app, &context, vec![pointer(from, true)]);
    frame(&mut app, &context, vec![egui::Event::PointerMoved(to)]);
    frame(&mut app, &context, vec![pointer(to, false)]);
    frame(&mut app, &context, vec![]);
    let resized = bounds(&context);
    assert!(
        resized.width() > original.width() + 60.0,
        "before={original:?}, after={resized:?}"
    );
    assert!((resized.right() - original.right()).abs() < 1.0);
    frame(&mut app, &context, vec![]);
    assert_eq!(bounds(&context), resized);
}

#[test]
fn settings_offer_only_local_agents_and_preserve_hidden_api_configuration() {
    let mut app = app();
    let old = ai::EngineConfig::new(ai::EngineKind::OpenAiCompatible);
    let old_id = old.id.clone();
    app.ai.settings.engines.push(old);
    app.ai.settings_open = true;
    let context = egui::Context::default();
    frame(&mut app, &context, vec![]);
    let output = frame(&mut app, &context, vec![]);
    assert!(text_center(&output, "本地智能体").y > 0.0);
    assert!(
        app.ai
            .settings
            .engines
            .iter()
            .any(|engine| engine.id == old_id)
    );
}

#[test]
fn settings_engine_card_selects_from_blank_area() {
    let mut app = app();
    let codex = ai::EngineConfig::new(ai::EngineKind::Codex);
    let id = codex.id.clone();
    app.ai.settings.engines.push(codex);
    app.ai.settings_open = true;
    let context = egui::Context::default();
    frame(&mut app, &context, vec![]);
    frame(&mut app, &context, vec![]);
    let response = context
        .read_response(egui::Id::new(("settings-engine-card", &id)))
        .unwrap();
    let blank = response.rect.right_bottom() - vec2(8.0, 8.0);
    click(&mut app, &context, blank);
    assert_eq!(app.ai.settings_engine, id);
}
