use super::*;

#[test]
fn form_components_roundtrip_and_preserve_explicit_radio_groups() {
    let mut project = Project::blank();
    for kind in [
        WidgetKind::Radio,
        WidgetKind::TextArea,
        WidgetKind::Droplist,
        WidgetKind::ListBox,
    ] {
        let id = project.allocate_id();
        let mut widget = Widget::new(id, kind, 20.0, 30.0);
        if kind == WidgetKind::Radio {
            widget.radio_group = Some("preferences".into());
        }
        project.pages[0].widgets.push(widget);
    }
    project.validate().unwrap();
    let encoded = serde_json::to_string(&project).unwrap();
    assert_eq!(serde_json::from_str::<Project>(&encoded).unwrap(), project);
    let legacy = Widget::new(7, WidgetKind::Checkbox, 0.0, 0.0);
    let encoded = serde_json::to_value(&legacy).unwrap();
    assert!(encoded.get("radio_group").is_none());
    assert_eq!(serde_json::from_value::<Widget>(encoded).unwrap(), legacy);
}

#[test]
fn group_paths_roundtrip_and_older_widgets_default_to_ungrouped() {
    let mut project = Project::blank();
    for path in [vec![10, 11], vec![10, 11], vec![10, 12]] {
        let id = project.allocate_id();
        let mut widget = Widget::new(id, WidgetKind::Rectangle, 0.0, 0.0);
        widget.groups = path;
        project.pages[0].widgets.push(widget);
    }
    project.next_id = 13;
    project.validate().unwrap();
    assert_eq!(
        serde_json::from_value::<Project>(serde_json::to_value(&project).unwrap()).unwrap(),
        project
    );
    let mut old_widget = serde_json::to_value(&project.pages[0].widgets[0]).unwrap();
    old_widget.as_object_mut().unwrap().remove("groups");
    assert!(
        serde_json::from_value::<Widget>(old_widget)
            .unwrap()
            .groups
            .is_empty()
    );
}

#[test]
fn validates_group_identity_paths_pages_and_contiguous_layers() {
    let mut valid = Project::blank();
    for _ in 0..3 {
        let id = valid.allocate_id();
        let mut widget = Widget::new(id, WidgetKind::Rectangle, 0.0, 0.0);
        widget.groups = vec![10];
        valid.pages[0].widgets.push(widget);
    }
    valid.next_id = 11;
    valid.validate().unwrap();
    for path in [
        vec![0],
        vec![1],
        vec![2],
        vec![10, 10],
        (100..133).collect(),
    ] {
        let mut invalid = valid.clone();
        invalid.pages[0].widgets[0].groups = path;
        invalid.next_id = 134;
        assert!(invalid.validate().is_err());
    }
    let mut invalid = valid.clone();
    invalid.pages[0].widgets[1].groups = vec![11, 10];
    invalid.next_id = 12;
    assert!(
        invalid.validate().is_err(),
        "same group cannot have different ancestors"
    );
    let mut invalid = valid.clone();
    invalid.pages[0].widgets[1].groups.clear();
    assert!(
        invalid.validate().is_err(),
        "group members must occupy consecutive layers"
    );
    let mut invalid = valid.clone();
    let mut second = invalid.pages[0].clone();
    second.id = 5;
    second.widgets.truncate(1);
    second.widgets[0].id = 6;
    invalid.pages.push(second);
    assert!(
        invalid.validate().is_err(),
        "a group belongs to exactly one page"
    );
    let mut invalid = valid;
    invalid.next_id = 10;
    assert!(
        invalid.validate().is_err(),
        "the allocator includes group IDs"
    );
}

