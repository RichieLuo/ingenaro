use super::*;
use crate::{
    model::WidgetKind,
    widget_library::{Category, ITEMS},
};
use std::collections::BTreeSet;

fn call(name: &str, arguments: Value) -> ToolCall {
    ToolCall {
        name: name.into(),
        arguments,
    }
}
fn read(project: &mut Project, args: Value) -> Result<Value, String> {
    execute(project, false, 0, &[], &call("rpshop_read_library", args))
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
fn preset(item_id: &str) -> Value {
    json!({"op":"create_library_item","item_id":item_id,"x":10.0,"y":20.0})
}
fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
}

#[test]
fn enumerates_all_gui_presets_in_catalog_order_without_mutating_live_state() {
    let mut project = Project::blank();
    project.next_id = u64::MAX - 1; // reads never consume the live allocator.
    let before = project.clone();
    let catalog = read(&mut project, json!({})).unwrap();
    assert_eq!(catalog["catalog_total"], ITEMS.len());
    assert_eq!(catalog["count"], ITEMS.len());
    assert_eq!(
        catalog["categories"].as_array().unwrap().len(),
        Category::ALL.len()
    );
    let items = catalog["items"].as_array().unwrap();
    for (value, actual) in items.iter().zip(ITEMS) {
        assert_eq!(value["id"], actual.id);
        assert_eq!(value["label"], actual.label);
        assert_eq!(value["keywords"], actual.keywords);
        assert_eq!(value["category"], actual.category.id());
        assert_eq!(value["category_label"], actual.category.label());
        assert!(!value["description"].as_str().unwrap().is_empty());
        assert!(value["widget_count"].as_u64().unwrap() > 0);
        assert!(value["native_constraints"].is_object());
        assert!(
            value.get("default_widgets").is_none(),
            "summary must stay bounded"
        );
    }
    let capabilities = execute(
        &mut project,
        false,
        0,
        &[],
        &call("rpshop_capabilities", json!({})),
    )
    .unwrap();
    assert_eq!(capabilities["library_tool"], "rpshop_read_library");
    assert_eq!(
        capabilities["library_item_ids"],
        json!(ITEMS.iter().map(|i| i.id).collect::<Vec<_>>())
    );
    assert!(
        capabilities["batch_operations"]
            .as_array()
            .unwrap()
            .contains(&json!("create_library_item"))
    );
    assert_eq!(project, before);
}

#[test]
fn library_search_uses_gui_multilingual_aliases_combines_filters_and_details_ids_are_examples() {
    let mut project = Project::blank();
    for query in ["table", "表格", "数据表格"] {
        let result = read(&mut project, json!({"query":query,"category":"forms"})).unwrap();
        assert_eq!(result["count"], 1);
        assert_eq!(result["items"][0]["id"], "table");
        assert_eq!(result["items"][0]["representation"], "grouped_widgets");
    }
    assert_eq!(
        read(&mut project, json!({"query":"flow start"})).unwrap()["items"][0]["id"],
        "flow-start"
    );
    assert_eq!(
        read(&mut project, json!({"query":"SEARCH","category":"icons"})).unwrap()["items"][0]["id"],
        "icon-search"
    );
    assert_eq!(
        read(&mut project, json!({"item_id":"table","category":"icons"})).unwrap()["count"],
        0
    );
    let pattern = read(&mut project, json!({"query":"登录","category":"patterns"})).unwrap();
    assert_eq!(pattern["count"], 1);
    assert_eq!(pattern["items"][0]["id"], "pattern-login");
    assert_eq!(pattern["items"][0]["representation"], "grouped_widgets");
    assert_eq!(
        pattern["items"][0]["widget_kinds"],
        json!(["Rectangle", "Text", "Input", "Button"])
    );
    let table = read(&mut project, json!({"item_id":"table"})).unwrap();
    let detail = &table["items"][0];
    assert_eq!(detail["default_widgets"].as_array().unwrap().len(), 12);
    assert_eq!(detail["widget_kinds"], json!(["Rectangle"]));
    assert_eq!(detail["template_group_ids"].as_array().unwrap().len(), 1);
    assert_eq!(detail["template_ids_authoritative"], false);
    assert!(
        detail["template_id_note"]
            .as_str()
            .unwrap()
            .contains("not the live document")
    );
    assert_eq!(
        detail["default_bounds"],
        json!({"x":0.0,"y":0.0,"width":300.0,"height":144.0})
    );
    let native = execute(
        &mut project,
        true,
        0,
        &[],
        &call("rpshop_read_library", json!({"item_id":"radio"})),
    )
    .unwrap();
    assert_eq!(native["native"], true);
    assert_eq!(
        native["items"][0]["native_constraints"]["creation"],
        "requires_form_template"
    );
    assert_eq!(
        native["items"][0]["native_constraints"]["required_template_kind"],
        "Radio"
    );
    assert_eq!(
        read(&mut project, json!({"item_id":"container"})).unwrap()["items"][0]["native_constraints"]
            ["creation"],
        "unsupported"
    );
    for args in [
        json!({"item_id":"Table"}),
        json!({"category":"repeater"}),
        json!({"query":true}),
        json!({"item_id":42}),
        json!({"query":null}),
        json!({"category":null}),
        json!({"query":"x".repeat(257)}),
        json!({"limit":1}),
    ] {
        let before = project.clone();
        assert!(read(&mut project, args).is_err());
        assert_eq!(project, before);
    }
}

