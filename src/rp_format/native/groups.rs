use super::*;

pub(super) fn collect_group_atoms(
    projection: &NativeOrderProjection<'_>,
    offset: u64,
    references: &[Value],
    metadata: &mut NativeGroupMetadata,
    path: &[u64],
    atoms: &mut Vec<NativeGroupAtom>,
    depth: usize,
) -> Result<()> {
    ensure!(depth <= 32, "原生组合嵌套超过限制");
    let graph = &projection.parts[&offset];
    for reference in references {
        let id = reference_id(reference).context("原生组合包含无法识别的引用")?;
        let node = graph.node(id).context("原生组合对象不存在")?;
        if node.kind.ends_with(":Layer") {
            let group = *metadata
                .layers
                .get(&format!("{}/{id}", graph.package))
                .context("原生组缺少编辑器绑定")?;
            metadata.references.insert(group, reference.clone());
            let mut nested = path.to_vec();
            nested.push(group);
            let start = atoms.len();
            collect_group_atoms(
                projection,
                offset,
                array(&node.properties["DiagramObjects"]),
                metadata,
                &nested,
                atoms,
                depth + 1,
            )?;
            if atoms[start..].iter().any(|atom| !atom.widgets.is_empty()) {
                continue;
            }
            // An entirely invisible subtree is one opaque source object. Its
            // descendants must stay attached even when unrelated widgets group.
            atoms.truncate(start);
        }
        atoms.push(NativeGroupAtom {
            reference: reference.clone(),
            groups: path.to_vec(),
            widgets: projection.object(
                offset,
                id,
                &NativeListOrders::new(),
                &mut BTreeSet::new(),
                0,
            )?,
        });
    }
    Ok(())
}

pub(super) fn append_group_entry(
    entries: &mut Vec<NativeGroupEntry>,
    path: &[u64],
    reference: Value,
) {
    if let Some((&group, rest)) = path.split_first() {
        if !matches!(entries.last(), Some(NativeGroupEntry::Group(id,_)) if *id == group) {
            entries.push(NativeGroupEntry::Group(group, Vec::new()));
        }
        let Some(NativeGroupEntry::Group(_, children)) = entries.last_mut() else {
            unreachable!();
        };
        append_group_entry(children, rest, reference);
    } else {
        entries.push(NativeGroupEntry::Object(reference));
    }
}

pub(super) fn group_entry_contains(
    entry: &NativeGroupEntry,
    id: &str,
    references: &BTreeMap<u64, Value>,
) -> bool {
    match entry {
        NativeGroupEntry::Object(reference) => reference_id(reference) == Some(id),
        NativeGroupEntry::Group(group, children) => {
            references.get(group).and_then(reference_id) == Some(id)
                || children
                    .iter()
                    .any(|child| group_entry_contains(child, id, references))
        }
    }
}

pub(super) fn group_entries_mut(
    entries: &mut Vec<NativeGroupEntry>,
    group: u64,
) -> Option<&mut Vec<NativeGroupEntry>> {
    for entry in entries {
        if let NativeGroupEntry::Group(id, children) = entry {
            if *id == group {
                return Some(children);
            }
            if let Some(found) = group_entries_mut(children, group) {
                return Some(found);
            }
        }
    }
    None
}

pub(super) fn preserve_unmapped_group_atoms(
    graph: &Graph,
    direct: &[Value],
    metadata: &NativeGroupMetadata,
    atoms: &[NativeGroupAtom],
    entries: &mut Vec<NativeGroupEntry>,
) -> Result<()> {
    for atom in atoms.iter().filter(|atom| atom.widgets.is_empty()) {
        let siblings = if let Some(parent) = atom.groups.last() {
            let parent = metadata
                .references
                .get(parent)
                .and_then(reference_id)
                .and_then(|id| graph.node(id))
                .context("无法映射的对象缺少原生父组合")?;
            array(&parent.properties["DiagramObjects"])
        } else {
            direct
        };
        let id = reference_id(&atom.reference).context("原生未知对象引用无效")?;
        let source_position = siblings
            .iter()
            .position(|reference| reference_id(reference) == Some(id))
            .context("无法映射的对象不在原生父组合中")?;
        let target = if let Some(parent) = atom.groups.last() {
            group_entries_mut(entries, *parent)
                .context("原生组合含有空组合或未知对象，无法安全解除或移出该父组合")?
        } else {
            &mut *entries
        };
        let next = siblings[source_position + 1..]
            .iter()
            .filter_map(reference_id)
            .find_map(|id| {
                target
                    .iter()
                    .position(|entry| group_entry_contains(entry, id, &metadata.references))
            });
        let position = next.unwrap_or(target.len());
        target.insert(position, NativeGroupEntry::Object(atom.reference.clone()));
    }
    Ok(())
}

