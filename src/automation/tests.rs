use super::*;
use crate::model::{Action, AuthoredEvent, EventTrigger, Interaction, PageTreeNode, WidgetKind};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

fn call(name: &str, arguments: Value) -> ToolCall {
    ToolCall {
        name: name.into(),
        arguments,
    }
}
fn batch(project: &mut Project, operations: Value) -> Result<Value, String> {
    execute(
        project,
        false,
        0,
        &[],
        &call("rpshop_batch_edit", json!({"operations":operations})),
    )
}
fn fixture() -> Project {
    let mut project = Project::blank();
    for kind in [WidgetKind::Rectangle, WidgetKind::Text, WidgetKind::Button] {
        let id = project.allocate_id();
        project.pages[0]
            .widgets
            .push(Widget::new(id, kind, 20.0, 40.0));
    }
    project
}

#[test]
fn batch_validation_and_operation_failures_preserve_document_and_allocator() {
    let mut project = fixture();
    let before = project.clone();
    assert!(
        batch(
            &mut project,
            json!([
                {"op":"create_page","name":"Candidate"},
                {"op":"create_widget","kind":"Button","properties":{"width":0}}
            ])
        )
        .is_err()
    );
    assert_eq!(project, before);
    assert!(
        batch(
            &mut project,
            json!([
                {"op":"update_widget","widget_id":2,"properties":{"text":"Changed"}},
                {"op":"delete_widget","widget_id":999999}
            ])
        )
        .is_err()
    );
    assert_eq!(project, before);
    assert!(
        batch(
            &mut project,
            json!([
                {"op":"create_widget","kind":"Rectangle","properties":{"unknown_property":1}}
            ])
        )
        .is_err()
    );
    assert_eq!(project, before);
    assert!(
        batch(
            &mut project,
            json!([
                {"op":"create_widget","kind":"Rectangle","id":u64::MAX}
            ])
        )
        .is_err()
    );
    assert_eq!(project, before);
}

#[test]
fn batch_creates_cross_references_reorders_groups_and_authored_interactions() {
    let mut project = fixture();
    let result = batch(
        &mut project,
        json!([
            {"op":"update_project","name":"Live unsaved"},
            {"op":"create_page","id":10,"name":"Destination"},
            {"op":"create_widget","id":11,"kind":"Button","properties":{"text":"Navigate"}},
            {"op":"create_interaction","owner":11,"trigger":"Click","name":"Next","actions":[
                {"action":"Navigate","target_page":10,"target_widget":null,"value":""},
                {"action":"SetText","target_page":null,"target_widget":3,"value":"Updated"}
            ]},
            {"op":"group","widget_ids":[2,3]},
            {"op":"reorder_widgets","widget_ids":[2,3],"direction":"front"},
            {"op":"reorder_pages","page_ids":[10,1]}
        ]),
    )
    .unwrap();
    assert_eq!(result["changed"], true);
    assert_eq!(project.name, "Live unsaved");
    assert_eq!(project.pages[0].id, 10);
    assert_eq!(
        project.pages[1]
            .widgets
            .iter()
            .map(|w| w.id)
            .collect::<Vec<_>>(),
        [4, 11, 2, 3]
    );
    assert_eq!(project.pages[1].widgets[2].groups, [12]);
    assert_eq!(project.interaction_events[0].owner, Some(11));
    assert_eq!(
        project.interaction_events[0].actions[1].action,
        Action::SetText
    );
    project.validate().unwrap();
}

