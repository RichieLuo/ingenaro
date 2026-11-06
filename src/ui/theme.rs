use super::*;

pub(super) fn setup_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let cjk_font = "noto-cjk-sc";
    fonts.font_data.insert(
        cjk_font.into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/NotoSansCJKsc-Regular.otf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, cjk_font.into());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, cjk_font.into());
    let mut bold_names = Vec::new();
    for (name, path) in [
        ("canvas-arial-bold", "C:/Windows/Fonts/arialbd.ttf"),
        ("canvas-cjk-bold", "C:/Windows/Fonts/msyhbd.ttc"),
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                name.into(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            bold_names.push(name.to_owned());
        }
    }
    if !bold_names.is_empty() {
        bold_names.extend(fonts.families[&egui::FontFamily::Proportional].clone());
        fonts
            .families
            .insert(egui::FontFamily::Name("canvas-bold".into()), bold_names);
    }
    ctx.set_fonts(fonts);
    let mut style = egui::Style {
        visuals: egui::Visuals::light(),
        ..Default::default()
    };
    style.visuals.override_text_color = Some(INK);
    style.visuals.panel_fill = Color32::WHITE;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    // Keep native egui selection, caret and interaction colors. Brand colors
    // belong to explicit product actions, not ordinary editing feedback.
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(247, 247, 251);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(247, 247, 251);
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    style.spacing.item_spacing = vec2(7.0, 7.0);
    style.spacing.button_padding = vec2(10.0, 6.0);
    style.spacing.interact_size = vec2(30.0, 29.0);
    style.spacing.text_edit_width = 205.0;
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(12.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(11.0));
    ctx.set_theme(egui::Theme::Light);
    ctx.set_style_of(egui::Theme::Light, style);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_editing_feedback_uses_framework_defaults_not_brand_colors() {
        let context = egui::Context::default();
        setup_style(&context);
        let actual = context.style();
        let defaults = egui::Visuals::light();
        assert_eq!(actual.visuals.selection, defaults.selection);
        assert_eq!(actual.visuals.text_cursor, defaults.text_cursor);
        assert_eq!(actual.visuals.widgets.hovered, defaults.widgets.hovered);
        assert_eq!(actual.visuals.hyperlink_color, defaults.hyperlink_color);
        assert_eq!(actual.visuals.override_text_color, Some(INK));
        assert_ne!(actual.visuals.selection.stroke.color, PURPLE);
    }

    #[test]
    fn selected_text_is_painted_with_default_selection_colors() {
        let context = egui::Context::default();
        setup_style(&context);
        let editor = egui::Id::new("theme-selection-regression");
        let mut text = "页面和图层".to_owned();
        let render = |text: &mut String| {
            context.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add(egui::TextEdit::singleline(text).id(editor));
                });
            })
        };
        render(&mut text);
        context.memory_mut(|memory| memory.request_focus(editor));
        let mut state = egui::TextEdit::load_state(&context, editor).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(text.chars().count()),
            )));
        state.store(&context, editor);
        let output = render(&mut text);
        let defaults = egui::Visuals::light().selection;
        let galley = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == "页面和图层" => {
                    Some(&text.galley)
                }
                _ => None,
            })
            .expect("selected editor text was painted");
        let colors = galley
            .rows
            .iter()
            .flat_map(|row| row.visuals.mesh.vertices.iter().map(|vertex| vertex.color))
            .collect::<Vec<_>>();
        assert!(colors.contains(&defaults.bg_fill));
        assert!(colors.contains(&defaults.stroke.color));
        assert!(!colors.contains(&PURPLE));
        assert!(!colors.contains(&Color32::from_rgb(239, 233, 255)));
    }
}
