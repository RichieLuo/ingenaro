use super::*;

pub(super) fn remove_visible_reference(parts: &mut BTreeMap<u64, Graph>, id: &str) {
    for graph in parts.values_mut() {
        if !id.starts_with(&format!("{}/", graph.package)) {
            continue;
        }
        for node in graph.objects.values_mut() {
            for field in ["diagram-objects", "DiagramObjects"] {
                if let Some(list) = node.properties[field].as_array_mut() {
                    list.retain(|v| reference_id(v) != Some(raw_id(id)));
                }
            }
        }
    }
}

pub(super) fn set_kind(node: &mut Node, kind: WidgetKind) -> Result<()> {
    match kind {
        WidgetKind::Rectangle
        | WidgetKind::Text
        | WidgetKind::Button
        | WidgetKind::Ellipse
        | WidgetKind::Line => {
            node.kind = "Axure:DiagramObject:VectorShape".into();
            node.properties["Shape"] = json!(match kind {
                WidgetKind::Ellipse => 15,
                WidgetKind::Line => 29,
                _ => 0,
            });
            if !matches!(kind, WidgetKind::Ellipse | WidgetKind::Line) {
                let points = [0,1,2,3].map(|i|json!({
                    "Path":{"X":if i==1||i==2{1.0}else{0.0},"Y":if i>=2{1.0}else{0.0}},
                    "LowerHandle":{"X":if i==1||i==2{1.0}else{0.0},"Y":if i>=2{1.0}else{0.0}},
                    "HigherHandle":{"X":if i==1||i==2{1.0}else{0.0},"Y":if i>=2{1.0}else{0.0}},"HigherSegmentExists":true
                }));
                node.properties["ControlPoints"] = json!([points]);
            } else if kind == WidgetKind::Ellipse {
                let k = 0.2761423749153967;
                node.properties["ControlPoints"] = json!([[
                    {"Path":{"X":0.5,"Y":0.0},"LowerHandle":{"X":0.5-k,"Y":0.0},"HigherHandle":{"X":0.5+k,"Y":0.0},"HigherSegmentExists":true},
                    {"Path":{"X":1.0,"Y":0.5},"LowerHandle":{"X":1.0,"Y":0.5-k},"HigherHandle":{"X":1.0,"Y":0.5+k},"HigherSegmentExists":true},
                    {"Path":{"X":0.5,"Y":1.0},"LowerHandle":{"X":0.5+k,"Y":1.0},"HigherHandle":{"X":0.5-k,"Y":1.0},"HigherSegmentExists":true},
                    {"Path":{"X":0.0,"Y":0.5},"LowerHandle":{"X":0.0,"Y":0.5+k},"HigherHandle":{"X":0.0,"Y":0.5-k},"HigherSegmentExists":true}
                ]]);
            } else {
                node.properties["ControlPoints"] = json!([[
                    {"Path":{"X":0.0,"Y":0.5},"LowerHandle":{"X":0.0,"Y":0.5},"HigherHandle":{"X":0.0,"Y":0.5},"HigherSegmentExists":true},
                    {"Path":{"X":1.0,"Y":0.5},"LowerHandle":{"X":1.0,"Y":0.5},"HigherHandle":{"X":1.0,"Y":0.5},"HigherSegmentExists":false}
                ]]);
            }
        }
        WidgetKind::Image => node.kind = "Axure:DiagramObject:ImageBox".into(),
        WidgetKind::Input => {
            node.kind = "Axure:DiagramObject:TextBox".into();
            node.properties
                .as_object_mut()
                .unwrap()
                .remove("RichTextBlock");
            if !node.properties["TextBlock"].is_object() {
                node.properties["TextBlock"] = json!({"*type":"text-block","text":""});
            }
            node.properties["WordWrap"] = json!(false);
            node.properties["ReadOnly"] = json!(false);
            node.properties["PlaceholderText"] = json!("");
            node.properties["HideHintOnFocus"] = json!(false);
            node.properties["Type"] = json!(0);
            node.properties["MaxLength"] = json!(2147483647i32);
        }
        WidgetKind::Checkbox => node.kind = "Axure:DiagramObject:Checkbox".into(),
        WidgetKind::Radio | WidgetKind::TextArea | WidgetKind::Droplist | WidgetKind::ListBox => {
            ensure!(
                Some(node.kind.as_str()) == native_form_kind(kind),
                "原生保存暂不支持转换为{}，请从同类原生模板新增",
                kind.label()
            );
        }
        WidgetKind::Panel => bail!("原生保存暂不支持将基础元件转换为动态容器"),
    }
    Ok(())
}

