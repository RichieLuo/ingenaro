use super::*;

#[test]
fn toolbar_adds_ai_tool_and_persists_removal_across_launches() {
    let mut storage = MemoryStorage::default();
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, &storage);
    assert!(!app.plugins.contains(plugins::Plugin::AiTool));
    assert!(!app.ai.visible);
    let project = app.project.clone();

    let output = frame(&mut app, &ctx, vec![]);
    let entry = text_center(&output, "添加插件");
    click(&mut app, &ctx, entry);
    let _ = frame(&mut app, &ctx, vec![]);
    let output = frame(&mut app, &ctx, vec![]);
    assert!(app.plugins.open);
    let _ = text_center(&output, "AI 工具");
    click(&mut app, &ctx, text_center(&output, "添加"));
    assert!(app.plugins.contains(plugins::Plugin::AiTool));
    assert!(app.ai.visible);
    assert_eq!(app.project, project);
    app.save(&mut storage);

    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, &storage);
    assert!(app.plugins.contains(plugins::Plugin::AiTool));
    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_center(&output, "添加插件"));
    let _ = frame(&mut app, &ctx, vec![]);
    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_center(&output, "移除"));
    assert!(!app.plugins.contains(plugins::Plugin::AiTool));
    assert!(!app.ai.visible);
    assert_eq!(app.project, project);
    app.save(&mut storage);

    let restarted = new_app(&egui::Context::default(), &storage);
    assert!(!restarted.plugins.contains(plugins::Plugin::AiTool));
    assert!(!restarted.plugins.open);
}

fn new_app(ctx: &egui::Context, storage: &MemoryStorage) -> App {
    let mut creation = eframe::CreationContext::_new_kittest(ctx.clone());
    creation.storage = Some(storage);
    App::new(&creation, None)
}
