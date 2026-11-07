//! Authored cases are appended to native trees; unrelated source cases are preserved.

use super::*;
use crate::model::{AuthoredEvent, EventTrigger};

pub(super) const AUTHORED_PREFIX: &str = "72707368";

pub(super) fn is_authored_case(id: &str) -> bool {
    id.rsplit('/')
        .next()
        .is_some_and(|id| id.len() == 32 && id.starts_with(AUTHORED_PREFIX))
}

pub(super) fn read(
    graph: &Graph,
    roots: &BTreeMap<String, String>,
    project: &Project,
    widgets: &BTreeMap<u64, native::Binding>,
    pages: &BTreeMap<String, u64>,
) -> Vec<AuthoredEvent> {
    use super::preview_program::{PreviewAction, SourceBinding};
    if !graph
        .objects
        .iter()
        .any(|(id, node)| node.kind == "Axure:InteractionCase" && is_authored_case(id))
    {
        return Vec::new();
    }
    let mut owners = Vec::new();
    let mut bindings = Vec::new();
    for (package, page) in pages {
        if let Some(source) = roots.get(package) {
            let binding = SourceBinding {
                key: format!("page:{page}"),
                source: source.clone(),
                scope: vec![package.clone()],
            };
            owners.push((binding.clone(), *page, None));
            bindings.push(binding);
        }
    }
    for page in &project.pages {
        for widget in &page.widgets {
            if let Some(native) = widgets.get(&widget.id) {
                let Some((package, _)) = native.object.split_once('/') else {
                    continue;
                };
                let package = pages
                    .iter()
                    .find(|(_, id)| **id == page.id)
                    .map(|(key, _)| key.as_str())
                    .unwrap_or(package);
                let binding = SourceBinding {
                    key: widget.id.to_string(),
                    source: native.object.clone(),
                    scope: vec![package.to_owned()],
                };
                owners.push((binding.clone(), page.id, Some(widget.id)));
                bindings.push(binding);
            }
        }
    }
    let page_keys = pages
        .iter()
        .map(|(key, id)| (key.clone(), id.to_string()))
        .collect();
    let program = super::preview_program::compile(graph, &bindings, &page_keys);
    let mut result = Vec::new();
    for event in &program.events {
        let Some((binding, page, owner)) = owners
            .iter()
            .find(|(binding, _, _)| binding.key == event.owner)
        else {
            continue;
        };
        let trigger = match (event.event_type, owner) {
            (320, None) => EventTrigger::PageLoad,
            (16, Some(_)) => EventTrigger::Click,
            _ => continue,
        };
        let Some(source) = graph.node(&binding.source) else {
            continue;
        };
        let field = if owner.is_some() {
            "InteractionMap"
        } else {
            "interactionmap"
        };
        let Some(map) = graph.resolve(&source.properties[field]) else {
            continue;
        };
        for case in &event.cases {
            let Some(id) = map.properties["node-table"]
                .get(&case.source)
                .and_then(reference_id)
                .filter(|id| is_authored_case(id))
            else {
                continue;
            };
            if !case.supported {
                continue;
            }
            let mut actions = Vec::new();
            let mut valid = true;
            for action in &case.actions {
                match action {
                    PreviewAction::LinkWindow { page, mode: 1 } => actions.push(Interaction {
                        action: Action::Navigate,
                        target_page: page.key.as_ref().and_then(|id| id.parse().ok()),
                        ..Default::default()
                    }),
                    PreviewAction::Fade { entries }
                        if entries.len() == 1
                            && entries[0].action == 2
                            && entries[0].show_type == 0
                            && entries[0].easing == 0
                            && entries[0].targets.len() == 1 =>
                    {
                        actions.push(Interaction {
                            action: Action::ToggleVisibility,
                            target_widget: entries[0].targets[0].parse().ok(),
                            ..Default::default()
                        });
                    }
                    PreviewAction::SetText { targets, value } if targets.len() == 1 => actions
                        .push(Interaction {
                            action: Action::SetText,
                            target_widget: targets[0].parse().ok(),
                            value: value.clone(),
                            ..Default::default()
                        }),
                    _ => {
                        valid = false;
                        break;
                    }
                }
            }
            if valid {
                let authored = AuthoredEvent {
                    id: id.rsplit('/').next().unwrap_or(id).into(),
                    page: *page,
                    owner: *owner,
                    trigger,
                    name: case.name.clone(),
                    actions,
                };
                let mut probe = project.clone();
                probe.interaction_events = vec![authored.clone()];
                if probe.validate().is_ok() {
                    result.push(authored);
                }
            }
        }
    }
    result
}