pub(super) fn native_form_kind(kind: WidgetKind) -> Option<&'static str> {
    match kind {
        WidgetKind::Radio => Some("Axure:DiagramObject:RadioButton"),
        WidgetKind::TextArea => Some("Axure:DiagramObject:TextArea"),
        WidgetKind::Droplist => Some("Axure:DiagramObject:ComboBox"),
        WidgetKind::ListBox => Some("Axure:DiagramObject:ListBox"),
        _ => None,
    }
}

/// SelectObject's string/selection fields were checked in the installed RP 11
/// form library. Old records are retained in case another source object uses them.
pub(super) fn set_list_options(graph: &mut Graph, node: &mut Node, text: &str) {
    let mut options = Vec::new();
    if !text.is_empty() {
        for (index, text) in text.split('\n').enumerate() {
            let id = guid();
            graph.objects.insert(
                id.clone(),
                Node {
                    kind: "Axure:SelectObject".into(),
                    properties: json!({"object":text,"selected":index==0}),
                },
            );
            options.push(json!({"$ref":id,"$token":3}));
        }
    }
    node.properties["list-options"] = json!(options);
    node.properties["multiple"] = json!(false);
}

pub(super) fn guid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{:032x}",
        timestamp
            ^ ((std::process::id() as u128) << 72)
            ^ (COUNTER.fetch_add(1, Ordering::Relaxed) as u128) << 32
    )
}