#[test]
fn every_library_item_is_created_atomically_using_the_actual_gui_factory_and_roundtrips() {
    for item in ITEMS {
        let mut project = Project::blank();
        let mut expected = project.clone();
        let widgets = item.create(&mut expected, 10.0, 20.0);
        expected.pages[0].widgets.extend(widgets);
        let result = batch(&mut project, json!([preset(item.id)])).unwrap();
        assert_eq!(
            project, expected,
            "different factory output for {}",
            item.id
        );
        assert_eq!(result["changed"], true);
        let result = &result["results"][0];
        assert_eq!(result["page_id"], 1);
        assert_eq!(result["item_id"], item.id);
        assert_eq!(
            result["widget_ids"],
            json!(
                project.pages[0]
                    .widgets
                    .iter()
                    .map(|w| w.id)
                    .collect::<Vec<_>>()
            )
        );
        let groups = project.pages[0]
            .widgets
            .iter()
            .flat_map(|w| w.groups.iter().copied())
            .collect::<BTreeSet<_>>();
        assert_eq!(result["group_ids"], json!(groups));
        project.validate().unwrap();
        let encoded = serde_json::to_vec(&project).unwrap();
        let decoded: Project = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(project, decoded, "roundtrip changed {}", item.id);
    }
    let mut project = Project::blank();
    let operations = ITEMS.iter().map(|item| preset(item.id)).collect::<Vec<_>>();
    let result = batch(&mut project, json!(operations)).unwrap();
    assert_eq!(result["results"].as_array().unwrap().len(), ITEMS.len());
    project.validate().unwrap();
    assert_eq!(
        project,
        serde_json::from_value::<Project>(json!(project)).unwrap()
    );
}

#[test]
fn table_cells_have_real_fresh_ids_grouping_custom_bounds_texts_and_subsequent_editability() {
    let mut project = Project::blank();
    project.next_id = 100;
    let texts = (0..12).map(|i| format!("单元格 {i}")).collect::<Vec<_>>();
    let result = batch(
        &mut project,
        json!([{
            "op":"create_library_item","item_id":"table","x":40.0,"y":60.0,
            "width":600.0,"height":240.0,"texts":texts,
            "properties":{"stroke":"#AABBCC","text_color":"#112233","font_size":20.0,"opacity":0.75}
        }]),
    )
    .unwrap();
    let result = &result["results"][0];
    assert_eq!(
        result["widget_ids"],
        json!((101u64..=112).collect::<Vec<_>>())
    );
    assert_eq!(result["group_ids"], json!([100]));
    assert_eq!(
        result["bounds"],
        json!({"x":40.0,"y":60.0,"width":600.0,"height":240.0})
    );
    assert_eq!(project.next_id, 113);
    for (index, widget) in project.pages[0].widgets.iter().enumerate() {
        assert_eq!(widget.kind, WidgetKind::Rectangle);
        assert_eq!(widget.groups, vec![100]);
        assert_eq!(widget.text, texts[index]);
        close(widget.x, 40.0 + (index % 3) as f32 * 200.0);
        close(widget.y, 60.0 + (index / 3) as f32 * 60.0);
        close(widget.width, 200.0);
        close(widget.height, 60.0);
        close(widget.font_size, 20.0);
        close(widget.opacity, 0.75);
        assert_eq!(widget.stroke, "#AABBCC");
        assert_eq!(widget.text_color, "#112233");
        assert_eq!(widget.fill, if index < 3 { "#F0EDFF" } else { "#FFFFFF" });
    }
    batch(
        &mut project,
        json!([{"op":"update_widget","widget_id":105,"properties":{"text":"editable cell"}}]),
    )
    .unwrap();
    assert_eq!(project.pages[0].widgets[4].text, "editable cell");
    batch(&mut project, json!([{"op":"ungroup","widget_ids":[101]}])).unwrap();
    assert!(project.pages[0].widgets.iter().all(|w| w.groups.is_empty()));
    project.validate().unwrap();
}

