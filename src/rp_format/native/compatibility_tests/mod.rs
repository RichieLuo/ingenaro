mod file_roundtrip;

mod behavior;

mod groups_and_order;

mod visuals;

mod forms;

use super::*;

fn order_fixture() -> (BTreeMap<u64, Graph>, Page, BTreeMap<u64, Binding>) {
    let id = |value: u64| format!("{value:032x}");
    let package = id(100);
    let nodes = [
        (101, "Axure:Page", json!({"diagram":{"$ref":id(102)}})),
        (
            102,
            "Axure:Diagram",
            json!({"diagram-objects":[
            {"$ref":id(1),"$token":3},{"$ref":id(103),"$token":3},
            {"$ref":id(4),"$token":3},{"$ref":id(104),"$token":3},{"$ref":id(7),"$token":3}],
            "unrecognized-setting":{"keep":42}}),
        ),
        (
            103,
            "Axure:DiagramObject:Layer",
            json!({"DiagramObjects":[{"$ref":id(2),"$token":3},{"$ref":id(3),"$token":3}],"unknown-group-data":17}),
        ),
        (
            104,
            "Axure:DiagramObject:Table",
            json!({"diagram-objects":[{"$ref":id(5)},{"$ref":id(6)}],"cells":[{"$ref":id(5)},{"$ref":id(6)}]}),
        ),
    ];
    let mut objects = nodes
        .into_iter()
        .map(|(i, kind, properties)| {
            (
                id(i),
                Node {
                    kind: kind.into(),
                    properties,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    for number in 1..=7 {
        objects.insert(
            id(number),
            Node {
                kind: "Axure:DiagramObject:VectorShape".into(),
                properties: json!({"Shape":0}),
            },
        );
    }
    let graph = Graph::new([11, 0, 0, 0], id(101), package.clone(), objects);
    let page = Page {
        id: 200,
        name: "Order fixture".into(),
        width: 1000.0,
        height: 1000.0,
        canvas: crate::model::CanvasMode::Fixed,
        responsive_min_width: None,
        background: "#FFFFFF".into(),
        widgets: (1..=7)
            .map(|i| Widget::new(i, WidgetKind::Rectangle, 0.0, 0.0))
            .collect(),
        group_names: BTreeMap::new(),
    };
    let bindings = (1..=7)
        .map(|i| {
            (
                i,
                Binding {
                    object: format!("{package}/{}", id(i)),
                    scale: 1.0,
                },
            )
        })
        .collect();
    (BTreeMap::from([(10, graph)]), page, bindings)
}

fn reordered(page: &Page, ids: &[u64]) -> Page {
    let mut result = page.clone();
    result.widgets = ids
        .iter()
        .map(|id| page.widgets.iter().find(|w| w.id == *id).unwrap().clone())
        .collect();
    result
}

type GroupedOrderFixture = (
    BTreeMap<u64, Graph>,
    Page,
    BTreeMap<u64, Binding>,
    BTreeMap<u64, GroupBinding>,
);
fn grouped_order_fixture() -> GroupedOrderFixture {
    let (parts, mut page, bindings) = order_fixture();
    for widget in &mut page.widgets {
        if matches!(widget.id, 2 | 3) {
            widget.groups = vec![300];
        }
    }
    let groups = BTreeMap::from([(
        300,
        GroupBinding {
            object: format!("{}/{:032x}", parts[&10].package, 103),
        },
    )]);
    (parts, page, bindings, groups)
}

fn close(left: f32, right: f32) {
    assert!((left - right).abs() < 0.001, "{left} != {right}");
}

fn check_native_fixture(variable: &str, legacy: bool) {
    let Some(path) = std::env::var_os(variable) else {
        return;
    };
    let path = PathBuf::from(path);
    let original = std::fs::read(&path).expect("read caller-selected fixture");
    let directory = std::env::temp_dir().join(format!("rpshop-compatibility-{}", guid()));
    std::fs::create_dir(&directory).unwrap();
    let destination = directory.join("roundtrip.rp");
    let mut native = super::super::open(&path).expect("open caller-selected fixture");
    for (offset, graph) in &native.parts {
        assert_eq!(
            binary::encode(graph).unwrap(),
            native.source.part(*offset).unwrap(),
            "every original object package must roundtrip byte-for-byte"
        );
    }
    if legacy {
        assert_eq!(
            native.parts.len(),
            37,
            "fixture object package count changed"
        );
    }
    native.save(&native.project.clone(), &destination).unwrap();
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        original,
        "unchanged save must be byte exact"
    );

    let mut edited = native.project.clone();
    let (page_index, widget_index) = edited
        .pages
        .iter()
        .enumerate()
        .find_map(|(pi, page)| {
            page.widgets.iter().enumerate().find_map(|(wi, widget)| {
                let binding = native.bindings.get(&widget.id)?;
                let source =
                    get_node(&native.parts, &owners(&native.parts), &binding.object).ok()?;
                let has_text = !widget.text.is_empty();
                let has_legacy_child = contained_text_id(
                    native
                        .parts
                        .get(owners(&native.parts).get(&binding.object)?)?,
                    source,
                )
                .is_some();
                (has_text && (!legacy || has_legacy_child)).then_some((pi, wi))
            })
        })
        .expect("fixture must contain an editable text widget");
    let widget = &mut edited.pages[page_index].widgets[widget_index];
    let id = widget.id;
    widget.text = "Ingenaro 保存验证\n第二行".into();
    widget.font_size = 22.0;
    widget.rotation = 23.5;
    native.save(&edited, &destination).unwrap();
    let reopened = super::super::open(&destination).unwrap();
    let actual = reopened
        .project
        .pages
        .iter()
        .flat_map(|p| &p.widgets)
        .find(|w| w.id == id)
        .unwrap();
    assert_eq!(actual.text, "Ingenaro 保存验证\n第二行");
    assert!((actual.font_size - 22.0).abs() < 0.001);
    assert!((actual.rotation - 23.5).abs() < 0.001);
    if legacy {
        let binding = &reopened.bindings[&id];
        let own = owners(&reopened.parts);
        let graph = &reopened.parts[&own[&binding.object]];
        let node = graph.node(raw_id(&binding.object)).unwrap();
        let text_id = contained_text_id(graph, node).unwrap();
        assert!(node.properties.get("RichTextBlock").is_none());
        assert_eq!(
            widget_text(&graph.node(&text_id).unwrap().properties),
            actual.text
        );
        if let Some(preview) = reopened
            .project
            .pages
            .iter()
            .flat_map(|p| &p.widgets)
            .find(|w| !reopened.is_editable_widget(w.id))
        {
            let mut invalid = reopened.project.clone();
            invalid
                .pages
                .iter_mut()
                .flat_map(|p| &mut p.widgets)
                .find(|w| w.id == preview.id)
                .unwrap()
                .text
                .push('!');
            let mut document = super::super::open(&destination).unwrap();
            assert!(
                document
                    .save(&invalid, &directory.join("rejected.rp"))
                    .is_err(),
                "master preview edits must never write through to a shared master"
            );
        }
        if let Some((&child_id, &parent_id)) = native.preview_parents.iter().next() {
            let before = native.project.clone();
            let mut moved = before.clone();
            let parent = moved
                .pages
                .iter_mut()
                .flat_map(|p| &mut p.widgets)
                .find(|w| w.id == parent_id)
                .unwrap();
            parent.x += 37.0;
            parent.y += 19.0;
            parent.rotation += 30.0;
            native.refresh_previews(&mut moved, &before);
            let preview = moved
                .pages
                .iter()
                .flat_map(|p| &p.widgets)
                .find(|w| w.id == child_id)
                .unwrap()
                .clone();
            native.refresh_previews(&mut moved, &before);
            assert_eq!(
                moved
                    .pages
                    .iter()
                    .flat_map(|p| &p.widgets)
                    .find(|w| w.id == child_id)
                    .unwrap(),
                &preview,
                "drag refresh must always start from drag.before, without cumulative transforms"
            );
            let master_bytes = native
                .parts
                .iter()
                .filter(|(_, graph)| graph.root().is_some_and(|node| node.kind == "Axure:Master"))
                .map(|(offset, graph)| (*offset, binary::encode(graph).unwrap()))
                .collect::<BTreeMap<_, _>>();
            native.save(&moved, &destination).unwrap();
            let saved = native
                .project
                .pages
                .iter()
                .flat_map(|p| &p.widgets)
                .find(|w| w.id == child_id)
                .unwrap();
            close(saved.x, preview.x);
            close(saved.y, preview.y);
            close(saved.rotation, preview.rotation);
            for (offset, bytes) in master_bytes {
                assert_eq!(
                    binary::encode(&native.parts[&offset]).unwrap(),
                    bytes,
                    "moving a master instance must never change its shared definition"
                );
            }
        }
        let mut expected_texts = native
            .project
            .pages
            .iter()
            .flat_map(|p| &p.widgets)
            .map(|w| w.text.clone())
            .collect::<Vec<_>>();
        let mut added_project = native.project.clone();
        let added_id = added_project.allocate_id();
        let mut added = Widget::new(added_id, WidgetKind::Rectangle, 120.0, 160.0);
        added.text = "Ingenaro 独立新增验证".into();
        added.font_size = 18.0;
        added.rotation = 12.0;
        expected_texts.push(added.text.clone());
        added_project.pages[page_index].widgets.push(added);
        native.save(&added_project, &destination).unwrap();
        let mut actual_texts = native
            .project
            .pages
            .iter()
            .flat_map(|p| &p.widgets)
            .map(|w| w.text.clone())
            .collect::<Vec<_>>();
        expected_texts.sort();
        actual_texts.sort();
        assert_eq!(
            actual_texts, expected_texts,
            "new legacy text objects must not share a caption with their template"
        );
        let added = native
            .project
            .pages
            .iter()
            .flat_map(|p| &p.widgets)
            .find(|w| w.text == "Ingenaro 独立新增验证")
            .unwrap();
        assert!((added.font_size - 18.0).abs() < 0.001);
        assert!((added.rotation - 12.0).abs() < 0.001);
    }
    let pair = native
        .project
        .pages
        .iter()
        .enumerate()
        .find_map(|(page_index, page)| {
            let package = native.pages.get(&page.id)?;
            let graph = native
                .parts
                .values()
                .find(|graph| graph.package == *package)?;
            let diagram = graph
                .root()?
                .properties
                .get("diagram")
                .and_then(reference_id)?;
            let root_ids = array(&graph.node(diagram)?.properties["diagram-objects"])
                .iter()
                .filter_map(reference_id)
                .collect::<BTreeSet<_>>();
            page.widgets.windows(2).find_map(|pair| {
                let eligible = pair.iter().all(|widget| {
                    widget.groups.is_empty()
                        && native.bindings.get(&widget.id).is_some_and(|binding| {
                            binding.object.starts_with(&format!("{package}/"))
                                && root_ids.contains(raw_id(&binding.object))
                                && graph.node(raw_id(&binding.object)).is_some_and(|node| {
                                    matches!(
                                        node.kind.rsplit(':').next(),
                                        Some(
                                            "VectorShape"
                                                | "ImageBox"
                                                | "TextBox"
                                                | "TextArea"
                                                | "Checkbox"
                                        )
                                    )
                                })
                        })
                });
                eligible.then_some((page_index, pair[0].id, pair[1].id))
            })
        })
        .expect("fixture needs two adjacent ordinary root widgets for grouping");
    let mut grouped = native.project.clone();
    let group_id = grouped.allocate_id();
    for widget in &mut grouped.pages[pair.0].widgets {
        if widget.id == pair.1 {
            widget.name = "Ingenaro group member one".into();
            widget.groups = vec![group_id];
        }
        if widget.id == pair.2 {
            widget.name = "Ingenaro group member two".into();
            widget.groups = vec![group_id];
        }
    }
    crate::grouping::rename_group(&mut grouped.pages[pair.0], group_id, "Ingenaro named group")
        .unwrap();
    native.save(&grouped, &destination).unwrap();
    let members = native
        .project
        .pages
        .iter()
        .flat_map(|page| &page.widgets)
        .filter(|widget| {
            matches!(
                widget.name.as_str(),
                "Ingenaro group member one" | "Ingenaro group member two"
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0].groups.len(), 1);
    assert_eq!(members[0].groups, members[1].groups);
    let group_id = members[0].groups[0];
    assert!(native.group_bindings.contains_key(&group_id));
    let mut renamed = native.project.clone();
    assert_eq!(
        renamed.pages[pair.0].group_name(group_id),
        "Ingenaro named group"
    );
    crate::grouping::rename_group(&mut renamed.pages[pair.0], group_id, "Ingenaro renamed group")
        .unwrap();
    native.save(&renamed, &destination).unwrap();
    assert!(
        native.project.pages[pair.0]
            .group_names
            .values()
            .any(|name| name == "Ingenaro renamed group")
    );
    let mut ungrouped = native.project.clone();
    for widget in ungrouped
        .pages
        .iter_mut()
        .flat_map(|page| &mut page.widgets)
    {
        if matches!(
            widget.name.as_str(),
            "Ingenaro group member one" | "Ingenaro group member two"
        ) {
            widget.groups.clear();
        }
    }
    ungrouped.pages[pair.0].prune_group_names();
    native.save(&ungrouped, &destination).unwrap();
    assert!(
        native
            .project
            .pages
            .iter()
            .flat_map(|page| &page.widgets)
            .filter(|widget| matches!(
                widget.name.as_str(),
                "Ingenaro group member one" | "Ingenaro group member two"
            ))
            .all(|widget| widget.groups.is_empty())
    );
    let target = native
        .project
        .pages
        .iter()
        .flat_map(|page| &page.widgets)
        .find(|widget| {
            native.is_editable_widget(widget.id)
                && widget.vector_points.is_some()
                && widget.kind == WidgetKind::Image
                && widget.image_data.is_some()
        })
        .or_else(|| {
            native
                .project
                .pages
                .iter()
                .flat_map(|page| &page.widgets)
                .find(|widget| {
                    native.is_editable_widget(widget.id) && widget.vector_points.is_some()
                })
        })
        .expect("fixture needs an editable native vector path")
        .id;
    let mut mirrored = native.project.clone();
    let widget = mirrored
        .pages
        .iter_mut()
        .flat_map(|page| &mut page.widgets)
        .find(|widget| widget.id == target)
        .unwrap();
    crate::vector_edit::flip_widget(widget, true).unwrap();
    crate::vector_edit::flip_widget(widget, false).unwrap();
    let expected_points = widget.vector_points.clone().unwrap();
    let expected_rotation = widget.rotation;
    native.save(&mirrored, &destination).unwrap();
    let widget = native
        .project
        .pages
        .iter()
        .flat_map(|page| &page.widgets)
        .find(|widget| widget.id == target)
        .unwrap();
    assert_eq!(widget.vector_points.as_ref(), Some(&expected_points));
    close(widget.rotation, expected_rotation);
    assert!(
        !native.is_bitmap_widget(target),
        "vector previews must keep their native vector identity"
    );
    let binding = &native.bindings[&target];
    let native_node = get_node(&native.parts, &owners(&native.parts), &binding.object).unwrap();
    assert!(native_node.kind.ends_with(":VectorShape"));
    assert_eq!(native_node.properties["ControlPoints"], expected_points);
    let state = native.session_state();
    let mut restored = super::super::open(&destination).unwrap();
    let mut legacy_state = state.clone();
    legacy_state["version"] = json!(1);
    assert!(restored.restore_session_state(&legacy_state).is_err());
    let mut incomplete_state = state.clone();
    incomplete_state
        .as_object_mut()
        .unwrap()
        .remove("preview_parents");
    assert!(restored.restore_session_state(&incomplete_state).is_err());
    restored.restore_session_state(&state).unwrap();
    assert_eq!(restored.project, native.project);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        original,
        "source fixture must stay untouched"
    );
    std::fs::remove_dir_all(directory).unwrap();
}