#[test]
fn page_tree_roundtrips_and_validates_unique_complete_references() {
    let mut project = Project::default();
    let first = project.pages[0].id;
    let second = project.pages[1].id;
    project.page_tree = vec![PageTreeNode::Folder {
        id: "folder-a".into(),
        name: "文档".into(),
        children: vec![PageTreeNode::Page {
            page_id: first,
            children: vec![PageTreeNode::Page {
                page_id: second,
                children: vec![],
            }],
        }],
    }];
    project.validate().unwrap();
    let value = serde_json::to_value(&project).unwrap();
    assert_eq!(
        serde_json::from_value::<Project>(value.clone()).unwrap(),
        project
    );
    let mut old = value;
    old.as_object_mut().unwrap().remove("page_tree");
    assert!(
        serde_json::from_value::<Project>(old)
            .unwrap()
            .page_tree
            .is_empty()
    );
    for id in [first, 999_999] {
        let mut invalid = project.clone();
        invalid.page_tree.push(PageTreeNode::Page {
            page_id: id,
            children: vec![],
        });
        assert!(invalid.validate().is_err());
    }
    let mut invalid = project.clone();
    invalid.page_tree = vec![PageTreeNode::Page {
        page_id: first,
        children: vec![],
    }];
    assert!(invalid.validate().is_err());
    let mut invalid = project;
    invalid.page_tree.push(PageTreeNode::Folder {
        id: "folder-a".into(),
        name: "重名 ID".into(),
        children: vec![],
    });
    assert!(invalid.validate().is_err());
}

#[test]
fn rotation_roundtrips_and_older_widgets_default_to_zero() {
    let mut widget = Widget::new(2, WidgetKind::Line, 20.0, 30.0);
    widget.rotation = -37.5;
    let mut encoded = serde_json::to_value(&widget).unwrap();
    assert_eq!(
        serde_json::from_value::<Widget>(encoded.clone()).unwrap(),
        widget
    );
    encoded.as_object_mut().unwrap().remove("rotation");
    assert_eq!(
        serde_json::from_value::<Widget>(encoded).unwrap().rotation,
        0.0
    );
}

#[test]
fn rejects_non_finite_or_excessive_rotation() {
    let mut project = Project::blank();
    let id = project.allocate_id();
    project.pages[0]
        .widgets
        .push(Widget::new(id, WidgetKind::Line, 0.0, 0.0));
    for rotation in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        36_001.0,
        -36_001.0,
    ] {
        project.pages[0].widgets[0].rotation = rotation;
        assert!(project.validate().is_err());
    }
    project.pages[0].widgets[0].rotation = 36_000.0;
    project.validate().unwrap();
}

#[test]
fn sample_roundtrips_and_references_are_valid() {
    let sample = Project::default();
    sample.validate().unwrap();
    assert_eq!(sample.pages.len(), 2);
    let encoded = serde_json::to_vec(&sample).unwrap();
    let decoded: Project = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(sample, decoded);
    assert!(
        sample
            .pages
            .iter()
            .flat_map(|p| &p.widgets)
            .any(|w| w.interaction.action == Action::Navigate)
    );
}

#[test]
fn rejects_nan_duplicates_invalid_targets_and_css_injection() {
    let mut project = Project::blank();
    let id = project.allocate_id();
    project.pages[0]
        .widgets
        .push(Widget::new(id, WidgetKind::Button, 0.0, 0.0));
    project.pages[0].widgets[0].x = f32::NAN;
    assert!(project.validate().is_err());
    project.pages[0].widgets[0].x = 0.0;
    project.pages[0].widgets[0].fill = "red;background:url(https://example.invalid)".into();
    assert!(project.validate().is_err());
    project.pages[0].widgets[0].fill = "#123456".into();
    project.pages[0].widgets[0].interaction.action = Action::Navigate;
    project.pages[0].widgets[0].interaction.target_page = Some(999);
    assert!(project.validate().is_err());
    project.pages[0].widgets[0].interaction = Interaction::default();
    project.pages[0].widgets[0].id = 1;
    assert!(project.validate().is_err());
}

#[test]
fn save_overwrites_without_corrupting_existing_document_on_validation_error() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "rpshop-model-{}-{nonce}.ingenaro",
        std::process::id()
    ));
    let mut project = Project::blank();
    project.save(&path).unwrap();
    project.name = "保存后的项目".into();
    project.save(&path).unwrap();
    assert_eq!(Project::load(&path).unwrap(), project);
    project.pages[0].width = f32::INFINITY;
    assert!(project.save(&path).is_err());
    assert_eq!(Project::load(&path).unwrap().name, "保存后的项目");
    fs::remove_file(path).unwrap();
}

#[test]
fn rejects_executable_image_uris() {
    let mut project = Project::blank();
    let id = project.allocate_id();
    let mut widget = Widget::new(id, WidgetKind::Image, 0.0, 0.0);
    widget.image_data = Some("data:image/svg+xml,<svg onload=alert(1)>".into());
    project.pages[0].widgets.push(widget);
    assert!(project.validate().is_err());
}