#[test]
fn menus_tree_icons_and_typography_keep_factory_structure_when_scaled_or_styled() {
    for (id, count, width, height) in [
        ("horizontal-menu", 4, 800.0, 72.0),
        ("vertical-menu", 4, 200.0, 288.0),
        ("tree", 5, 440.0, 360.0),
        ("icon-home", 1, 64.0, 96.0),
        ("drop-marker", 1, 80.0, 104.0),
        ("heading1", 1, 600.0, 102.4),
    ] {
        let mut project = Project::blank();
        let mut sandbox = Project::blank();
        let defaults =
            ITEMS
                .iter()
                .find(|item| item.id == id)
                .unwrap()
                .create(&mut sandbox, 0.0, 0.0);
        let mut operation = preset(id);
        operation["width"] = json!(width);
        operation["height"] = json!(height);
        batch(&mut project, json!([operation])).unwrap();
        assert_eq!(project.pages[0].widgets.len(), count);
        for (widget, default) in project.pages[0].widgets.iter().zip(&defaults) {
            assert_eq!(widget.groups, default.groups);
            assert_eq!(widget.vector_points, default.vector_points);
            assert_eq!(widget.font_size, default.font_size);
            assert_eq!(widget.radius, default.radius);
            assert_eq!(widget.text_layout, default.text_layout);
            assert_eq!(widget.text, default.text);
        }
        if id == "icon-home" {
            assert!(project.pages[0].widgets[0].vector_points.is_some());
        }
    }
    let mut project = Project::blank();
    batch(&mut project,json!([{
        "op":"create_library_item","item_id":"horizontal-menu","x":0,"y":0,"texts":["首页","产品"],
        "properties":{"fill":"#FEFEFE","locked":true,"hidden":true,
            "text_layout":{"horizontal":0,"vertical":2,"padding":[2,3,4,5],"wrap":false,"bold":true}}
    }])).unwrap();
    assert_eq!(
        project.pages[0]
            .widgets
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>(),
        ["首页", "产品", "菜单 3", "菜单 4"]
    );
    assert!(project.pages[0].widgets.iter().all(|w| w.locked
        && w.hidden
        && w.fill == "#FEFEFE"
        && w.text_layout.as_ref().unwrap().bold));
}

#[test]
fn omitted_page_id_uses_batch_start_page_and_explicit_ids_target_new_pages() {
    let mut project = Project::blank();
    batch(
        &mut project,
        json!([{"op":"create_page","id":50,"name":"Page 2"}]),
    )
    .unwrap();
    let result = execute(
        &mut project,
        false,
        1,
        &[],
        &call(
            "rpshop_batch_edit",
            json!({"operations":[
                {"op":"create_page","id":100,"name":"Page 3"},
                preset("table"),
                {"op":"create_library_item","page_id":100,"item_id":"icon-plus","x":3,"y":4}
            ]}),
        ),
    )
    .unwrap();
    assert!(project.pages[0].widgets.is_empty());
    assert_eq!(project.pages[1].widgets.len(), 12);
    assert_eq!(project.pages[2].widgets.len(), 1);
    assert_eq!(result["results"][1]["page_id"], 50);
    assert_eq!(result["results"][2]["page_id"], 100);
}

