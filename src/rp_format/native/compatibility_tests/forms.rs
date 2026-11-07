use super::*;

#[test]
fn new_native_forms_require_their_own_template_and_options_are_typed_records() {
    for kind in [
        WidgetKind::Radio,
        WidgetKind::TextArea,
        WidgetKind::Droplist,
        WidgetKind::ListBox,
    ] {
        let mut parts = BTreeMap::new();
        let widget = Widget::new(2, kind, 0.0, 0.0);
        let error = add_widget(&mut parts, "page", &widget, &BTreeMap::new()).unwrap_err();
        assert!(error.to_string().contains("原生模板"));
        assert!(parts.is_empty());
        let mut node = Node {
            kind: "Axure:DiagramObject:VectorShape".into(),
            properties: json!({}),
        };
        assert!(set_kind(&mut node, kind).is_err());
    }
    let mut graph = Graph::new(
        [11, 0, 0, 0],
        EMPTY_ID.into(),
        "11111111111111111111111111111111".into(),
        BTreeMap::new(),
    );
    let mut node = Node {
        kind: "Axure:DiagramObject:ComboBox".into(),
        properties: json!({}),
    };
    set_list_options(&mut graph, &mut node, "First\nSecond");
    assert_eq!(
        list_options_text(&node.properties, &graph).as_deref(),
        Some("First\nSecond")
    );
    let options = node.properties["list-options"].as_array().unwrap();
    assert_eq!(
        graph.resolve(&options[0]).unwrap().properties["selected"],
        true
    );
    assert_eq!(
        graph.resolve(&options[1]).unwrap().properties["selected"],
        false
    );
    graph.objects.insert(EMPTY_ID.into(), node);
    let encoded = binary::encode(&graph).unwrap();
    let parsed = binary::parse(&encoded).unwrap();
    assert_eq!(
        list_options_text(&parsed.root().unwrap().properties, &parsed).as_deref(),
        Some("First\nSecond")
    );
}

#[test]
fn installed_native_form_templates_save_to_copies_with_true_types() {
    let base = Path::new(r"D:\Program Files (x86)\Axure\Axure RP 11\DefaultSettings");
    let library = base.join("Libraries/Default.rplib");
    let project_path = base.join("Training/Quick Win.rp");
    if !library.is_file() || !project_path.is_file() {
        return;
    }
    let directory =
        std::env::temp_dir().join(format!("rpshop-forms-{}-{}", std::process::id(), guid()));
    std::fs::create_dir(&directory).unwrap();
    let source = std::fs::read(&library).unwrap();
    let mut native = open(&library).unwrap();
    let mut edited = native.project.clone();
    for kind in [
        WidgetKind::Radio,
        WidgetKind::TextArea,
        WidgetKind::Droplist,
        WidgetKind::ListBox,
    ] {
        let widget = edited
            .pages
            .iter_mut()
            .flat_map(|page| &mut page.widgets)
            .find(|widget| widget.kind == kind)
            .unwrap();
        widget.text = if kind.is_option_list() {
            "First\nSecond"
        } else {
            "Ingenaro form text"
        }
        .into();
        widget.name = format!("Ingenaro {}", kind.label());
        if kind == WidgetKind::Radio {
            widget.radio_group = Some("Ingenaro group".into());
        }
    }
    let destination = directory.join("forms.rplib");
    native.save(&edited, &destination).unwrap();
    let reopened = open(&destination).unwrap();
    for kind in [
        WidgetKind::Radio,
        WidgetKind::TextArea,
        WidgetKind::Droplist,
        WidgetKind::ListBox,
    ] {
        let widget = reopened
            .project
            .pages
            .iter()
            .flat_map(|page| &page.widgets)
            .find(|widget| widget.kind == kind)
            .unwrap();
        assert!(
            widget
                .name
                .ends_with(&format!(" / Ingenaro {}", kind.label())),
            "library gallery names retain their component prefix: {}",
            widget.name
        );
        assert_eq!(
            widget.text,
            if kind.is_option_list() {
                "First\nSecond"
            } else {
                "Ingenaro form text"
            }
        );
        if kind == WidgetKind::Radio {
            assert_eq!(widget.radio_group.as_deref(), Some("Ingenaro group"));
        }
    }
    assert_eq!(std::fs::read(&library).unwrap(), source);

    let source = std::fs::read(&project_path).unwrap();
    let mut native = open(&project_path).unwrap();
    let mut edited = native.project.clone();
    let id = edited.allocate_id();
    let mut widget = Widget::new(id, WidgetKind::Radio, 120.0, 160.0);
    widget.name = "Ingenaro radio template".into();
    widget.radio_group = Some("Ingenaro new group".into());
    widget.text = "Native radio".into();
    edited.pages[0].widgets.push(widget);
    let destination = directory.join("forms.rp");
    native.save(&edited, &destination).unwrap();
    let reopened = open(&destination).unwrap();
    let widget = reopened.project.pages[0]
        .widgets
        .iter()
        .find(|widget| widget.name == "Ingenaro radio template")
        .unwrap();
    assert_eq!(widget.kind, WidgetKind::Radio);
    assert_eq!(widget.text, "Native radio");
    assert_eq!(widget.radio_group.as_deref(), Some("Ingenaro new group"));
    assert_eq!(std::fs::read(&project_path).unwrap(), source);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn new_widget_ids_remain_editable_before_native_save() {
    let mut baseline = Project::blank();
    let id = baseline.allocate_id();
    baseline.pages[0]
        .widgets
        .push(Widget::new(id, WidgetKind::Rectangle, 0.0, 0.0));
    let mut bindings = BTreeMap::new();
    assert!(
        !editable_native_id(&baseline, &bindings, id),
        "known read-only previews stay read-only"
    );
    assert!(
        editable_native_id(&baseline, &bindings, baseline.next_id),
        "new unsaved widgets must be editable"
    );
    bindings.insert(
        id,
        Binding {
            object: "source/object".into(),
            scale: 1.0,
        },
    );
    assert!(editable_native_id(&baseline, &bindings, id));
}

#[test]
fn master_preview_follows_translation_rotation_and_uniform_scale() {
    let mut parent = Widget::new(1, WidgetKind::Panel, 100.0, 100.0);
    parent.width = 200.0;
    parent.height = 100.0;
    let mut child = Widget::new(2, WidgetKind::Rectangle, 120.0, 110.0);
    child.width = 40.0;
    child.height = 20.0;
    child.locked = true;
    let mut moved = parent.clone();
    moved.x += 50.0;
    moved.y -= 20.0;
    let preview = transform_preview(&child, &parent, &moved);
    close(preview.x, 170.0);
    close(preview.y, 90.0);
    assert!(preview.locked);
    let mut rotated = parent.clone();
    rotated.rotation = 90.0;
    let preview = transform_preview(&child, &parent, &rotated);
    close(preview.x, 210.0);
    close(preview.y, 80.0);
    close(preview.rotation, 90.0);
    let mut scaled = parent.clone();
    scaled.width *= 2.0;
    scaled.height *= 2.0;
    let preview = transform_preview(&child, &parent, &scaled);
    close(preview.x, 140.0);
    close(preview.y, 120.0);
    close(preview.width, 80.0);
    close(preview.height, 40.0);
}
