use super::*;

#[test]
fn native_order_supports_root_siblings_beside_layers_and_tables() {
    let (mut parts, before, bindings) = order_fixture();
    let package = parts[&10].package.clone();
    let table_before = parts[&10]
        .node(&format!("{:032x}", 104))
        .unwrap()
        .properties
        .clone();
    let after = reordered(&before, &[4, 2, 3, 1, 5, 6, 7]);
    patch_native_order(
        &mut parts,
        &package,
        &after,
        &before,
        &bindings,
        &BTreeMap::new(),
    )
    .unwrap();
    let diagram = &parts[&10]
        .node(&format!("{:032x}", 102))
        .unwrap()
        .properties;
    let order = array(&diagram["diagram-objects"])
        .iter()
        .map(|v| reference_id(v).unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(order, [4, 103, 1, 104, 7].map(|n| format!("{n:032x}")));
    assert_eq!(diagram["unrecognized-setting"]["keep"], 42);
    assert_eq!(diagram["diagram-objects"][0]["$token"], 3);
    assert_eq!(
        parts[&10]
            .node(&format!("{:032x}", 104))
            .unwrap()
            .properties,
        table_before
    );
    let bytes = binary::encode(&parts[&10]).unwrap();
    assert_eq!(
        binary::encode(&binary::parse(&bytes).unwrap()).unwrap(),
        bytes
    );
}

#[test]
fn native_order_reorders_layer_siblings_without_changing_root_refs() {
    let (mut parts, before, bindings) = order_fixture();
    let package = parts[&10].package.clone();
    let root_before = parts[&10]
        .node(&format!("{:032x}", 102))
        .unwrap()
        .properties
        .clone();
    let after = reordered(&before, &[1, 3, 2, 4, 5, 6, 7]);
    patch_native_order(
        &mut parts,
        &package,
        &after,
        &before,
        &bindings,
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(
        parts[&10]
            .node(&format!("{:032x}", 102))
            .unwrap()
            .properties,
        root_before
    );
    let group = &parts[&10]
        .node(&format!("{:032x}", 103))
        .unwrap()
        .properties;
    assert_eq!(
        reference_id(&group["DiagramObjects"][0]),
        Some(format!("{:032x}", 3).as_str())
    );
    assert_eq!(group["unknown-group-data"], 17);
}

#[test]
fn native_order_rejects_cross_parent_and_table_internal_changes_atomically() {
    for ids in [[2, 1, 3, 4, 5, 6, 7], [1, 2, 3, 4, 6, 5, 7]] {
        let (mut parts, before, bindings) = order_fixture();
        let original = binary::encode(&parts[&10]).unwrap();
        let package = parts[&10].package.clone();
        let after = reordered(&before, &ids);
        assert!(
            patch_native_order(
                &mut parts,
                &package,
                &after,
                &before,
                &bindings,
                &BTreeMap::new()
            )
            .is_err()
        );
        assert_eq!(binary::encode(&parts[&10]).unwrap(), original);
    }
}

#[test]
fn native_groups_wrap_existing_layers_and_preserve_opaque_tables() {
    let (mut parts, before, bindings, groups) = grouped_order_fixture();
    let package = parts[&10].package.clone();
    let table = parts[&10]
        .node(&format!("{:032x}", 104))
        .unwrap()
        .properties
        .clone();
    let mut after = before.clone();
    for widget in &mut after.widgets {
        if matches!(widget.id, 1..=3) {
            widget.groups.insert(0, 301);
        }
    }
    crate::grouping::rename_group(&mut after, 301, "页面导航").unwrap();
    patch_native_groups(
        &mut parts,
        &package,
        &after,
        &before,
        &bindings,
        &BTreeMap::new(),
        &groups,
    )
    .unwrap();
    patch_native_order(
        &mut parts,
        &package,
        &after,
        &before,
        &bindings,
        &BTreeMap::new(),
    )
    .unwrap();
    let existing_group = raw_id(&groups[&300].object);
    patch_native_group_name(parts.get_mut(&10).unwrap(), existing_group, "组内元素").unwrap();
    let graph = &parts[&10];
    let root = array(&graph.node(&format!("{:032x}", 102)).unwrap().properties["diagram-objects"]);
    let group = graph.resolve(&root[0]).unwrap();
    assert!(group.kind.ends_with(":Layer"));
    assert_eq!(group.properties["name-block"]["text"], "页面导航");
    assert_eq!(
        reference_id(&group.properties["DiagramObjects"][0]),
        Some(format!("{:032x}", 1).as_str())
    );
    assert_eq!(
        reference_id(&group.properties["DiagramObjects"][1]),
        Some(format!("{:032x}", 103).as_str())
    );
    assert_eq!(
        graph.node(&format!("{:032x}", 104)).unwrap().properties,
        table
    );
    assert_eq!(
        graph.node(&format!("{:032x}", 103)).unwrap().properties["unknown-group-data"],
        17
    );
    let bytes = binary::encode(graph).unwrap();
    let reopened = binary::parse(&bytes).unwrap();
    assert_eq!(binary::encode(&reopened).unwrap(), bytes);
    let roots = BTreeMap::from([(package.clone(), format!("{:032x}", 101))]);
    let styles = Styles::new(&reopened);
    let mut converter = Converter {
        graph: &reopened,
        roots: &roots,
        styles: &styles,
        next_id: 1,
        warnings: BTreeMap::new(),
        pending_links: Vec::new(),
        bindings: BTreeMap::new(),
        preview_parents: BTreeMap::new(),
        group_bindings: BTreeMap::new(),
    };
    let imported = converter.page(&package, "Roundtrip").unwrap();
    let groups = &imported.widgets[1].groups;
    assert_eq!(groups.len(), 2);
    assert_eq!(imported.group_name(groups[0]), "页面导航");
    assert_eq!(imported.group_name(groups[1]), "组内元素");
}

#[test]
fn native_ungroup_expands_refs_without_editing_child_geometry_or_unknown_group_data() {
    let (mut parts, before, bindings, groups) = grouped_order_fixture();
    let package = parts[&10].package.clone();
    let old_group = parts[&10]
        .node(&format!("{:032x}", 103))
        .unwrap()
        .properties
        .clone();
    let old_child = parts[&10]
        .node(&format!("{:032x}", 2))
        .unwrap()
        .properties
        .clone();
    let mut after = before.clone();
    for widget in &mut after.widgets {
        widget.groups.clear();
    }
    patch_native_groups(
        &mut parts,
        &package,
        &after,
        &before,
        &bindings,
        &BTreeMap::new(),
        &groups,
    )
    .unwrap();
    patch_native_order(
        &mut parts,
        &package,
        &after,
        &before,
        &bindings,
        &BTreeMap::new(),
    )
    .unwrap();
    let root = &parts[&10]
        .node(&format!("{:032x}", 102))
        .unwrap()
        .properties["diagram-objects"];
    assert_eq!(reference_id(&root[1]), Some(format!("{:032x}", 2).as_str()));
    assert_eq!(reference_id(&root[2]), Some(format!("{:032x}", 3).as_str()));
    assert_eq!(
        parts[&10]
            .node(&format!("{:032x}", 103))
            .unwrap()
            .properties,
        old_group
    );
    assert_eq!(
        parts[&10].node(&format!("{:032x}", 2)).unwrap().properties,
        old_child
    );
}

#[test]
fn native_groups_reject_extracting_one_table_cell_without_mutation() {
    let (mut parts, before, bindings, groups) = grouped_order_fixture();
    let original = binary::encode(&parts[&10]).unwrap();
    let package = parts[&10].package.clone();
    let mut after = before.clone();
    after.widgets.iter_mut().find(|w| w.id == 5).unwrap().groups = vec![301];
    assert!(
        patch_native_groups(
            &mut parts,
            &package,
            &after,
            &before,
            &bindings,
            &BTreeMap::new(),
            &groups
        )
        .is_err()
    );
    assert_eq!(binary::encode(&parts[&10]).unwrap(), original);
}
