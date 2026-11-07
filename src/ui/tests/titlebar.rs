use super::*;

fn header_frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 720.0))),
            events,
            ..Default::default()
        },
        |ctx| app.header(ctx),
    )
}

fn app_with_logo(ctx: &egui::Context) -> App {
    let mut app = fixture(Project::default());
    app.logo = Some(ctx.load_texture(
        "titlebar-test-logo",
        egui::ColorImage::filled([1, 1], Color32::WHITE),
        egui::TextureOptions::LINEAR,
    ));
    app
}

#[test]
fn macos_titlebar_clears_native_controls_and_keeps_the_file_menu() {
    let ctx = egui::Context::default();
    let mut app = app_with_logo(&ctx);
    let output = header_frame(&mut app, &ctx, vec![]);
    let brand = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "RPShop" => Some(text),
            _ => None,
        })
        .unwrap();
    assert_eq!(brand.pos.x, 130.0);
    assert!(brand.pos.y >= 0.0 && brand.pos.y + brand.galley.size().y <= 44.0);
    assert!(text_center(&output, "▶  预览原型").y < 44.0);
    let file = text_center(&output, "文件");
    assert!(file.y > 44.0);
    let _ = header_frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(file), pointer(file, true)],
    );
    let _ = header_frame(&mut app, &ctx, vec![pointer(file, false)]);
    assert!(egui::Popup::is_any_open(&ctx));
}

#[test]
fn macos_brand_area_starts_a_native_window_drag() {
    let ctx = egui::Context::default();
    let mut app = app_with_logo(&ctx);
    let output = header_frame(&mut app, &ctx, vec![]);
    let brand = text_center(&output, "RPShop");
    let output = header_frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(brand), pointer(brand, true)],
    );
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::StartDrag)
    );
}
