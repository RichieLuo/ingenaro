use super::*;
use ingenaro::model::EventTrigger;

fn inspector_text(output: &egui::FullOutput, expected: &str) -> Pos2 {
    fn find(shape: &egui::epaint::Shape, expected: &str) -> Option<Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == expected => {
                let center = text.pos + text.galley.size() / 2.0;
                (center.x > 1100.0).then_some(center)
            }
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().find_map(|shape| find(shape, expected))
            }
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|shape| find(&shape.shape, expected))
        .unwrap_or_else(|| panic!("inspector text {expected:?} was not painted"))
}

fn click_inspector(app: &mut App, ctx: &egui::Context, text: &str) -> egui::FullOutput {
    let output = frame(app, ctx, vec![]);
    let position = inspector_text(&output, text);
    click(app, ctx, position)
}

fn two_widget_project() -> (Project, u64, u64) {
    let mut project = Project::blank();
    project.pages[0].name = "Page A".into();
    let owner = add(&mut project, WidgetKind::Button, 80.0, 80.0);
    let target = add(&mut project, WidgetKind::Text, 350.0, 80.0);
    project.pages[0].widgets[0].name = "Owner button".into();
    project.pages[0].widgets[1].name = "Output text".into();
    let mut second = project.pages[0].clone();
    second.id = project.allocate_id();
    second.name = "Page B".into();
    second.widgets.clear();
    project.pages.push(second);
    (project, owner, target)
}

fn add_action(app: &mut App, ctx: &egui::Context, label: &str) {
    click_inspector(app, ctx, "＋ 添加动作");
    click_inspector(app, ctx, label);
}

fn replace_text(app: &mut App, ctx: &egui::Context, painted: &str, value: &str) {
    click_inspector(app, ctx, painted);
    frame(
        app,
        ctx,
        vec![
            key(egui::Key::A, egui::Modifiers::COMMAND),
            egui::Event::Text(value.into()),
        ],
    );
}

#[test]
fn page_interaction_buttons_create_edit_remove_and_undo_redo_authored_cases() {
    let (project, owner, _) = two_widget_project();
    let original = project.clone();
    let mut app = fixture(project);
    app.tab = 1;
    let ctx = egui::Context::default();
    click_inspector(&mut app, &ctx, "＋ 新建交互");
    let event = &app.project.interaction_events[0];
    assert_eq!(event.trigger, EventTrigger::PageLoad);
    assert_eq!(event.owner, None);
    assert_eq!(event.page, app.project.pages[0].id);
    assert!(event.actions.is_empty());
    assert_eq!(app.undo.len(), 1);
    assert_eq!(app.saved, original);
    add_action(&mut app, &ctx, "跳转页面");
    let action = &app.project.interaction_events[0].actions[0];
    assert_eq!(action.action, Action::Navigate);
    assert_eq!(action.target_page, Some(app.project.pages[1].id));

    click_inspector(&mut app, &ctx, "跳转页面");
    click_inspector(&mut app, &ctx, "设置组件文本");
    assert_eq!(
        app.project.interaction_events[0].actions[0].action,
        Action::SetText
    );
    assert_eq!(
        app.project.interaction_events[0].actions[0].target_widget,
        Some(owner)
    );
    assert_eq!(
        app.project.interaction_events[0].actions[0].target_page,
        None
    );
    replace_text(&mut app, &ctx, "要设置的文本", "Page load text");
    assert_eq!(
        app.project.interaction_events[0].actions[0].value,
        "Page load text"
    );
    replace_text(&mut app, &ctx, "用例1", "Welcome case");
    assert_eq!(app.project.interaction_events[0].name, "Welcome case");

    let with_action = app.project.clone();
    click_inspector(&mut app, &ctx, "×");
    assert!(app.project.interaction_events[0].actions.is_empty());
    let without_action = app.project.clone();
    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_center(&output, "撤销"));
    assert_eq!(app.project, with_action);
    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_center(&output, "重做"));
    assert_eq!(app.project, without_action);
    click_inspector(&mut app, &ctx, "删除交互");
    assert!(app.project.interaction_events.is_empty());
    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_center(&output, "撤销"));
    assert_eq!(app.project, without_action);
    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_center(&output, "重做"));
    assert!(app.project.interaction_events.is_empty());
    app.project.validate().unwrap();
}