pub(super) fn new_native_group(
    graph: &mut Graph,
    diagram: &str,
    children: Vec<Value>,
    name: &str,
) -> Result<String> {
    let id = guid();
    let style_id = guid();
    let template = graph
        .objects
        .iter()
        .find(|(_, node)| node.kind.ends_with(":Layer"))
        .map(|(id, node)| (id.clone(), node.clone()));
    if let Some((template_id, template)) = &template {
        let source = graph.clone();
        graph.copy_node_from(&source, template_id, &id)?;
        let style = template.properties.get("MyStyle").and_then(reference_id);
        if let Some(style) = style.filter(|style| source.node(style).is_some()) {
            graph.copy_node_from(&source, style, &style_id)?;
        }
    }
    let node = graph.objects.entry(id.clone()).or_insert_with(|| Node {
        kind: "Axure:DiagramObject:Layer".into(),
        properties: json!({}),
    });
    if let Some((template_id, _)) = template {
        replace_reference(&mut node.properties, &template_id, &id);
    }
    node.properties["DiagramObjects"] = Value::Array(children);
    node.properties["name-block"] = json!({"*type":"text-block","text":name,"property-dictionary":{
        "type":"label","diagram-object-id":{"$guid":id},"parent-diagram-id":{"$guid":diagram}}});
    node.properties["MyStyle"] = json!({"$ref":style_id,"$token":3});
    node.properties["Annotation"] = json!({"$ref":EMPTY_ID,"$token":3});
    node.properties["AdaptiveStyles"] = json!([]);
    node.properties["SubmitButton"] = json!({"$ref":EMPTY_ID,"$token":3});
    node.properties["SelectedStyleGroup"] = json!("");
    node.properties["tooltip"] = json!("");
    for field in ["IsLocked", "IsContained", "Disabled", "Selected"] {
        node.properties[field] = json!(false);
    }
    node.properties["VisibleFromSettings"] = json!(true);
    node.properties["Propagate"] = json!(true);
    node.properties
        .as_object_mut()
        .unwrap()
        .remove("InteractionMap");
    let style = graph
        .objects
        .entry(style_id.clone())
        .or_insert_with(|| Node {
            kind: "Axure:Style".into(),
            properties: json!({"Capabilities":0,"All":false,"style-name":""}),
        });
    style.properties["Id"] = json!({"$guid":style_id});
    style.properties["PropList"] = json!([]);
    Ok(id)
}

pub(super) fn write_group_entries(
    graph: &mut Graph,
    diagram: &str,
    entries: &[NativeGroupEntry],
    groups: &BTreeMap<u64, GroupBinding>,
    references: &BTreeMap<u64, Value>,
    page: &Page,
) -> Result<Vec<Value>> {
    let mut result = Vec::with_capacity(entries.len());
    for entry in entries {
        match entry {
            NativeGroupEntry::Object(reference) => result.push(reference.clone()),
            NativeGroupEntry::Group(group, children) => {
                let children =
                    write_group_entries(graph, diagram, children, groups, references, page)?;
                if let Some(binding) = groups.get(group) {
                    let node = graph
                        .objects
                        .get_mut(raw_id(&binding.object))
                        .context("原生组合绑定无效")?;
                    node.properties["DiagramObjects"] = Value::Array(children);
                    result.push(references.get(group).context("原生组父引用丢失")?.clone());
                } else {
                    let id = new_native_group(graph, diagram, children, &page.group_name(*group))?;
                    result.push(json!({"$ref":id,"$token":3}));
                }
            }
        }
    }
    Ok(result)
}

pub(super) fn patch_native_group_name(graph: &mut Graph, object: &str, name: &str) -> Result<()> {
    let node = graph.objects.get_mut(object).context("原生分组对象丢失")?;
    let label = &mut node.properties["name-block"];
    if label.is_null() {
        *label = json!({"*type":"text-block","text":name,"property-dictionary":{
            "type":"label","diagram-object-id":{"$guid":object}
        }});
    } else {
        ensure!(label.is_object(), "无法识别原生分组名称结构");
        label["text"] = json!(name);
    }
    Ok(())
}