#[test]
fn named_layers_are_discoverable_and_edits_follow_the_group_lifecycle() {
    let mut project = fixture();
    let inner = project.next_id;
    let result = batch(
        &mut project,
        json!([
            {"op":"group","widget_ids":[2,3],"name":"页头"},
            {"op":"group","widget_ids":[2,4],"name":"内容"},
            {"op":"rename_group","group_id":inner,"name":"顶部导航"},
            {"op":"update_widget","widget_id":2,"properties":{"name":"导航背景"}}
        ]),
    )
    .unwrap();
    let outer = result["results"][1]["group_id"].as_u64().unwrap();
    let read = call("rpshop_read_layers", json!({"page_id":1}));
    let layers = execute(&mut project, false, 0, &[], &read).unwrap();
    let root = &layers["layers"][0];
    assert_eq!(root["name"], "内容");
    assert_eq!(root["children"][0]["widget_id"], 4);
    let nested = &root["children"][1];
    assert_eq!(nested["group_id"], inner);
    assert_eq!(nested["name"], "顶部导航");
    assert_eq!(nested["children"][1]["name"], "导航背景");

    let saved: Project = serde_json::from_str(&serde_json::to_string(&project).unwrap()).unwrap();
    assert_eq!(saved, project);
    let invalid = batch(
        &mut project,
        json!([
            {"op":"rename_group","group_id":outer,"name":"候选名称"},
            {"op":"rename_group","group_id":inner,"name":"   "}
        ]),
    );
    assert!(invalid.is_err());
    assert_eq!(project, saved);

    batch(&mut project, json!([{"op":"ungroup","widget_ids":[2]}])).unwrap();
    assert_eq!(project.pages[0].group_names.len(), 1);
    assert_eq!(project.pages[0].group_name(inner), "顶部导航");
    batch(
        &mut project,
        json!([{"op":"delete_widget","widget_id":2},{"op":"delete_widget","widget_id":3}]),
    )
    .unwrap();
    assert!(project.pages[0].group_names.is_empty());
}

#[test]
fn native_preflight_rejects_unsupported_structure_before_changes() {
    let mut project = fixture();
    let before = project.clone();
    for operation in [
        json!({"op":"create_page","name":"No"}),
        json!({"op":"delete_page","page_id":1}),
        json!({"op":"reorder_pages","page_ids":[1]}),
        json!({"op":"update_project","name":"No"}),
        json!({"op":"update_page","properties":{"width":500}}),
        json!({"op":"create_widget","kind":"Panel"}),
        json!({"op":"update_widget","widget_id":2,"properties":{"kind":"Radio"}}),
        json!({"op":"update_widget","widget_id":2,"properties":{"image_data":null}}),
    ] {
        let request = call(
            "rpshop_batch_edit",
            json!({"operations":[
                {"op":"update_widget","widget_id":2,"properties":{"text":"Before unsupported edit"}},operation
            ]}),
        );
        assert!(execute(&mut project, true, 0, &[], &request).is_err());
        assert_eq!(project, before);
    }
    let result = execute(
        &mut project,
        true,
        0,
        &[],
        &call(
            "rpshop_batch_edit",
            json!({"operations":[
                {"op":"update_widget","widget_id":2,"properties":{"text":"Supported candidate"}},
                {"op":"create_interaction","owner":2,"trigger":"Click","actions":[
                    {"action":"ToggleVisibility","target_page":null,"target_widget":3,"value":""}
                ]}
            ]}),
        ),
    )
    .unwrap();
    assert_eq!(result["requires_native_validation"], true);
}

#[test]
fn deletion_prunes_legacy_and_authored_references_and_promotes_tree_children() {
    let mut project = fixture();
    batch(
        &mut project,
        json!([{"op":"create_page","id":10,"name":"Second"}]),
    )
    .unwrap();
    project.page_tree = vec![PageTreeNode::Page {
        page_id: 10,
        children: vec![PageTreeNode::Page {
            page_id: 1,
            children: vec![],
        }],
    }];
    project.pages[0].widgets[0].interaction = Interaction {
        action: Action::Navigate,
        target_page: Some(10),
        target_widget: None,
        value: String::new(),
    };
    let mut event = AuthoredEvent::new(1, Some(2), EventTrigger::Click);
    event.actions = vec![Interaction {
        action: Action::SetText,
        target_page: None,
        target_widget: Some(3),
        value: "new".into(),
    }];
    project.interaction_events.push(event);
    batch(
        &mut project,
        json!([{"op":"delete_page","page_id":10},{"op":"delete_widget","widget_id":3}]),
    )
    .unwrap();
    assert_eq!(project.pages[0].widgets[0].interaction.action, Action::None);
    assert!(project.interaction_events[0].actions.is_empty());
    assert_eq!(
        project.page_tree,
        vec![PageTreeNode::Page {
            page_id: 1,
            children: vec![]
        }]
    );
    project.validate().unwrap();
}

