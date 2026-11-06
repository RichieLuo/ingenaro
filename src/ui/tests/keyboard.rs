use super::*;

fn component_clipboard(output: &egui::FullOutput) -> String {
    output
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            egui::OutputCommand::CopyText(contents) => Some(contents.clone()),
            _ => None,
        })
        .expect("component copy must publish a native system clipboard command")
}

#[test]
fn system_clipboard_remaps_names_of_nested_groups_on_paste() {
    let mut project = Project::blank();
    let first = add(&mut project, WidgetKind::Text, 30.0, 30.0);
    let second = add(&mut project, WidgetKind::Text, 50.0, 50.0);
    let third = add(&mut project, WidgetKind::Text, 70.0, 70.0);
    let (inner, _) = rpshop::grouping::group_named(
        &mut project,
        0,
        &BTreeSet::from([first, second]),
        Some("Inner"),
    )
    .unwrap();
    let (outer, selected) = rpshop::grouping::group_named(
        &mut project,
        0,
        &BTreeSet::from([first, third]),
        Some("Outer"),
    )
    .unwrap();
    let mut app = fixture(project);
    app.selected = selected;
    let ctx = egui::Context::default();
    let copied = frame(&mut app, &ctx, vec![egui::Event::Copy]);
    let clipboard = component_clipboard(&copied);
    assert!(clipboard.starts_with("RPShop.components.v2\n"));

    let mut target = fixture(app.project.clone());
    let ctx = egui::Context::default();
    frame(&mut target, &ctx, vec![egui::Event::Paste(clipboard)]);
    let page = &target.project.pages[0];
    let [copied_outer, copied_inner] = page.widgets[3].groups.as_slice() else {
        panic!("nested group path did not survive paste");
    };
    assert_ne!(*copied_outer, outer);
    assert_ne!(*copied_inner, inner);
    assert_eq!(page.group_name(*copied_outer), "Outer");
    assert_eq!(page.group_name(*copied_inner), "Inner");
    target.project.validate().unwrap();
}

#[test]
fn native_clipboard_events_copy_and_paste_across_app_instances() {
    let mut project = Project::blank();
    let id = add(&mut project, WidgetKind::Button, 80.0, 80.0);
    let original = project.pages[0].widgets[0].clone();
    let mut source = fixture(project);
    source.selected.insert(id);
    let ctx = egui::Context::default();
    let _ = frame(&mut source, &ctx, Vec::new());
    let copied = frame(&mut source, &ctx, vec![egui::Event::Copy]);
    let clipboard = component_clipboard(&copied);
    assert!(source.undo.is_empty(), "copy is not a document edit");

    let mut destination = fixture(Project::blank());
    let destination_ctx = egui::Context::default();
    let _ = frame(&mut destination, &destination_ctx, Vec::new());
    let _ = frame(
        &mut destination,
        &destination_ctx,
        vec![egui::Event::Paste(clipboard)],
    );
    assert_eq!(destination.project.pages[0].widgets.len(), 1);
    let pasted = &destination.project.pages[0].widgets[0];
    assert_eq!(pasted.kind, original.kind);
    assert_eq!(pasted.text, original.text);
    assert_eq!(pasted.x, original.x + 24.0);
    assert_eq!(destination.undo.len(), 1);
    destination.project.validate().unwrap();
}

#[test]
fn native_cut_preserves_locked_widgets_and_is_one_undo_transaction() {
    let mut project = Project::blank();
    let movable = add(&mut project, WidgetKind::Rectangle, 80.0, 80.0);
    let locked = add(&mut project, WidgetKind::Button, 320.0, 80.0);
    project.pages[0].widgets[1].locked = true;
    let original = project.clone();
    let mut app = fixture(project);
    app.selected = BTreeSet::from([movable, locked]);
    let ctx = egui::Context::default();
    let _ = frame(&mut app, &ctx, Vec::new());
    let output = frame(&mut app, &ctx, vec![egui::Event::Cut]);
    let clipboard = component_clipboard(&output);
    assert!(clipboard.starts_with("RPShop.components.v1\n"));
    assert_eq!(app.clipboard.widgets.len(), 1);
    assert_eq!(app.clipboard.widgets[0].id, movable);
    assert_eq!(app.project.pages[0].widgets.len(), 1);
    assert_eq!(app.project.pages[0].widgets[0].id, locked);
    assert_eq!(app.undo.len(), 1);
    app.undo();
    assert_eq!(app.project, original);
}

