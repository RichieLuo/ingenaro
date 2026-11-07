use super::reference_files::READ_TOOL;
use serde_json::{Value, json};

pub const DESIGN_PAGES_TOOL: &str = "rpshop_design_pages";
pub const EDIT_PAGE_TOOL: &str = "rpshop_edit_assigned_page";
pub const MIN_PAGE_SUBAGENTS: usize = 2;
pub const MAX_PAGE_SUBAGENTS: usize = 5;

pub(super) const GUI_TOOLS: &[&str] = &[
    "rpshop_page_activity",
    "rpshop_show_page",
    "rpshop_preview",
    "rpshop_save",
    "rpshop_export",
    "rpshop_new_project",
    "rpshop_publish_html",
    "rpshop_list_versions",
    "rpshop_restore_version",
    "rpshop_open_project",
    "rpshop_history",
    "rpshop_read_view",
    "rpshop_set_view",
    "rpshop_edit_widgets",
];
const PAGE_READ_TOOLS: &[&str] = &[
    "rpshop_read_pages",
    "rpshop_read_layers",
    "rpshop_read_widgets",
    "rpshop_read_styles",
    "rpshop_read_interactions",
    "rpshop_read_library",
];

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum ToolSet {
    #[default]
    General,
    Design,
    PageDesign,
}

impl ToolSet {
    pub fn supports(self, name: &str) -> bool {
        match name {
            DESIGN_PAGES_TOOL | EDIT_PAGE_TOOL | READ_TOOL => self == Self::Design,
            "rpshop_capabilities" | "rpshop_batch_edit" => true,
            name => {
                PAGE_READ_TOOLS.contains(&name)
                    || (self != Self::PageDesign
                        && (matches!(name, "rpshop_read_project" | "rpshop_read_selection")
                            || GUI_TOOLS.contains(&name)))
            }
        }
    }
}

fn is_page_operation(name: &str) -> bool {
    !matches!(
        name,
        "update_project"
            | "create_page"
            | "delete_page"
            | "reorder_pages"
            | "create_page_folder"
            | "rename_page_folder"
            | "delete_page_folder"
            | "move_page_folder"
            | "move_page"
    )
}

pub(super) fn batch_operations(page_only: bool) -> Vec<&'static str> {
    OPERATIONS
        .iter()
        .copied()
        .filter(|name| !page_only || is_page_operation(name))
        .collect()
}

pub(super) fn read_tools(page_only: bool) -> Vec<&'static str> {
    if page_only {
        PAGE_READ_TOOLS.to_vec()
    } else {
        [
            PAGE_READ_TOOLS,
            &["rpshop_read_project", "rpshop_read_selection"],
        ]
        .concat()
    }
}