#[test]
fn readonly_tools_are_strict_and_never_modify_live_document() {
    let mut project = fixture();
    let before = project.clone();
    for (name, args) in [
        ("rpshop_capabilities", json!({})),
        ("rpshop_read_project", json!({})),
        ("rpshop_read_pages", json!({})),
        ("rpshop_read_widgets", json!({"page_id":1,"widget_ids":[2]})),
        ("rpshop_read_selection", json!({})),
        ("rpshop_read_styles", json!({})),
        ("rpshop_read_interactions", json!({"owner":null})),
    ] {
        execute(&mut project, false, 0, &[2], &call(name, args)).unwrap();
        assert_eq!(project, before);
    }
    for (name, args) in [
        ("rpshop_read_widgets", json!({"page_id":0})),
        ("rpshop_read_widgets", json!({"widget_ids":[9999]})),
        ("rpshop_read_interactions", json!({"owner":"oops"})),
        ("rpshop_read_selection", json!({"change_selection":[2]})),
        ("rpshop_read_project", Value::Null),
        ("invented", json!({})),
    ] {
        assert!(execute(&mut project, false, 0, &[], &call(name, args)).is_err());
    }
    for name in schema::GUI_TOOLS {
        assert!(
            execute(&mut project, false, 0, &[], &call(name, json!({})))
                .unwrap_err()
                .contains("GUI host")
        );
    }
    assert_eq!(project, before);
}

#[test]
fn locked_widgets_require_explicit_unlock_and_cross_scope_reorder_fails_atomically() {
    let mut project = fixture();
    project.pages[0].widgets[0].locked = true;
    let before = project.clone();
    assert!(
        batch(
            &mut project,
            json!([{"op":"update_widget","widget_id":2,"properties":{"text":"no"}}])
        )
        .is_err()
    );
    assert_eq!(project, before);
    batch(
        &mut project,
        json!([
            {"op":"update_widget","widget_id":2,"properties":{"locked":false}},
            {"op":"update_widget","widget_id":2,"properties":{"text":"yes"}},
            {"op":"group","widget_ids":[2,3]}
        ]),
    )
    .unwrap();
    let before = project.clone();
    assert!(
        batch(
            &mut project,
            json!([{"op":"reorder_widgets","widget_ids":[2,4],"direction":"front"}])
        )
        .is_err()
    );
    assert_eq!(project, before);
}

#[test]
fn interaction_updates_delete_and_invalid_owner_are_transactional() {
    let mut project = fixture();
    let result = batch(
        &mut project,
        json!([{"op":"create_interaction","trigger":"PageLoad","actions":[]}]),
    )
    .unwrap();
    let event_id = result["results"][0]["event_id"].as_str().unwrap();
    batch(
        &mut project,
        json!([{"op":"update_interaction","event_id":event_id,"properties":{"name":"Load"}}]),
    )
    .unwrap();
    let before = project.clone();
    assert!(
        batch(
            &mut project,
            json!([{"op":"update_interaction","event_id":event_id,"properties":{"owner":2}}])
        )
        .is_err()
    );
    assert_eq!(project, before);
    batch(
        &mut project,
        json!([{"op":"delete_interaction","event_id":event_id}]),
    )
    .unwrap();
    assert!(project.interaction_events.is_empty());
}