#[test]
fn malformed_library_operations_roll_back_the_entire_batch_and_allocator() {
    let mut project = Project::blank();
    for bad in [
        json!({"item_id":"missing"}),
        json!({"item_id":""}),
        json!({"page_id":999}),
        json!({"x":null}),
        json!({"x":1e100}),
        json!({"x":100001}),
        json!({"y":"NaN"}),
        json!({"width":0}),
        json!({"height":20001}),
        json!({"width":1,"height":1}),
        json!({"texts":vec!["extra";13]}),
        json!({"texts":[123]}),
        json!({"texts":["x".repeat(100001)]}),
        json!({"texts":["汉".repeat(40000)]}),
        json!({"properties":null}),
        json!({"properties":{"id":20}}),
        json!({"properties":{"kind":"Panel"}}),
        json!({"properties":{"groups":[77]}}),
        json!({"properties":{"width":600}}),
        json!({"properties":{"text":"not a common style"}}),
        json!({"properties":{"fill":"red"}}),
        json!({"properties":{"font_size":0}}),
        json!({"properties":{"opacity":2}}),
        json!({"properties":{"locked":"yes"}}),
        json!({"properties":{"vector_points":[]}}),
        json!({"properties":{"text_layout":{"horizontal":0,"vertical":0,"padding":[0,0,0,0],"wrap":false,"bold":true,"unknown":1}}}),
        json!({"extra":true}),
    ] {
        let mut operation = preset("table");
        operation
            .as_object_mut()
            .unwrap()
            .extend(bad.as_object().unwrap().clone());
        let before = project.clone();
        let result = batch(
            &mut project,
            json!([
                {"op":"create_page","name":"Must not survive"},preset("icon-check"),operation
            ]),
        );
        assert!(result.is_err(), "unexpectedly accepted {bad}");
        assert_eq!(project, before, "failed operation leaked changes: {bad}");
    }
    for item_id in ["icon-search", "heading1", "rectangle"] {
        let before = project.clone();
        let mut operation = preset(item_id);
        operation["texts"] = json!(["one", "too many"]);
        assert!(batch(&mut project, json!([operation])).is_err());
        assert_eq!(project, before);
    }
    let before = project.clone();
    assert!(
        batch(
            &mut project,
            json!([preset("table"),{"op":"delete_widget","widget_id":999}])
        )
        .is_err()
    );
    assert_eq!(project, before);
}

#[test]
fn preset_allocation_checks_exact_group_headroom_before_factory_can_overflow() {
    for (item_id, allocated) in [("table", 13u64), ("tree", 6), ("icon-check", 1)] {
        for next_id in [u64::MAX - 1, u64::MAX - allocated] {
            let mut project = Project::blank();
            project.next_id = next_id;
            let before = project.clone();
            assert!(
                batch(&mut project, json!([preset(item_id)]))
                    .unwrap_err()
                    .contains("IDs exhausted")
            );
            assert_eq!(project, before);
        }
        let mut project = Project::blank();
        project.next_id = u64::MAX - allocated - 1;
        batch(&mut project, json!([preset(item_id)])).unwrap();
        assert_eq!(project.next_id, u64::MAX - 1);
        project.validate().unwrap();
    }
}

fn populate(project: &mut Project, page: usize, count: usize) {
    for _ in 0..count {
        let id = project.allocate_id();
        project.pages[page]
            .widgets
            .push(Widget::new(id, WidgetKind::Rectangle, 0.0, 0.0));
    }
}

#[test]
fn expanded_preset_counts_enforce_page_and_project_limits_atomically() {
    let mut project = Project::blank();
    populate(&mut project, 0, 4989);
    let before = project.clone();
    assert!(
        batch(&mut project, json!([preset("table")]))
            .unwrap_err()
            .contains("5000")
    );
    assert_eq!(project, before);
    project.pages[0].widgets.pop();
    batch(&mut project, json!([preset("table")])).unwrap();
    assert_eq!(project.pages[0].widgets.len(), 5000);

    let mut project = Project::blank();
    batch(
        &mut project,
        json!([
            {"op":"create_page","name":"P2"},{"op":"create_page","name":"P3"},
            {"op":"create_page","name":"P4"},{"op":"create_page","name":"P5"}
        ]),
    )
    .unwrap();
    for page in 0..4 {
        populate(&mut project, page, 5000);
    }
    project.validate().unwrap();
    let before = project.clone();
    let mut operation = preset("icon-close");
    operation["page_id"] = json!(project.pages[4].id);
    assert!(
        batch(&mut project, json!([operation]))
            .unwrap_err()
            .contains("20000")
    );
    assert_eq!(project, before);
}