#[test]
fn widget_click_creation_and_all_case_controls_respect_the_owner_lock() {
    let (project, owner, _) = two_widget_project();
    let mut app = fixture(project);
    app.tab = 1;
    app.selected.insert(owner);
    let ctx = egui::Context::default();
    click_inspector(&mut app, &ctx, "＋ 新建交互");
    assert_eq!(
        app.project.interaction_events[0].trigger,
        EventTrigger::Click
    );
    assert_eq!(app.project.interaction_events[0].owner, Some(owner));
    add_action(&mut app, &ctx, "设置组件文本");
    assert_eq!(
        app.project.interaction_events[0].actions[0].action,
        Action::SetText
    );
    app.project.pages[0].widgets[0].locked = true;
    let locked = app.project.clone();
    let undo = app.undo.len();
    for label in [
        "＋ 新建交互",
        "删除交互",
        "×",
        "设置组件文本",
        "＋ 添加动作",
    ] {
        click_inspector(&mut app, &ctx, label);
        assert_eq!(app.project, locked, "locked owner must block {label}");
    }
    assert_eq!(app.undo.len(), undo);
    app.project.pages[0].widgets[0].locked = false;
    click_inspector(&mut app, &ctx, "删除交互");
    assert!(app.project.interaction_events.is_empty());
}

#[test]
fn duplicate_remaps_event_owner_and_target_and_delete_prunes_only_that_owner() {
    let (project, owner, target) = two_widget_project();
    let mut app = fixture(project);
    app.tab = 1;
    app.selected.insert(owner);
    let ctx = egui::Context::default();
    click_inspector(&mut app, &ctx, "＋ 新建交互");
    add_action(&mut app, &ctx, "设置组件文本");
    click_inspector(&mut app, &ctx, "Owner button");
    click_inspector(&mut app, &ctx, "Output text");
    assert_eq!(
        app.project.interaction_events[0].actions[0].target_widget,
        Some(target)
    );
    let original_event = app.project.interaction_events[0].clone();
    app.selected = BTreeSet::from([owner, target]);
    frame(&mut app, &ctx, vec![]);
    frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::D, egui::Modifiers::CTRL)],
    );
    assert_eq!(app.project.interaction_events.len(), 2);
    let copied_owner = app.project.pages[0]
        .widgets
        .iter()
        .find(|w| app.selected.contains(&w.id) && w.name == "Owner button 副本")
        .unwrap()
        .id;
    let copied_target = app.project.pages[0]
        .widgets
        .iter()
        .find(|w| app.selected.contains(&w.id) && w.name == "Output text 副本")
        .unwrap()
        .id;
    let copied = app
        .project
        .interaction_events
        .iter()
        .find(|event| event.owner == Some(copied_owner))
        .unwrap();
    assert_ne!(copied.id, original_event.id);
    assert_eq!(copied.actions[0].target_widget, Some(copied_target));
    assert_eq!(app.project.interaction_events[0], original_event);
    app.selected = BTreeSet::from([copied_owner]);
    frame(&mut app, &ctx, vec![]);
    let before_delete = app.project.clone();
    frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Delete, egui::Modifiers::NONE)],
    );
    assert_eq!(app.project.interaction_events, vec![original_event]);
    assert!(
        app.project.pages[0]
            .widgets
            .iter()
            .all(|widget| widget.id != copied_owner)
    );
    assert!(
        app.project.pages[0]
            .widgets
            .iter()
            .any(|widget| widget.id == copied_target)
    );
    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_center(&output, "撤销"));
    assert_eq!(app.project, before_delete);
    app.project.validate().unwrap();
}