#[test]
fn shift_z_redoes_and_backspace_deletes_as_single_edits() {
    let mut project = Project::blank();
    let id = add(&mut project, WidgetKind::Rectangle, 80.0, 80.0);
    let mut app = fixture(project);
    app.selected.insert(id);
    let ctx = egui::Context::default();
    let _ = frame(&mut app, &ctx, Vec::new());
    let _ = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Backspace, egui::Modifiers::NONE)],
    );
    assert!(app.project.pages[0].widgets.is_empty());
    assert_eq!(app.undo.len(), 1);
    let _ = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Z, egui::Modifiers::CTRL)],
    );
    assert_eq!(app.project.pages[0].widgets.len(), 1);
    let _ = frame(
        &mut app,
        &ctx,
        vec![key(
            egui::Key::Z,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        )],
    );
    assert!(app.project.pages[0].widgets.is_empty());
    assert_eq!(app.undo.len(), 1);
    assert!(app.redo.is_empty());
}

#[test]
fn focused_text_edit_keeps_clipboard_delete_tools_and_undo_events() {
    let mut project = Project::blank();
    let id = add(&mut project, WidgetKind::Rectangle, 80.0, 80.0);
    let original = project.clone();
    let mut app = fixture(project);
    app.selected.insert(id);
    let ctx = egui::Context::default();
    let focus = egui::Id::new("keyboard-test-text");
    let mut text = String::from("editable text");
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add(egui::TextEdit::singleline(&mut text).id(focus))
                .request_focus();
        });
    });
    let events = vec![
        egui::Event::Copy,
        egui::Event::Cut,
        egui::Event::Paste("text paste".into()),
        key(egui::Key::Backspace, egui::Modifiers::NONE),
        key(egui::Key::R, egui::Modifiers::NONE),
        key(egui::Key::G, egui::Modifiers::CTRL),
        key(egui::Key::Z, egui::Modifiers::CTRL),
    ];
    let _ = ctx.run(
        egui::RawInput {
            events: events.clone(),
            ..Default::default()
        },
        |ctx| {
            assert!(ctx.wants_keyboard_input());
            app.keyboard(ctx);
            assert_eq!(ctx.input(|input| input.events.clone()), events);
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add(egui::TextEdit::singleline(&mut text).id(focus));
            });
        },
    );
    assert_eq!(app.project, original);
    assert!(app.clipboard.widgets.is_empty());
    assert!(app.undo.is_empty());
    assert!(app.tool.is_none());
}

#[test]
fn save_as_resolves_before_save_even_with_keyboard_focus() {
    let ctx = egui::Context::default();
    for (modifiers, save_as) in [
        (egui::Modifiers::CTRL | egui::Modifiers::SHIFT, true),
        (egui::Modifiers::CTRL, false),
    ] {
        let _ = ctx.run(
            egui::RawInput {
                events: vec![key(egui::Key::S, modifiers)],
                ..Default::default()
            },
            |ctx| {
                ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("save-text")));
                assert!(ctx.wants_keyboard_input());
                assert_eq!(App::consume_save_shortcut(ctx), Some(save_as));
                assert_eq!(App::consume_save_shortcut(ctx), None);
            },
        );
    }
}

#[test]
fn external_text_and_invalid_component_payload_never_paste_stale_widgets() {
    let mut project = Project::blank();
    let id = add(&mut project, WidgetKind::Rectangle, 80.0, 80.0);
    let mut app = fixture(project);
    app.selected.insert(id);
    app.copy();
    let before = app.project.clone();
    let ctx = egui::Context::default();
    let _ = frame(&mut app, &ctx, Vec::new());
    for contents in [
        "ordinary external text".to_owned(),
        "RPShop.components.v1\n[{}]".to_owned(),
        {
            let mut bad = app.clipboard.widgets[0].clone();
            bad.width = -1.0;
            format!(
                "RPShop.components.v1\n{}",
                serde_json::to_string(&vec![bad]).unwrap()
            )
        },
    ] {
        let _ = frame(&mut app, &ctx, vec![egui::Event::Paste(contents)]);
        assert_eq!(app.project, before);
        assert!(app.undo.is_empty());
    }
}