pub(super) fn add_widget(
    parts: &mut BTreeMap<u64, Graph>,
    package: &str,
    widget: &Widget,
    pages: &BTreeMap<u64, String>,
) -> Result<String> {
    ensure!(
        widget.kind != WidgetKind::Panel,
        "原生保存暂不支持新增动态容器，请使用矩形或其他基础元件"
    );
    let form_kind = native_form_kind(widget.kind);
    let (template_graph, template_id) = parts
        .values()
        .find_map(|g| {
            g.objects
                .iter()
                .find(|(_, n)| {
                    if let Some(kind) = form_kind {
                        n.kind == kind
                    } else {
                        n.kind == "Axure:DiagramObject:VectorShape"
                            && n.properties["Shape"].as_i64() == Some(0)
                    }
                })
                .map(|(id, _)| (g.clone(), id.clone()))
        })
        .with_context(|| if form_kind.is_some() {
            format!("此文件缺少{}原生模板，暂不支持在该原生文件中新增此类型；可在 Ingenaro 工程中使用", widget.kind.label())
        } else {
            "此文件没有可用于新增元件的原生形状模板".into()
        })?;
    let template = template_graph.node(&template_id).unwrap();
    let style_id = template
        .properties
        .get("MyStyle")
        .and_then(reference_id)
        .context("原生模板样式不存在")?;
    let style_graph = parts
        .values()
        .find(|g| g.node(style_id).is_some())
        .context("原生模板样式对象不存在")?
        .clone();
    let graph = parts
        .values_mut()
        .find(|g| g.package == package)
        .context("新增元件的原生页面不存在")?;
    let diagram = graph
        .root()
        .and_then(|n| n.properties.get("diagram"))
        .and_then(reference_id)
        .context("原生页面画布不存在")?
        .to_owned();
    let id = guid();
    let new_style_id = guid();
    graph.copy_node_from(&template_graph, &template_id, &id)?;
    graph.copy_node_from(&style_graph, style_id, &new_style_id)?;
    let mut node = graph.node(&id).unwrap().clone();
    let mut style = graph.node(&new_style_id).unwrap().clone();
    let mut text_child = if !widget.kind.is_plain_text_control() {
        if let Some(source_id) = contained_text_id(&template_graph, template) {
            let child_id = guid();
            graph.copy_node_from(&template_graph, &source_id, &child_id)?;
            let mut child = graph.node(&child_id).unwrap().clone();
            replace_reference(&mut child.properties, &source_id, &child_id);
            replace_reference(&mut child.properties, &template_id, &id);
            node.properties["diagram-objects"] = json!([{"$ref":child_id}]);
            Some((child_id, child))
        } else {
            None
        }
    } else {
        node.properties["diagram-objects"] = json!([]);
        None
    };
    replace_reference(&mut node.properties, &template_id, &id);
    if let Some(old_diagram) = template
        .properties
        .pointer("/name-block/property-dictionary/parent-diagram-id")
        .and_then(reference_id)
    {
        replace_reference(&mut node.properties, old_diagram, &diagram);
        if let Some((_, child)) = &mut text_child {
            replace_reference(&mut child.properties, old_diagram, &diagram);
        }
    }
    node.properties
        .as_object_mut()
        .unwrap()
        .remove("InteractionMap");
    node.properties["AdaptiveStyles"] = json!([]);
    node.properties["MyStyle"] = json!({"$ref":new_style_id});
    node.properties["name-block"] = json!({"*type":"text-block","text":widget.name,"property-dictionary":{
        "type":"label","diagram-object-id":{"$guid":id},"parent-diagram-id":{"$guid":diagram}}});
    node.properties["VisibleFromSettings"] = json!(!widget.hidden);
    node.properties["IsLocked"] = json!(widget.locked);
    for field in [
        "AutoFitHeight",
        "AutoFitWidth",
        "IsContained",
        "Disabled",
        "Selected",
        "Error",
    ] {
        node.properties[field] = json!(false);
    }
    node.properties["SubmitButton"] = json!({"$ref":EMPTY_ID});
    set_kind(&mut node, widget.kind)?;
    if widget.kind == WidgetKind::Radio {
        node.properties["radio-group"] =
            json!(widget.radio_group.as_deref().unwrap_or(&widget.name));
    }
    if widget.kind.is_option_list() {
        set_list_options(graph, &mut node, &widget.text);
    }
    if widget.kind.is_plain_text_control() {
        node.properties["ReadOnly"] = json!(false);
        node.properties["PlaceholderText"] = json!("");
        node.properties["HideHintOnFocus"] = json!(false);
        if widget.kind == WidgetKind::TextArea {
            node.properties["WordWrap"] = json!(true);
        }
    }
    if widget.kind != WidgetKind::Image
        && let Some(points) = &widget.vector_points
    {
        crate::vector_edit::validate_points(points)?;
        node.properties["ControlPoints"] = points.clone();
    }
    if matches!(
        widget.kind,
        WidgetKind::Image | WidgetKind::Checkbox | WidgetKind::Radio
    ) || widget.kind.is_plain_text_control()
    {
        node.properties
            .as_object_mut()
            .unwrap()
            .remove("ControlPoints");
    }
    if widget.kind.is_plain_text_control() {
        node.properties["TextBlock"]["property-dictionary"] = json!({"type":"plain-text-object",
            "diagram-object-label":widget.name,"text-object-id":{"$guid":id},"parent-diagram-id":{"$guid":diagram}});
    }
    if let Some((child_id, mut child)) = text_child {
        if child.properties.get("rtf-block").is_none() {
            child.properties["rtf-block"] = json!({"*type":"rich-text-block", "attributed-string":{
                "Version":{"Major":8,"Minor":0,"Build":0,"Revision":0},"Paragraphs":[]}});
        }
        child.properties["ContainedSize"] = json!({"Width":widget.width,"Height":widget.height});
        set_text(&mut child.properties, &widget.text);
        update_inlines(&mut child.properties, widget, 1.0)?;
        apply_added_text_layout(&mut child.properties, widget);
        graph.objects.insert(child_id, child);
    } else {
        set_text(
            &mut node.properties,
            if widget.kind.is_option_list() {
                ""
            } else {
                &widget.text
            },
        );
        update_inlines(&mut node.properties, widget, 1.0)?;
        apply_added_text_layout(&mut node.properties, widget);
    }
    style.properties["PropList"] = json!([]);
    for (key, value) in [
        (101, json!({"X":widget.x,"Y":widget.y})),
        (105, json!({"Width":widget.width,"Height":widget.height})),
        (
            7,
            json!({"fill-type":"solid","fill-color":color_argb(&widget.fill)?}),
        ),
        (
            8,
            json!({"fill-type":"solid","fill-color":color_argb(if widget.kind==WidgetKind::Line {&widget.fill}else{&widget.stroke})?}),
        ),
        (
            6,
            json!({"fill-type":"solid","fill-color":color_argb(&widget.text_color)?}),
        ),
        (2, json!(widget.font_size * 0.75)),
        (71, json!(widget.radius)),
        (70, json!(widget.opacity)),
        (ROTATION_PROPERTY, json!(widget.rotation)),
    ] {
        set_property(&mut style.properties, key, value);
    }
    if let Some(layout) = &widget.text_layout {
        set_property(&mut style.properties, 9, json!(layout.horizontal));
        set_property(&mut style.properties, 10, json!(layout.vertical));
        for (property, padding) in [11, 12, 13, 14].into_iter().zip(layout.padding) {
            set_property(&mut style.properties, property, json!(padding));
        }
    }
    if let Some(data) = &widget.image_data {
        set_property(
            &mut style.properties,
            27,
            json!({"Stream":{"$binary":data.split_once(',').context("图片格式无效")?.1}}),
        );
    }
    graph.objects.insert(id.clone(), node);
    graph.objects.insert(new_style_id, style);
    graph
        .objects
        .get_mut(&diagram)
        .context("原生画布对象不存在")?
        .properties["diagram-objects"]
        .as_array_mut()
        .context("原生画布元件列表无效")?
        .push(json!({"$ref":id}));
    let offset = *parts.iter().find(|(_, g)| g.package == package).unwrap().0;
    set_interaction(parts, offset, &id, &widget.interaction, pages)?;
    Ok(format!("{package}/{id}"))
}