#[test]
fn native_library_creation_permits_layout_vectors_and_template_forms_but_rejects_panels_atomically()
{
    let mut project = Project::blank();
    let result=execute(&mut project,true,0,&[],&call("rpshop_batch_edit",json!({"operations":[
        preset("table"),preset("horizontal-menu"),preset("tree"),preset("heading1"),preset("icon-home"),
        preset("radio"),preset("textarea"),preset("droplist"),preset("listbox")
    ]}))).unwrap();
    assert_eq!(result["requires_native_validation"], true);
    assert!(
        project.pages[0]
            .widgets
            .iter()
            .any(|w| w.text_layout.is_some())
    );
    assert!(
        project.pages[0]
            .widgets
            .iter()
            .any(|w| w.vector_points.is_some())
    );
    let before = project.clone();
    let result = execute(
        &mut project,
        true,
        0,
        &[],
        &call(
            "rpshop_batch_edit",
            json!({"operations":[
                preset("icon-plus"),preset("container")
            ]}),
        ),
    )
    .unwrap_err();
    assert!(result.contains("Panel"));
    assert_eq!(project, before);

    let mut project = Project::blank();
    let layout = json!({"horizontal":0,"vertical":1,"padding":[1,2,3,4],"wrap":false,"bold":true});
    let points = crate::shape_geometry::polygon(&[[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]);
    let result=execute(&mut project,true,0,&[],&call("rpshop_batch_edit",json!({"operations":[
        {"op":"create_widget","kind":"Rectangle","properties":{"vector_points":points,"text_layout":layout}}
    ]}))).unwrap();
    assert_eq!(result["requires_native_validation"], true);
    project.validate().unwrap();
}

#[test]
fn native_source_preview_still_decides_available_templates_without_writing_source_files() {
    // Only caller-selected supported RP8/RP11 fixtures: an installed application's
    // version does not determine the format of its bundled training documents.
    let Some(selected) = std::env::var_os("RPSHOP_NATIVE_V11_FIXTURE")
        .or_else(|| std::env::var_os("RPSHOP_NATIVE_FIXTURE"))
    else {
        return;
    };
    let path = std::path::PathBuf::from(selected);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "Cannot read configured native fixture {}: {error}",
            path.display()
        )
    });
    let native = crate::rp_format::open(&path).unwrap_or_else(|error| {
        panic!(
            "Configured native fixture {} must be a supported RP8/RP11 project: {error:#}",
            path.display()
        )
    });
    for item_id in [
        "table",
        "heading1",
        "icon-search",
        "radio",
        "textarea",
        "droplist",
        "listbox",
    ] {
        let mut candidate = native.project.clone();
        let result = execute(
            &mut candidate,
            true,
            0,
            &[],
            &call("rpshop_batch_edit", json!({"operations":[preset(item_id)]})),
        )
        .unwrap();
        assert_eq!(result["requires_native_validation"], true);
        let template_kind = ITEMS.iter().find(|item| item.id == item_id).unwrap().kind();
        let has_existing_kind = native
            .project
            .pages
            .iter()
            .flat_map(|p| &p.widgets)
            .any(|w| w.kind == template_kind);
        let preview = native.preview_scene(&candidate);
        if has_existing_kind || matches!(item_id, "table" | "heading1" | "icon-search") {
            if let Err(error) = preview {
                panic!("{item_id}: {error:#}");
            }
        } else if let Err(error) = preview {
            assert!(
                format!("{error:#}").contains("模板"),
                "{item_id}: {error:#}"
            );
        }
        assert_eq!(
            native.project.clone(),
            crate::rp_format::open(&path).unwrap().project
        );
    }
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn strict_library_schema_and_runtime_share_all_ids_categories_and_style_fields() {
    let tools = tools();
    let list = tools.as_array().unwrap();
    let read = list
        .iter()
        .find(|tool| tool["name"] == "rpshop_read_library")
        .unwrap();
    assert_eq!(read["annotations"]["readOnlyHint"], true);
    assert_eq!(read["inputSchema"]["additionalProperties"], false);
    assert_eq!(read["inputSchema"]["properties"]["query"]["maxLength"], 256);
    assert_eq!(
        read["inputSchema"]["properties"]["item_id"]["enum"],
        json!(library::item_ids())
    );
    assert_eq!(
        read["inputSchema"]["properties"]["category"]["enum"],
        json!(library::category_ids())
    );
    let batch = list
        .iter()
        .find(|tool| tool["name"] == "rpshop_batch_edit")
        .unwrap();
    let operation = batch["inputSchema"]["properties"]["operations"]["items"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|op| op["properties"]["op"]["const"] == "create_library_item")
        .unwrap();
    assert_eq!(operation["additionalProperties"], false);
    assert_eq!(operation["required"], json!(["item_id", "x", "y", "op"]));
    assert_eq!(
        operation["properties"]["item_id"]["enum"],
        json!(library::item_ids())
    );
    assert_eq!(operation["properties"]["texts"]["maxItems"], 12);
    let style = &operation["properties"]["properties"];
    assert_eq!(style["additionalProperties"], false);
    let names = style["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        names,
        library::STYLE_FIELDS
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
    );
    assert!(
        list.iter()
            .any(|tool| tool["name"] == "rpshop_page_activity"),
        "preserve parent-owned GUI tool"
    );
}