#[test]
fn tool_alignment_and_resize_shortcuts_use_exact_modifiers() {
    let mut project = Project::blank();
    let first = add(&mut project, WidgetKind::Rectangle, 80.0, 80.0);
    let second = add(&mut project, WidgetKind::Rectangle, 320.0, 240.0);
    let mut app = fixture(project);
    app.selected = BTreeSet::from([first, second]);
    let ctx = egui::Context::default();
    let _ = frame(&mut app, &ctx, Vec::new());
    let _ = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::L, egui::Modifiers::NONE)],
    );
    assert_eq!(app.tool, Some(WidgetKind::Line));
    let _ = frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::V, egui::Modifiers::NONE)],
    );
    assert_eq!(app.tool, None);
    let _ = frame(
        &mut app,
        &ctx,
        vec![key(
            egui::Key::L,
            egui::Modifiers::CTRL | egui::Modifiers::ALT,
        )],
    );
    assert_eq!(app.project.pages[0].widgets[0].x, 80.0);
    assert_eq!(app.project.pages[0].widgets[1].x, 80.0);
    assert_eq!(app.tool, None, "modified L must not select the line tool");
    assert_eq!(app.undo.len(), 1);
    let before = app.project.clone();
    let _ = frame(
        &mut app,
        &ctx,
        vec![key(
            egui::Key::ArrowRight,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        )],
    );
    for (old, changed) in before.pages[0]
        .widgets
        .iter()
        .zip(&app.project.pages[0].widgets)
    {
        assert_eq!(changed.x, old.x);
        assert_eq!(changed.y, old.y);
        assert_eq!(changed.width, old.width + 10.0);
        assert_eq!(changed.height, old.height);
    }
    assert_eq!(app.undo.len(), 2);
}

#[test]
fn lock_shortcut_locks_mixed_selection_then_unlocks_all() {
    let mut project = Project::blank();
    let first = add(&mut project, WidgetKind::Rectangle, 80.0, 80.0);
    let second = add(&mut project, WidgetKind::Rectangle, 320.0, 240.0);
    project.pages[0].widgets[1].locked = true;
    let mut app = fixture(project);
    app.selected = BTreeSet::from([first, second]);
    let ctx = egui::Context::default();
    let _ = frame(&mut app, &ctx, Vec::new());
    let lock_key = || key(egui::Key::K, egui::Modifiers::CTRL | egui::Modifiers::SHIFT);
    let _ = frame(&mut app, &ctx, vec![lock_key()]);
    assert!(
        app.project.pages[0]
            .widgets
            .iter()
            .all(|widget| widget.locked)
    );
    assert_eq!(app.undo.len(), 1);
    let _ = frame(&mut app, &ctx, vec![lock_key()]);
    assert!(
        app.project.pages[0]
            .widgets
            .iter()
            .all(|widget| !widget.locked)
    );
    assert_eq!(app.undo.len(), 2);
}

#[test]
fn native_copy_event_with_control_alt_aligns_center_instead_of_copying() {
    let mut project = Project::blank();
    let first = add(&mut project, WidgetKind::Rectangle, 80.0, 80.0);
    let second = add(&mut project, WidgetKind::Rectangle, 320.0, 240.0);
    let mut app = fixture(project);
    app.selected = BTreeSet::from([first, second]);
    let ctx = egui::Context::default();
    let _ = frame(&mut app, &ctx, Vec::new());
    let output = ctx.run(
        egui::RawInput {
            modifiers: egui::Modifiers::CTRL | egui::Modifiers::ALT,
            events: vec![egui::Event::Copy],
            ..Default::default()
        },
        |ctx| app.keyboard(ctx),
    );
    assert_eq!(
        app.project.pages[0].widgets[0].x,
        app.project.pages[0].widgets[1].x
    );
    assert_eq!(app.undo.len(), 1);
    assert!(app.clipboard.widgets.is_empty());
    assert!(output.platform_output.commands.is_empty());
}
