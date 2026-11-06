use super::*;
use crate::ui::tests::{click, fixture, frame};

#[test]
fn effort_picker_uses_live_supported_values_and_selection_updates_next_turn() {
    let mut app = fixture(Project::blank());
    app.ai.visible = true;
    let config = ai::EngineConfig::new(ai::EngineKind::Claude);
    let id = config.id.clone();
    app.ai.selected_engine = id.clone();
    app.ai.settings.engines.push(config);
    app.ai.last_project_key = app.ai_project_key();
    let catalog = ai::ModelCatalog {
        models: vec![ai::ModelInfo {
            id: "default".into(),
            name: "Default".into(),
            description: String::new(),
            is_default: true,
            reasoning_efforts: vec!["low".into(), "high".into(), "max".into()],
        }],
        source: "CLI".into(),
        warning: None,
    };
    app.ai
        .catalogs
        .insert(id, models::CatalogState::ready(catalog));
    let context = egui::Context::default();
    let _ = frame(&mut app, &context, vec![]);
    let button = toolbar::control_bounds(&context, "model").center();
    click(&mut app, &context, button);
    let _ = frame(&mut app, &context, vec![]);
    let highest = toolbar::effort_bounds(&context, Some(ai::ReasoningEffort::Max)).center();
    click(&mut app, &context, highest);
    assert_eq!(app.ai.effort_override, Some(ai::ReasoningEffort::Max));
    assert!(app.project.pages[0].widgets.is_empty());
    assert!(app.undo.is_empty());
}