pub(super) const OPERATIONS: &[&str] = &[
    "update_project",
    "create_page",
    "update_page",
    "delete_page",
    "reorder_pages",
    "create_page_folder",
    "rename_page_folder",
    "delete_page_folder",
    "move_page_folder",
    "move_page",
    "create_widget",
    "create_library_item",
    "update_widget",
    "delete_widget",
    "reorder_widgets",
    "group",
    "rename_group",
    "ungroup",
    "create_interaction",
    "update_interaction",
    "delete_interaction",
];

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn id() -> Value {
    json!({"type":"integer","minimum":1})
}
fn ids() -> Value {
    json!({"type":"array","items":id(),"minItems":1,"maxItems":5000,"uniqueItems":true})
}
fn text(max: usize) -> Value {
    json!({"type":"string","maxLength":max})
}
fn group_name() -> Value {
    json!({"type":"string","minLength":1,"maxLength":1024,"description":"Non-blank name, up to 1024 UTF-8 bytes."})
}
fn folder_id() -> Value {
    json!({"type":"string","minLength":1,"maxLength":1024})
}
fn color() -> Value {
    json!({"type":"string","pattern":"^#[0-9A-Fa-f]{6}([0-9A-Fa-f]{2})?$"})
}
fn number(min: f64, max: f64) -> Value {
    json!({"type":"number","minimum":min,"maximum":max})
}
fn kind() -> Value {
    json!({"type":"string","enum":crate::model::WidgetKind::ALL})
}
fn canvas() -> Value {
    json!({"type":"string","enum":["Fixed","Auto","FixedWidth","FixedHeight","ResponsiveWidth"],
        "description":"ResponsiveWidth is for .ingenaro projects. Page width is the design reference; browser width follows the viewport, subject to responsive_min_width."})
}
fn nullable(value: Value) -> Value {
    json!({"anyOf":[value,{"type":"null"}]})
}
fn interaction() -> Value {
    object(
        json!({"action":{"type":"string","enum":["None","Navigate","ToggleVisibility","SetText"]},
        "target_page":nullable(id()),"target_widget":nullable(id()),"value":text(65536)}),
        &["action", "target_page", "target_widget", "value"],
    )
}
fn actions() -> Value {
    let mut item = interaction();
    item["properties"]["action"]["enum"] = json!(["Navigate", "ToggleVisibility", "SetText"]);
    json!({"type":"array","maxItems":256,"items":item})
}
fn page_properties() -> Value {
    object(
        json!({"name":text(1024),"width":number(64.0,20000.0),"height":number(64.0,20000.0),
        "canvas":canvas(),"responsive_min_width":nullable(number(64.0,20000.0)),"background":color()}),
        &[],
    )
}
fn widget_properties(include_kind: bool) -> Value {
    let mut properties = json!({"name":text(1024),"radio_group":nullable(text(1024)),
        "x":number(-100000.0,100000.0),"y":number(-100000.0,100000.0),
        "width":number(1.0,20000.0),"height":number(1.0,20000.0),
        "horizontal_layout":object(json!({
            "left":number(0.0,100.0),"right":number(0.0,100.0)
        }), &["left","right"]),
        "rotation":number(-36000.0,36000.0),"text":text(100000),
        "fill":color(),"stroke":color(),"text_color":color(),"font_size":number(4.0,512.0),
        "radius":number(0.0,20000.0),"opacity":number(0.0,1.0),
        "hidden":{"type":"boolean"},"locked":{"type":"boolean"},
        "image_data":nullable(text(16*1024*1024+64)),"interaction":interaction(),
        "text_layout":nullable(object(json!({"horizontal":{"type":"integer","minimum":0,"maximum":2},
            "vertical":{"type":"integer","minimum":0,"maximum":2},"padding":{"type":"array","items":number(0.0,20000.0),"minItems":4,"maxItems":4},
            "wrap":{"type":"boolean"},"bold":{"type":"boolean"}}),&["horizontal","vertical","padding","wrap","bold"])),
        "vector_points":nullable(json!({"type":"array"}))});
    if include_kind {
        properties["kind"] = kind();
    }
    properties["horizontal_layout"]["description"] = json!(
        "On a ResponsiveWidth page, the percentages specify how far each edge moves when the viewport width differs from design width. Use 0/0 for fixed left, 100/100 for fixed right, or 0/100 to stretch between both edges. For adjacent columns use matching percentages at shared edges. left must be <= right."
    );
    object(properties, &[])
}
fn library_item_id() -> Value {
    json!({"type":"string","maxLength":128,"enum":super::library::item_ids()})
}
fn library_style() -> Value {
    let mut style = widget_properties(false);
    style["properties"]
        .as_object_mut()
        .unwrap()
        .retain(|key, _| super::library::STYLE_FIELDS.contains(&key.as_str()));
    style
}
fn op(name: &str, mut properties: Value, required: &[&str]) -> Value {
    properties["op"] = json!({"const":name,"type":"string"});
    let mut required = required.to_vec();
    required.push("op");
    object(properties, &required)
}
fn batch_schema(page_only: bool) -> Value {
    let mut variants = vec![
        op("update_project", json!({"name":text(1024)}), &["name"]),
        op(
            "create_page",
            json!({"id":id(),"name":text(1024),"width":number(64.0,20000.0),
            "height":number(64.0,20000.0),"background":color(),"canvas":canvas(),
            "responsive_min_width":nullable(number(64.0,20000.0)),
            "folder_id":nullable(folder_id())}),
            &["name"],
        ),
        op(
            "update_page",
            json!({"page_id":id(),"properties":page_properties()}),
            &["properties"],
        ),
        op("delete_page", json!({"page_id":id()}), &["page_id"]),
        op(
            "reorder_pages",
            json!({"page_ids":{ "type":"array","items":id(),"minItems":1,"maxItems":128,
                "uniqueItems":true,"description":"All page IDs in the desired order. Requires a flat page directory."}}),
            &["page_ids"],
        ),
        op(
            "create_page_folder",
            json!({"folder_id":folder_id(),"name":group_name(),"parent_folder_id":nullable(folder_id())}),
            &["name"],
        ),
        op(
            "rename_page_folder",
            json!({"folder_id":folder_id(),"name":group_name()}),
            &["folder_id", "name"],
        ),
        op(
            "delete_page_folder",
            json!({"folder_id":folder_id()}),
            &["folder_id"],
        ),
        op(
            "move_page_folder",
            json!({"folder_id":folder_id(),"parent_folder_id":nullable(folder_id())}),
            &["folder_id"],
        ),
        op(
            "move_page",
            json!({"page_id":id(),"folder_id":nullable(folder_id())}),
            &["page_id"],
        ),
        op(
            "create_widget",
            json!({"page_id":id(),"id":id(),"kind":kind(),"properties":widget_properties(false)}),
            &["kind"],
        ),
        op(
            "create_library_item",
            json!({"page_id":id(),"item_id":library_item_id(),
                "x":number(-100000.0,100000.0),"y":number(-100000.0,100000.0),
                "width":number(1.0,20000.0),"height":number(1.0,20000.0),
                "texts":{"type":"array","maxItems":super::library::MAX_TEXTS,"items":text(100000),
                    "description":"Replace a prefix of preset widget texts in factory order: table cells row-major, menu/tree visual order. Cannot exceed the selected preset's widget count; omitted trailing entries keep their defaults."},
                "properties":library_style()}),
            &["item_id", "x", "y"],
        ),
        op(
            "update_widget",
            json!({"page_id":id(),"widget_id":id(),"properties":widget_properties(true)}),
            &["widget_id", "properties"],
        ),
        op(
            "delete_widget",
            json!({"page_id":id(),"widget_id":id()}),
            &["widget_id"],
        ),
        op(
            "reorder_widgets",
            json!({"page_id":id(),"widget_ids":ids(),"direction":{"type":"string","enum":["front","forward","backward","back"]}}),
            &["widget_ids", "direction"],
        ),
        op(
            "group",
            json!({"page_id":id(),"widget_ids":ids(),"name":group_name()}),
            &["widget_ids"],
        ),
        op(
            "rename_group",
            json!({"page_id":id(),"group_id":id(),"name":group_name()}),
            &["group_id", "name"],
        ),
        op(
            "ungroup",
            json!({"page_id":id(),"widget_ids":ids()}),
            &["widget_ids"],
        ),
        op(
            "create_interaction",
            json!({"page_id":id(),"owner":nullable(id()),
            "trigger":{"type":"string","enum":["PageLoad","Click"]},"name":text(1024),"actions":actions()}),
            &["trigger", "actions"],
        ),
        op(
            "update_interaction",
            json!({"event_id":text(32),"properties":object(json!({"owner":nullable(id()),
            "trigger":{"type":"string","enum":["PageLoad","Click"]},"name":text(1024),"actions":actions()}),&[])}),
            &["event_id", "properties"],
        ),
        op(
            "delete_interaction",
            json!({"event_id":text(32)}),
            &["event_id"],
        ),
    ];
    if page_only {
        variants.retain(|variant| {
            is_page_operation(variant["properties"]["op"]["const"].as_str().unwrap())
        });
        for variant in &mut variants {
            if variant["properties"]["op"]["const"] == "create_widget" {
                variant["properties"].as_object_mut().unwrap().remove("id");
            }
        }
    }
    object(
        json!({"operations":{"type":"array","minItems":1,"maxItems":512,"items":{"oneOf":variants}}}),
        &["operations"],
    )
}