pub(super) fn patch_native_groups(
    parts: &mut BTreeMap<u64, Graph>,
    package: &str,
    page: &Page,
    before: &Page,
    bindings: &BTreeMap<u64, Binding>,
    preview_parents: &BTreeMap<u64, u64>,
    groups: &BTreeMap<u64, GroupBinding>,
) -> Result<()> {
    let projection = native_order_projection(parts, page, before, bindings, preview_parents)?;
    let offset = *projection
        .packages
        .get(package)
        .context("组合页面部件不存在")?;
    let graph = &parts[&offset];
    let diagram = graph
        .root()
        .and_then(|root| root.properties.get("diagram"))
        .and_then(reference_id)
        .context("组合页面画布不存在")?
        .to_owned();
    let mut metadata = NativeGroupMetadata {
        layers: BTreeMap::new(),
        references: BTreeMap::new(),
    };
    for (group, binding) in groups {
        if binding.object.starts_with(&format!("{package}/")) {
            ensure!(
                metadata
                    .layers
                    .insert(binding.object.clone(), *group)
                    .is_none(),
                "原生组合映射不唯一"
            );
        }
    }
    let mut atoms = Vec::new();
    let original_registry = array(&graph.objects[&diagram].properties["diagram-objects"]).to_vec();
    let members_before = layer_registry_members(graph, &original_registry)?;
    let direct = direct_diagram_references(graph, &original_registry)?;
    collect_group_atoms(
        &projection,
        offset,
        &direct,
        &mut metadata,
        &[],
        &mut atoms,
        0,
    )?;
    let allowed = metadata.references.keys().copied().collect::<BTreeSet<_>>();
    let previous = before
        .widgets
        .iter()
        .map(|widget| (widget.id, widget))
        .collect::<BTreeMap<_, _>>();
    let current = page
        .widgets
        .iter()
        .enumerate()
        .map(|(index, widget)| (widget.id, (index, widget)))
        .collect::<BTreeMap<_, _>>();
    let mut positioned = Vec::with_capacity(atoms.len());
    for atom in atoms.iter().filter(|atom| !atom.widgets.is_empty()) {
        let mut target_path: Option<Vec<u64>> = None;
        let mut positions = Vec::new();
        for id in &atom.widgets {
            let (position, widget) = current.get(id).context("组合操作跨越了原生父容器")?;
            positions.push(*position);
            let suffix = previous
                .get(id)
                .map(|old| old.groups.strip_prefix(atom.groups.as_slice()))
                .unwrap_or(Some(&[]))
                .context("原生组合层级与编辑器不一致")?;
            ensure!(
                widget.groups.ends_with(suffix),
                "暂不支持跨表格、母版或动态面板状态改变组合结构"
            );
            let path = widget.groups[..widget.groups.len() - suffix.len()].to_vec();
            ensure!(
                target_path.as_ref().is_none_or(|target| *target == path),
                "暂不支持拆分表格、母版或动态面板状态中的部分元件进行组合"
            );
            target_path = Some(path);
        }
        let first = *positions.iter().min().unwrap();
        let last = *positions.iter().max().unwrap();
        ensure!(
            last - first + 1 == atom.widgets.len(),
            "组合操作会拆散原生复杂容器，未修改源文件"
        );
        let path = target_path.unwrap();
        ensure!(
            path.iter()
                .all(|id| !groups.contains_key(id) || allowed.contains(id)),
            "暂不支持跨动态面板状态或页面移动原生组合"
        );
        positioned.push((first, path, atom.reference.clone()));
    }
    positioned.sort_by_key(|(position, _, _)| *position);
    let mut entries = Vec::new();
    for (_, path, reference) in positioned {
        append_group_entry(&mut entries, &path, reference);
    }
    preserve_unmapped_group_atoms(graph, &direct, &metadata, &atoms, &mut entries)?;
    drop(projection);
    let graph = parts.get_mut(&offset).unwrap();
    let references = write_group_entries(
        graph,
        &diagram,
        &entries,
        groups,
        &metadata.references,
        page,
    )?;
    let references =
        rebuild_diagram_registry(graph, &original_registry, &members_before, &references)?;
    graph.objects.get_mut(&diagram).unwrap().properties["diagram-objects"] =
        Value::Array(references);
    Ok(())
}