fn insert(graph: &mut Graph, id: String, kind: &str, properties: Value) {
    graph.objects.insert(
        id,
        Node {
            kind: kind.into(),
            properties,
        },
    );
}

fn guid() -> String {
    AuthoredEvent::new(1, None, EventTrigger::PageLoad)
        .id
        .replacen(AUTHORED_PREFIX, "2a216987", 1)
}

fn literal(graph: &mut Graph, text: &str) -> String {
    let id = guid();
    insert(
        graph,
        id.clone(),
        "Axure:Expr:StringLiteral",
        serde_json::json!({"value":text,"variable-identifiers":[],"repeater-scope":{"$ref":EMPTY_ID},"TargetRepeaterScope":{"$ref":EMPTY_ID}}),
    );
    id
}

fn path(graph: &mut Graph, object: &str) -> String {
    let id = guid();
    insert(
        graph,
        id.clone(),
        "Axure:DiagramObjectPath",
        serde_json::json!({"object-id":{"$ref":object},"rdo-path":[],"repeater-path":[],"is-dummy":false}),
    );
    id
}

fn remove_cases(graph: &mut Graph, ids: &BTreeSet<String>) -> Result<()> {
    let maps = graph
        .objects
        .iter()
        .filter(|(_, node)| node.kind == "Axure:InteractionTreeMap")
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    for map_id in maps {
        let Some(table) = graph.objects[&map_id].properties["node-table"].as_object() else {
            continue;
        };
        let cases = table
            .iter()
            .filter(|(_, value)| reference_id(value).is_some_and(|id| ids.contains(id)))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for case in cases {
            let children = graph
                .node(&case)
                .map(|node| {
                    array(&node.properties["children"])
                        .iter()
                        .filter_map(reference_id)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            for node in graph
                .objects
                .values_mut()
                .filter(|node| node.kind == "Axure:Node")
            {
                if let Some(children) = node.properties["children"].as_array_mut() {
                    children.retain(|value| reference_id(value) != Some(case.as_str()));
                }
            }
            let table = graph.objects.get_mut(&map_id).unwrap().properties["node-table"]
                .as_object_mut()
                .unwrap();
            table.remove(&case);
            for child in children {
                table.remove(&child);
            }
        }
    }
    Ok(())
}

pub(super) fn patch(
    parts: &mut BTreeMap<u64, Graph>,
    old: &Project,
    current: &Project,
    widgets: &BTreeMap<u64, native::Binding>,
    pages: &BTreeMap<u64, String>,
) -> Result<()> {
    if old.interaction_events == current.interaction_events {
        return Ok(());
    }
    let ids = old
        .interaction_events
        .iter()
        .chain(&current.interaction_events)
        .map(|event| event.id.clone())
        .collect::<BTreeSet<_>>();
    for graph in parts.values_mut() {
        remove_cases(graph, &ids)?;
    }
    for event in &current.interaction_events {
        ensure!(
            is_authored_case(&event.id),
            "新增原生用例必须使用Ingenaro生成的交互标识"
        );
        let package = pages.get(&event.page).context("交互页面原生绑定不存在")?;
        let source = if let Some(owner) = event.owner {
            widgets
                .get(&owner)
                .context("交互所属元件只读或缺少原生绑定")?
                .object
                .clone()
        } else {
            let graph = parts
                .values()
                .find(|graph| &graph.package == package)
                .context("交互页面部件不存在")?;
            format!("{package}/{}", graph.root)
        };
        let (owner_package, owner_id) = source.rsplit_once('/').context("交互所属对象标识无效")?;
        let offset = *parts
            .iter()
            .find(|(_, graph)| graph.package == owner_package)
            .context("交互所属部件不存在")?
            .0;
        let graph = parts.get_mut(&offset).unwrap();
        append(graph, owner_id, event, current, widgets, pages)?;
    }
    Ok(())
}

fn append(
    graph: &mut Graph,
    owner: &str,
    event: &AuthoredEvent,
    project: &Project,
    widgets: &BTreeMap<u64, native::Binding>,
    pages: &BTreeMap<u64, String>,
) -> Result<()> {
    use serde_json::json;
    let field = if event.owner.is_some() {
        "InteractionMap"
    } else {
        "interactionmap"
    };
    let existing = graph
        .node(owner)
        .context("原生交互所属对象不存在")?
        .properties
        .get(field)
        .and_then(reference_id)
        .filter(|id| *id != EMPTY_ID)
        .map(str::to_owned);
    let (map_id, tree_id) = if let Some(map) = existing {
        let tree = graph.node(&map).context("原生交互树不存在")?.properties["tree"]
            .as_object()
            .and_then(|_| reference_id(&graph.objects[&map].properties["tree"]))
            .context("原生交互树引用无效")?
            .to_owned();
        ensure!(
            graph.objects[&map].properties["node-table"].is_object(),
            "原生交互节点表无效"
        );
        (map, tree)
    } else {
        let map = guid();
        let tree = guid();
        insert(
            graph,
            map.clone(),
            "Axure:InteractionTreeMap",
            json!({"node-table":{},"tree":{"$ref":tree},"PackageMode":false}),
        );
        insert(
            graph,
            tree.clone(),
            "Axure:Tree",
            json!({"children":[],"tree":{"$ref":tree}}),
        );
        graph.objects.get_mut(owner).unwrap().properties[field] = json!({"$ref":map});
        (map, tree)
    };
    let existing_event = array(
        &graph
            .node(&tree_id)
            .context("原生事件树根不存在")?
            .properties["children"],
    )
    .iter()
    .filter_map(reference_id)
    .find(|tree| {
        graph.objects[&map_id].properties["node-table"]
            .get(*tree)
            .and_then(|value| graph.resolve(value))
            .is_some_and(|node| {
                node.kind == "Axure:InteractionEvent"
                    && node.properties["event-type"].as_i64() == Some(event.trigger.native_type())
                    && node.properties["Disabled"].as_bool() != Some(true)
            })
    })
    .map(str::to_owned);
    let event_tree = if let Some(id) = existing_event {
        id
    } else {
        let branch = guid();
        let actual = guid();
        insert(
            graph,
            branch.clone(),
            "Axure:Node",
            json!({"children":[],"parent-node":{"$ref":tree_id},"tree":{"$ref":tree_id}}),
        );
        insert(
            graph,
            actual.clone(),
            "Axure:InteractionEvent",
            json!({"Disabled":false,"event-type":event.trigger.native_type(),"master-containing-raised-event":{"$ref":EMPTY_ID,"$type":null,"$token":16},"raised-event-id":{"$guid":EMPTY_ID},"variable-id":{"$guid":EMPTY_ID}}),
        );
        graph.objects.get_mut(&tree_id).unwrap().properties["children"]
            .as_array_mut()
            .context("原生事件列表无效")?
            .push(json!({"$ref":branch}));
        graph.objects.get_mut(&map_id).unwrap().properties["node-table"][&branch] =
            json!({"$ref":actual});
        branch
    };
    let case_tree = guid();
    insert(
        graph,
        case_tree.clone(),
        "Axure:Node",
        json!({"children":[],"parent-node":{"$ref":event_tree},"tree":{"$ref":tree_id}}),
    );
    insert(
        graph,
        event.id.clone(),
        "Axure:InteractionCase",
        json!({"Disabled":false,"Description":event.name,"Condition":{"$ref":EMPTY_ID},"IsNewIfGroup":false}),
    );
    graph.objects.get_mut(&event_tree).unwrap().properties["children"]
        .as_array_mut()
        .context("原生用例列表无效")?
        .push(json!({"$ref":case_tree}));
    graph.objects.get_mut(&map_id).unwrap().properties["node-table"][&case_tree] =
        json!({"$ref":event.id});
    for interaction in &event.actions {
        let action = write_action(graph, interaction, project, widgets, pages)?;
        let branch = guid();
        insert(
            graph,
            branch.clone(),
            "Axure:Node",
            json!({"children":[],"parent-node":{"$ref":case_tree},"tree":{"$ref":tree_id}}),
        );
        graph.objects.get_mut(&case_tree).unwrap().properties["children"]
            .as_array_mut()
            .unwrap()
            .push(json!({"$ref":branch}));
        graph.objects.get_mut(&map_id).unwrap().properties["node-table"][&branch] =
            json!({"$ref":action});
    }
    graph.set_dictionary_key_token(&map_id, "/node-table", 3)?;
    Ok(())
}

fn write_action(
    graph: &mut Graph,
    action: &Interaction,
    project: &Project,
    widgets: &BTreeMap<u64, native::Binding>,
    pages: &BTreeMap<u64, String>,
) -> Result<String> {
    use serde_json::json;
    let id = guid();
    match action.action {
        Action::Navigate => {
            let target = guid();
            let text = literal(graph, "");
            let page = action
                .target_page
                .and_then(|page| pages.get(&page))
                .map(String::as_str)
                .unwrap_or(EMPTY_ID);
            insert(
                graph,
                target.clone(),
                "Axure:InteractionTarget",
                json!({"target-type":1,"target-page-id":{"$ref":page,"$type":"Axure:Page","$token":19},"target-literal":{"$ref":text}}),
            );
            insert(
                graph,
                id.clone(),
                "Axure:InteractionLinkWindowAction",
                json!({"Disabled":false,"target":{"$ref":target},"link-type":1}),
            );
        }
        Action::ToggleVisibility | Action::SetText => {
            let target = action.target_widget.context("交互目标元件未选择")?;
            let source = &widgets
                .get(&target)
                .context("交互目标元件只读或缺少原生绑定")?
                .object;
            let object = source.rsplit('/').next().unwrap();
            let target_path = path(graph, object);
            if action.action == Action::ToggleVisibility {
                let info = guid();
                insert(
                    graph,
                    info.clone(),
                    "Axure:FadeWidgetInfo",
                    json!({"fade-action-type":2,"show-type":0,"easing":0,"duration":0.0,"EasingHide":0,"DurationHide":0.0,"BringToFront":false,"vertical":false,"compress-easing":0,"compress-duration":0.0}),
                );
                insert(
                    graph,
                    id.clone(),
                    "Axure:InteractionFadeWidgetAction",
                    json!({"Disabled":false,"WidgetPaths":[{"$ref":target_path}],"FadeInfos":[{"$ref":info}],"panelobjectpaths":[{"$ref":target_path}]}),
                );
            } else {
                let input = project
                    .pages
                    .iter()
                    .flat_map(|page| &page.widgets)
                    .find(|widget| widget.id == target)
                    .is_some_and(|widget| widget.kind.is_plain_text_control());
                let function = if input {
                    "SetWidgetFormText"
                } else {
                    "SetWidgetRichText"
                };
                let target_literal = guid();
                insert(
                    graph,
                    target_literal.clone(),
                    "Axure:Expr:DiagramObjectPathLiteral",
                    json!({"value":{"$ref":target_path}}),
                );
                let text = literal(graph, &action.value);
                let call = guid();
                let block = guid();
                let mut args = vec![json!({"$ref":target_literal}), json!({"$ref":text})];
                if !input {
                    let plain = guid();
                    insert(
                        graph,
                        plain.clone(),
                        "Axure:Expr:BooleanLiteral",
                        json!({"value":"True"}),
                    );
                    args.push(json!({"$ref":plain}));
                }
                insert(
                    graph,
                    call.clone(),
                    "Axure:Expr:FCall",
                    json!({"function-name":function,"args":args}),
                );
                insert(
                    graph,
                    block.clone(),
                    "Axure:Expr:Block",
                    json!({"subexpressions":[{"$ref":call}]}),
                );
                insert(
                    graph,
                    id.clone(),
                    "Axure:InterationActionSetFunction",
                    json!({"Disabled":false,"action":{"$ref":block},"SetFunction":function}),
                );
            }
        }
        Action::None => anyhow::bail!("不能写入空交互动作"),
    }
    Ok(id)
}

#[cfg(test)]
#[path = "native_events_tests.rs"]
mod tests;
