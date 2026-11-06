use super::*;
use crate::ui::tests::{click, fixture, frame, text_center};

#[test]
fn selected_widget_shows_reference_chip_without_typing_at_and_x_removes_it() {
    let mut app = fixture(Project::default());
    app.ai.visible = true;
    let id = app.project.pages[0].widgets[0].id;
    app.project.pages[0].widgets[0].name = "自动引用专用元件".into();
    app.selected.insert(id);
    let context = egui::Context::default();
    let output = frame(&mut app, &context, vec![]);
    let chip = text_center(&output, "自动引用专用元件");
    assert!(chip.x > 1000.0);
    assert!(app.ai.references.selected.is_some());
    assert!(!app.ai.prompt.contains('@'));
    let remove = context
        .data(|data| data.get_temp::<Rect>(egui::Id::new("ai-reference-remove-test")))
        .unwrap();
    click(&mut app, &context, remove.center());
    frame(&mut app, &context, vec![]);
    assert!(app.ai.references.selected.is_none());
    assert_eq!(app.ai.context, 3);
    assert!(
        app.selected.contains(&id),
        "Removing a reference must not deselect the canvas"
    );
    app.selected.clear();
    frame(&mut app, &context, vec![]);
    app.selected.insert(id);
    frame(&mut app, &context, vec![]);
    assert!(app.ai.references.selected.is_some());
}

#[test]
fn attached_image_preview_opens_and_remove_click_cannot_open_it_again() {
    let mut app = fixture(Project::blank());
    app.ai.visible = true;
    let before = app.project.clone();
    let image = ai::ImageAttachment::from_rgba("参考图", 32, 24, &vec![150; 32 * 24 * 4]).unwrap();
    let id = image.id().to_owned();
    app.ai.images.push(image);
    let context = egui::Context::default();
    frame(&mut app, &context, vec![]);
    let rect = context
        .data(|data| data.get_temp::<Rect>(egui::Id::new(("ai-image-open-test", id.as_str()))))
        .unwrap();
    click(&mut app, &context, rect.center());
    assert_eq!(app.ai.image_draft.preview.as_deref(), Some(id.as_str()));
    app.ai.image_draft.preview = None;
    frame(&mut app, &context, vec![]);
    let rect = context
        .data(|data| data.get_temp::<Rect>(egui::Id::new(("ai-image-remove-test", id.as_str()))))
        .unwrap();
    click(&mut app, &context, rect.center());
    assert!(app.ai.images.is_empty());
    assert!(app.ai.image_draft.preview.is_none());
    assert_eq!(app.project, before);
    assert!(app.undo.is_empty());
}
