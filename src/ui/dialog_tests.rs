use super::*;
use crate::ui::tests::{click, fixture, frame, text_center};

#[test]
fn authorization_uses_shared_surface_and_grants_only_after_primary_action() {
    let mut app = fixture(Project::blank());
    app.ai.visible = true;
    let context = egui::Context::default();
    frame(&mut app, &context, vec![]);
    app.ai_request_full_permission_test();
    frame(&mut app, &context, vec![]);
    let output = frame(&mut app, &context, vec![]);
    assert!(!app.ai_has_full_permission());
    let button = text_center(&output, "确认完全授权本会话");
    let title = text_center(&output, "完全授权当前 AI 会话？");
    assert!(button.y > title.y + 150.0);
    assert!(button.x > title.x);
    click(&mut app, &context, button);
    assert!(app.ai_has_full_permission());
}

#[test]
fn unsaved_cancel_preserves_document_and_text_dialog_completion_keeps_edit() {
    let mut app = fixture(Project::default());
    let before = app.project.clone();
    app.confirm = Some("new".into());
    let context = egui::Context::default();
    let output = frame(&mut app, &context, vec![]);
    let cancel = text_center(&output, "取消");
    click(&mut app, &context, cancel);
    assert!(app.confirm.is_none());
    assert_eq!(app.project, before);
    let id = app.project.pages[0].widgets[0].id;
    app.editing = Some(id);
    let _ = frame(&mut app, &context, vec![]);
    let output = frame(&mut app, &context, vec![]);
    let done = text_center(&output, "完成");
    click(&mut app, &context, done);
    assert!(app.editing.is_none());
    assert_eq!(app.project, before);
}

#[test]
fn input_placeholder_is_muted_and_distinct_from_user_text() {
    fn texts<'a>(shape: &'a egui::epaint::Shape, output: &mut Vec<&'a egui::epaint::TextShape>) {
        match shape {
            egui::epaint::Shape::Text(shape) => output.push(shape),
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    texts(shape, output)
                }
            }
            _ => {}
        }
    }
    let mut app = fixture(Project::blank());
    app.ai.visible = true;
    let context = egui::Context::default();
    let output = frame(&mut app, &context, vec![]);
    let mut labels = vec![];
    for shape in &output.shapes {
        texts(&shape.shape, &mut labels);
    }
    let hint = labels
        .iter()
        .find(|label| label.galley.text() == "描述你的设计，选中元件会自动引用，Enter 发送")
        .unwrap();
    let color = hint.galley.rows[0].visuals.mesh.vertices[0].color;
    assert!(
        color.r() >= 140 && color.g() >= 140 && color.b() >= 140,
        "placeholder must be muted: {color:?}"
    );
    assert_ne!(color, INK);
}