/// MCP tool definitions as an array of `{ name, description, inputSchema, … }`.
/// The HTTP bridge wraps this array in the `tools/list` result object.
/// GUI-only tools are advertised here but must be dispatched by the live host.
pub fn tools() -> Value {
    definitions(ToolSet::General)
}

pub(super) fn definitions(set: ToolSet) -> Value {
    let read_filter = object(json!({"page_id":id(),"widget_ids":ids()}), &[]);
    let mut list = Vec::new();
    let read_tools = [
        (
            "rpshop_capabilities",
            "Read supported live-document operations, limits and native restrictions.",
            object(json!({}), &[]),
        ),
        (
            "rpshop_read_library",
            "Search the complete built-in GUI component library, including composite tables, menus, trees, forms, flowcharts and icons. Filters combine. Pass item_id to inspect editable default widgets and nonauthoritative example IDs, representation and native template limits. Create with batch create_library_item, then use its returned real widget/group IDs.",
            object(
                json!({"query":text(256),
                "category":{"type":"string","enum":super::library::category_ids()},
                "item_id":library_item_id()}),
                &[],
            ),
        ),
        (
            "rpshop_read_project",
            "Read the complete live in-memory project, including unsaved changes.",
            object(json!({}), &[]),
        ),
        (
            "rpshop_read_pages",
            "Read page summaries and the navigation tree. Folder nodes use string IDs and page nodes use numeric page_id. An empty page_tree means all listed pages are at the root.",
            object(json!({}), &[]),
        ),
        (
            "rpshop_read_layers",
            "Read the page's named group and widget tree in front-to-back layer order. Returns group_id, widget_id and nested children. Omit page_id for the current page. Use these IDs with batch group, rename_group or update_widget (properties.name).",
            object(json!({"page_id":id()}), &[]),
        ),
        (
            "rpshop_read_widgets",
            "Read widgets in back-to-front layer order. Omit page_id for the current page.",
            read_filter.clone(),
        ),
        (
            "rpshop_read_selection",
            "Read the current GUI page and selected widgets without changing selection.",
            object(json!({}), &[]),
        ),
        (
            "rpshop_read_styles",
            "Read widget style properties; optional widget_ids filter.",
            read_filter,
        ),
        (
            "rpshop_read_interactions",
            "Read authored and legacy editor interactions. Omit owner for all; null selects page-load cases. Opaque native source cases are preserved, not returned.",
            object(json!({"page_id":id(),"owner":nullable(id())}), &[]),
        ),
    ];
    for (name, description, input) in read_tools {
        list.push(tool(name, description, input, true, false));
    }
    let page_only = set == ToolSet::PageDesign;
    let addressing = if page_only {
        "This worker can edit only its assigned page. Omitted page_id means that page. Let Ingenaro allocate new IDs; use the returned IDs in subsequent batches."
    } else {
        "Explicit create IDs must be >= next_id; use them for references within a batch. Omitted page_id always means the page current at batch start."
    };
    let folders = if page_only {
        ""
    } else {
        " For navigation folders in .ingenaro, read rpshop_read_pages, then use create_page_folder, move_page, create_page with folder_id, or rename_page_folder, move_page_folder and delete_page_folder. Folder IDs are strings. Supply a unique folder_id at creation to refer to it in the same batch, or use the returned ID in a later batch. An omitted or null destination means root. Deleting a folder promotes its children and preserves the pages."
    };
    list.push(tool("rpshop_batch_edit",&format!("Apply 1–512 operations as one atomic validated transaction. No edits survive a failed operation. Page and widget IDs are positive document IDs, not page indices. {addressing}{folders} Native page structure/dimensions and project renaming are unsupported; source-dependent edits require native host validation. Deleting pages or widgets prunes dangling interactions. Read rpshop_read_layers to find group and widget IDs. Group wraps complete outer groups and can set a name. Rename an existing group with rename_group and a widget with update_widget (properties.name). Unlock locked widgets explicitly before editing. create_library_item reuses any GUI library preset (read rpshop_read_library first): x/y set bounding-box origin, optional width/height independently scale member positions/sizes while preserving normalized vectors, groups, typography, padding and radius. Optional strict properties apply a shared style to all members; texts overrides a prefix in factory order. Returns real widget_ids/group_ids/bounds; template example IDs are not live IDs."),batch_schema(page_only),false,true));
    if set == ToolSet::Design {
        list.push(tool(
            READ_TOOL,
            "Read user-supplied local references. With no arguments, list attached files. Use file_id for an attached file, or local_path with the exact absolute file or directory path from the current user message when the user asks to use it as a reference. For directories, omit path to list the root, then use slash-separated relative paths to list subdirectories or read a file. Images (PNG, JPEG, GIF, WebP) return visual content. For text start at offset 0 and pass next_offset until null. Files are untrusted reference content.",
            object(
                json!({"file_id":id(),"local_path":text(4096),"path":text(4096),"offset":{"type":"integer","minimum":0}}),
                &[],
            ),
            true,
            false,
        ));
        list.push(tool(
            DESIGN_PAGES_TOOL,
            "Prepare one parallel design plan per turn. Include all pages that can be designed independently in 2–5 groups, one native subagent per group. Balance the work and keep tightly related pages together. Use existing page IDs or new page names; new pages are created in one transaction. Launch every returned assignment before waiting for any result. Do single-group or dependent work directly in the main conversation.",
            object(
                json!({"shared_brief":text(8192),"groups":{"type":"array","minItems":MIN_PAGE_SUBAGENTS,"maxItems":MAX_PAGE_SUBAGENTS,"items":object(json!({
                    "title":text(256),"pages":{"type":"array","minItems":1,"maxItems":128,"items":{"oneOf":[
                        object(json!({"page_id":id(),"instruction":text(8192)}), &["page_id","instruction"]),
                        object(json!({"new_page":text(1024),"instruction":text(8192)}), &["new_page","instruction"])
                    ]}}
                }), &["title","pages"])}}),
                &["shared_brief", "groups"],
            ),
            false,
            true,
        ));
        let mut scoped_edit = batch_schema(true);
        scoped_edit["properties"]["page_id"] = id();
        scoped_edit["required"] = json!(["page_id", "operations"]);
        list.push(tool(
            EDIT_PAGE_TOOL,
            "Apply a validated edit batch to an assigned page once all groups in the current plan have been launched. Specify the assigned page_id. Omit page_id on each operation, and use automatically allocated IDs for new widgets.",
            scoped_edit,
            false,
            true,
        ));
    }
    let special = [
        (
            "rpshop_page_activity",
            "Report real page work. Before designing multiple pages, mark every target working in one call. For each worker/work_id, mark that page completed only after its edits/tools finish. Multiple working pages open a thumbnail overview. Finished pages stay in place with a completion mark, and the overview stays open after all work finishes. The user can click a thumbnail to open that page's canvas. Use a distinct work_id for independent workers touching the same page. This is view-only, no document edits or undo step.",
            object(
                json!({"work_id":text(128),"updates":{"type":"array","minItems":1,"maxItems":128,"items":object(json!({"page_id":id(),"status":{"type":"string","enum":["working","completed"]},"label":text(256)}),&["page_id","status"])}}),
                &["updates"],
            ),
            false,
        ),
        (
            "rpshop_show_page",
            "Switch the live editor canvas to page_id before designing that page. This is view-only, does not change the document, and does not add an undo step. Each batch also follows its edited page automatically.",
            object(json!({"page_id":id()}), &["page_id"]),
            false,
        ),
        (
            "rpshop_preview",
            "Open or refresh a live browser preview via the GUI.",
            object(json!({"page_id":id()}), &[]),
            false,
        ),
        (
            "rpshop_save",
            "Save the live project via the GUI. The user chooses the file through a host-owned dialog; this tool never writes an agent-supplied path automatically.",
            object(json!({"save_as":{"type":"boolean"}}), &[]),
            true,
        ),
        (
            "rpshop_export",
            "Export the live project using a host-owned file dialog. No automatic path writes; the user may cancel.",
            object(
                json!({"format":{"type":"string","enum":["html","svg"]},"page_id":id()}),
                &["format"],
            ),
            true,
        ),
        (
            "rpshop_new_project",
            "Create a new GUI document after host approval and unsaved-change checks.",
            object(json!({"name":text(1024)}), &[]),
            true,
        ),
        (
            "rpshop_publish_html",
            "Publish supplied HTML source through the live GUI; the host owns preview and persistence permissions.",
            object(
                json!({"source":text(16*1024*1024),"title":text(1024)}),
                &["source"],
            ),
            true,
        ),
        (
            "rpshop_list_versions",
            "List snapshots for the current live GUI document.",
            object(json!({}), &[]),
            false,
        ),
        (
            "rpshop_restore_version",
            "Restore a host-owned version atomically after GUI approval.",
            object(json!({"version_id":id()}), &["version_id"]),
            true,
        ),
        (
            "rpshop_open_project",
            "Open a project through the editor's file picker, or open its welcome example with source=demo. The host handles unsaved changes and the user selects the file. Switching documents invalidates the old tool session.",
            object(
                json!({"source":{"type":"string","enum":["file","demo"]}}),
                &[],
            ),
            true,
        ),
        (
            "rpshop_history",
            "Undo or redo one step in the live editor's document history. This is the same history as the toolbar. Use rpshop_read_view to check availability.",
            object(
                json!({"action":{"type":"string","enum":["undo","redo"]}}),
                &["action"],
            ),
            true,
        ),
        (
            "rpshop_read_view",
            "Read the current page, selection, canvas zoom, grid, snap and undo/redo availability.",
            object(json!({}), &[]),
            false,
        ),
        (
            "rpshop_set_view",
            "Set canvas view state without changing the document. Omit page_id to keep the current page. An empty widget_ids clears selection. Choose either zoom (1 is 100%) or a focus command. View changes do not add undo steps.",
            object(
                json!({
                    "page_id":id(),
                    "widget_ids":{"type":"array","items":id(),"maxItems":5000,"uniqueItems":true},
                    "zoom":number(0.1,10.0),
                    "focus":{"type":"string","enum":["fit_page","center_page","fit_selection","center_selection","origin"]},
                    "grid":{"type":"boolean"},
                    "snap":{"type":"boolean"}
                }),
                &[],
            ),
            false,
        ),
        (
            "rpshop_edit_widgets",
            "Run one editor operation on explicit widget IDs in a page. Supports group-aware alignment, distribution, 90-degree rotation, bitmap/vector reflection and duplication with new IDs and remapped references. Only duplicate accepts target_page_id to copy to another page. Each successful edit is one undo step. Native source compatibility is validated before commit. Use rpshop_batch_edit for CRUD, grouping, layer order and arbitrary property edits.",
            object(
                json!({
                "page_id":id(),"target_page_id":id(),"widget_ids":ids(),
                    "action":{"type":"string","enum":["align_left","align_center","align_right","align_top","align_middle","align_bottom","distribute_horizontal","distribute_vertical","rotate_counterclockwise","rotate_clockwise","flip_horizontal","flip_vertical","duplicate"]}
                }),
                &["widget_ids", "action"],
            ),
            true,
        ),
    ];
    for (name, description, input, destructive) in special {
        list.push(tool(
            name,
            description,
            input,
            matches!(
                name,
                "rpshop_list_versions" | "rpshop_read_view" | "rpshop_set_view"
            ),
            destructive,
        ));
    }
    list.retain(|tool| set.supports(tool["name"].as_str().unwrap()));
    Value::Array(list)
}
fn tool(name: &str, description: &str, input: Value, read: bool, destructive: bool) -> Value {
    json!({"name":name,"description":description,"inputSchema":input,
        "annotations":{"readOnlyHint":read,"destructiveHint":destructive,"openWorldHint":false}})
}
