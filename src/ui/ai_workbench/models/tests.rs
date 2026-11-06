use super::*;

#[test]
fn completed_model_discovery_is_applied_without_manual_refresh() {
    let (tx, rx) = mpsc::channel();
    let mut state = CatalogState::qa(ai::builtin_models(ai::EngineKind::Claude), rx);
    let mut live = ai::builtin_models(ai::EngineKind::Claude);
    live.models.push(ai::ModelInfo {
        id: "live-only-model".into(),
        name: "Live-only model".into(),
        description: String::new(),
        is_default: false,
        reasoning_efforts: vec!["high".into()],
    });
    live.source = "live CLI directory".into();
    live.warning = None;
    tx.send(Ok(live.clone())).unwrap();
    state.poll(Instant::now());
    assert_eq!(state.catalog, live);
    assert!(state.receiver.is_none());
    assert!(!state.needs_request(&state.fingerprint, false, Instant::now()));
}

#[test]
fn failed_or_disconnected_catalog_reads_retry_without_losing_models() {
    for disconnected in [false, true] {
        let (tx, rx) = mpsc::channel();
        let catalog = ai::builtin_models(ai::EngineKind::Claude);
        let mut state = CatalogState::qa(catalog.clone(), rx);
        if !disconnected {
            tx.send(Err("temporary failure".into())).unwrap();
        }
        drop(tx);
        let now = Instant::now();
        state.poll(now);
        assert!(state.receiver.is_none());
        assert_eq!(state.catalog.models, catalog.models);
        assert!(state.catalog.warning.is_some());
        assert!(!state.needs_request(&state.fingerprint, false, now));
        assert!(state.needs_request(&state.fingerprint, false, now + Duration::from_secs(16)));
        assert!(state.needs_request(&state.fingerprint, true, now));
        assert!(state.needs_request("changed CLI path", false, now));
    }
}

#[test]
fn live_catalog_arrives_in_the_open_picker_without_refresh_clicks() {
    use crate::ui::tests::{click, fixture, frame, text_center};
    let mut app = fixture(Project::blank());
    app.ai.visible = true;
    app.ai.last_project_key = app.ai_project_key();
    let engine = ai::EngineConfig::new(ai::EngineKind::Claude);
    let id = engine.id.clone();
    app.ai.selected_engine = id.clone();
    app.ai.settings.engines.push(engine);
    let (tx, rx) = mpsc::channel();
    app.ai.catalogs.insert(
        id.clone(),
        CatalogState::qa(ai::builtin_models(ai::EngineKind::Claude), rx),
    );
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    let model_button = composer::toolbar::control_bounds(&ctx, "model").center();
    click(&mut app, &ctx, model_button);
    let mut catalog = ai::builtin_models(ai::EngineKind::Claude);
    catalog.models.push(ai::ModelInfo {
        id: "new-live-row".into(),
        name: "New live row".into(),
        description: String::new(),
        is_default: false,
        reasoning_efforts: vec!["low".into()],
    });
    tx.send(Ok(catalog)).unwrap();
    frame(&mut app, &ctx, vec![]);
    let output = frame(&mut app, &ctx, vec![]);
    text_center(&output, "New live row");
    assert!(app.ai.catalogs[&id].receiver.is_none());
    assert!(egui::Popup::is_any_open(&ctx));
}