#[test]
fn tool_schemas_cover_every_dispatch_name_and_define_strict_batch_variants() {
    let tools = tools();
    let list = tools.as_array().unwrap();
    assert_eq!(list.len(), 10 + schema::GUI_TOOLS.len());
    for tool in list {
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    }
    let batch = list
        .iter()
        .find(|tool| tool["name"] == "rpshop_batch_edit")
        .unwrap();
    assert_eq!(
        batch["inputSchema"]["properties"]["operations"]["items"]["oneOf"]
            .as_array()
            .unwrap()
            .len(),
        schema::OPERATIONS.len()
    );
    let publish = list
        .iter()
        .find(|tool| tool["name"] == "rpshop_publish_html")
        .unwrap();
    assert_eq!(publish["inputSchema"]["required"], json!(["source"]));
}

fn request_from(host: &ToolHost) -> ToolRequest {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(request) = host.try_recv() {
            return request;
        }
        assert!(Instant::now() < deadline, "No live request arrived");
        thread::yield_now();
    }
}

#[test]
fn live_host_routes_scope_and_executes_against_current_unsaved_state() {
    let (host, client) = ToolHost::new();
    let cancelled = Arc::new(AtomicBool::new(false));
    let client = client.scoped("document-generation-42".into(), cancelled);
    let worker = thread::spawn(move || client.call(call("rpshop_read_project", json!({}))));
    let request = request_from(&host);
    assert_eq!(request.scope, "document-generation-42");
    assert!(!request.cancelled.load(Ordering::Acquire));
    let mut project = fixture();
    project.name = "Unsaved GUI edit".into();
    request
        .reply
        .send(execute(&mut project, false, 0, &[], &request.call))
        .unwrap();
    assert_eq!(
        worker.join().unwrap().unwrap()["project"]["name"],
        "Unsaved GUI edit"
    );
}

#[test]
fn queued_call_cancellation_marks_request_and_returns_without_waiting_for_gui() {
    let (host, client) = ToolHost::new();
    let cancelled = Arc::new(AtomicBool::new(false));
    let client = client.scoped("scope".into(), cancelled.clone());
    let worker = thread::spawn(move || client.call(call("rpshop_read_project", json!({}))));
    let request = request_from(&host);
    cancelled.store(true, Ordering::Release);
    assert!(
        request.is_cancelled(),
        "Host observes parent cancellation synchronously"
    );
    let start = Instant::now();
    assert!(worker.join().unwrap().unwrap_err().contains("cancelled"));
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(request.cancelled.load(Ordering::Acquire));
}

#[test]
fn timeout_cancels_host_request_and_disconnection_does_not_hang() {
    let (host, client) = ToolHost::new();
    let worker = thread::spawn(move || {
        client.call_until(
            call("rpshop_read_pages", json!({})),
            &Arc::new(AtomicBool::new(false)),
            Duration::from_millis(100),
        )
    });
    let request = request_from(&host);
    assert!(worker.join().unwrap().unwrap_err().contains("timed out"));
    assert!(request.cancelled.load(Ordering::Acquire));
    let (host, client) = ToolHost::new();
    drop(host);
    assert!(
        client
            .call(call("rpshop_read_project", json!({})))
            .unwrap_err()
            .contains("disconnected")
    );
}

#[test]
fn live_host_batch_failure_does_not_commit_partial_changes() {
    let (host, client) = ToolHost::new();
    let worker = thread::spawn(move || {
        client.call(call(
            "rpshop_batch_edit",
            json!({"operations":[
                {"op":"create_widget","kind":"Button"},
                {"op":"update_page","properties":{"background":"not a color"}}
            ]}),
        ))
    });
    let request = request_from(&host);
    let mut live = fixture();
    let before = live.clone();
    assert!(!request.cancelled.load(Ordering::Acquire));
    request
        .reply
        .send(execute(&mut live, false, 0, &[], &request.call))
        .unwrap();
    assert!(worker.join().unwrap().is_err());
    assert_eq!(live, before);
}

#[test]
fn already_cancelled_call_never_enters_host_queue() {
    let (host, client) = ToolHost::new();
    let client = client.scoped("old-document".into(), Arc::new(AtomicBool::new(true)));
    assert!(client.call(call("rpshop_read_pages", json!({}))).is_err());
    assert!(host.try_recv().is_none());
}
