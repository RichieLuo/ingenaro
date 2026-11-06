//! Live-document automation. The GUI owns permissions, history and persistence;
//! this module never opens a project file or owns a second document snapshot.

mod bridge;
mod edit;
mod library;
mod schema;
mod server;
mod stdio;

pub use bridge::{ToolClient, ToolHost, ToolRequest};
pub use schema::{DESIGN_PAGES_TOOL, EDIT_PAGE_TOOL, MAX_PAGE_SUBAGENTS, tools};
pub use server::{Server, start};
pub use stdio::run_stdio;

use crate::grouping::{Layer, layers};
use crate::model::{Page, Project, Widget};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// One call against the document currently owned by the GUI thread.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    #[serde(default = "empty_object")]
    pub arguments: Value,
}

fn empty_object() -> Value {
    json!({})
}

pub const MAX_ARGUMENT_BYTES: usize = 24 * 1024 * 1024;

pub(crate) fn is_design_tool(name: &str) -> bool {
    schema::ToolSet::Design.supports(name)
}

/// Execute a read, or atomically commit an entire batch after model validation.
/// Native callers must additionally stage the candidate through
/// `NativeDocument::preview_scene` (or a native validate-only API) before keeping
/// it: a boolean cannot describe source bindings, templates, or read-only masters.
/// GUI-only tools deliberately return an error here instead of touching disk.
pub fn execute(
    project: &mut Project,
    native: bool,
    page: usize,
    selection: &[u64],
    call: &ToolCall,
) -> Result<Value, String> {
    execute_inner(project, native, page, selection, call, false)
}

/// Execute using a worker's assigned page, independent of the editor's focus.
pub fn execute_page(
    project: &mut Project,
    native: bool,
    page_id: u64,
    call: &ToolCall,
) -> Result<Value, String> {
    if !schema::ToolSet::PageDesign.supports(&call.name) {
        return Err("此工具不适用于页面设计任务".into());
    }
    if call
        .arguments
        .get("page_id")
        .is_some_and(|id| id.as_u64() != Some(page_id))
    {
        return Err("页面设计任务只能读取或修改分配的页面".into());
    }
    let page = project
        .pages
        .iter()
        .position(|page| page.id == page_id)
        .ok_or("分配的页面已不存在")?;
    execute_inner(project, native, page, &[], call, true)
}

fn execute_inner(
    project: &mut Project,
    native: bool,
    page: usize,
    selection: &[u64],
    call: &ToolCall,
    page_only: bool,
) -> Result<Value, String> {
    if !call.arguments.is_object() {
        return Err("Tool arguments must be an object".into());
    }
    if serde_json::to_vec(&call.arguments)
        .map_err(|e| e.to_string())?
        .len()
        > MAX_ARGUMENT_BYTES
    {
        return Err("Tool arguments exceed the 24 MiB limit".into());
    }
    if call.name == "rpshop_batch_edit" {
        return edit::batch(project, native, page, &call.arguments, page_only)
            .map_err(|e| format!("{e:#}"));
    }
    let args = &call.arguments;
    match call.name.as_str() {
        "rpshop_capabilities" => {
            fields(args, &[])?;
            Ok(json!({
                "native": native,
                "read_tools": schema::read_tools(page_only),
                "library_tool":"rpshop_read_library",
                "library_item_ids":library::item_ids(),
                "library_categories":library::category_ids(),
                "library_note":"The complete GUI component catalog is available via rpshop_read_library and create_library_item. Table (12 editable grouped Rectangle cells), menus and trees are composite presets, not additional WidgetKind variants. Inspect item_id defaults, then use returned real widget/group IDs for later edits.",
                "edit_tool": "rpshop_batch_edit",
                "batch_operations": schema::batch_operations(page_only),
                "limits": {"pages":128,"widgets_per_page":5000,"widgets":20000,
                    "batch_operations":512,"group_depth":32,"argument_bytes":MAX_ARGUMENT_BYTES},
                "native_constraints": {
                    "page_structure":false,"project_rename":false,"page_dimensions":false,
                    "new_panel":false,"cross_page_widget_move":false,
                    "requires_host_native_validation":native,
                    "note":"Native templates, library galleries, read-only masters, source hierarchy and event bindings require GUI-side native patch validation. Authored interactions preserve other source cases."
                },
                "widget_kinds":crate::model::WidgetKind::ALL,
                "actions":["Navigate","ToggleVisibility","SetText"],
                "gui_only_tools":if page_only { &[][..] } else { schema::GUI_TOOLS }
            }))
        }
        "rpshop_read_library" => library::read(args, native).map_err(|e| format!("{e:#}")),
        "rpshop_read_project" => {
            fields(args, &[])?;
            Ok(json!({"project":project,"native":native,
                "current_page_id":project.pages.get(page).map(|p|p.id)}))
        }
        "rpshop_read_pages" => {
            fields(args, &[])?;
            Ok(json!({"pages":project.pages.iter().map(|p|json!({
                "id":p.id,"name":p.name,"width":p.width,"height":p.height,
                "canvas":p.canvas,"background":p.background,"widget_count":p.widgets.len()
            })).collect::<Vec<_>>(),"page_tree":project.page_tree}))
        }
        "rpshop_read_selection" => {
            fields(args, &[])?;
            let current = project
                .pages
                .get(page)
                .ok_or("Current page does not exist")?;
            let widgets = current
                .widgets
                .iter()
                .filter(|w| selection.contains(&w.id))
                .collect::<Vec<_>>();
            Ok(
                json!({"page_id":current.id,"widget_ids":widgets.iter().map(|w|w.id).collect::<Vec<_>>(),"widgets":widgets}),
            )
        }
        "rpshop_read_layers" => {
            fields(args, &["page_id"])?;
            let current = read_page(project, page, args)?;
            Ok(json!({"page_id":current.id,"layers":layer_tree(current, &current.widgets, 0)}))
        }
        "rpshop_read_widgets" | "rpshop_read_styles" => {
            fields(args, &["page_id", "widget_ids"])?;
            let current = read_page(project, page, args)?;
            let ids = optional_ids(args)?;
            if let Some(ids) = &ids {
                for id in ids {
                    if !current.widgets.iter().any(|w| w.id == *id) {
                        return Err(format!("Widget {id} does not exist on page {}", current.id));
                    }
                }
            }
            let widgets = current
                .widgets
                .iter()
                .filter(|w| ids.as_ref().is_none_or(|ids| ids.contains(&w.id)));
            let data = if call.name == "rpshop_read_styles" {
                widgets.map(style).collect::<Vec<_>>()
            } else {
                widgets
                    .map(|w| serde_json::to_value(w).expect("widget serializes"))
                    .collect()
            };
            Ok(json!({"page_id":current.id,"widgets":data}))
        }
        "rpshop_read_interactions" => {
            fields(args, &["page_id", "owner"])?;
            let current = read_page(project, page, args)?;
            let owner = match args.get("owner") {
                None => None,
                Some(Value::Null) => Some(None),
                Some(value) => Some(Some(
                    value
                        .as_u64()
                        .filter(|id| *id > 0)
                        .ok_or("owner must be a positive widget ID or null")?,
                )),
            };
            if let Some(Some(id)) = owner
                && !current.widgets.iter().any(|w| w.id == id)
            {
                return Err(format!(
                    "Interaction owner {id} does not exist on this page"
                ));
            }
            let authored = project
                .interaction_events
                .iter()
                .filter(|e| e.page == current.id && owner.is_none_or(|owner| owner == e.owner))
                .collect::<Vec<_>>();
            let legacy = current
                .widgets
                .iter()
                .filter(|w| owner.is_none_or(|owner| owner == Some(w.id)))
                .map(|w| json!({"owner":w.id,"interaction":w.interaction}))
                .collect::<Vec<_>>();
            Ok(
                json!({"page_id":current.id,"authored":authored,"legacy":legacy,
                "native_source_cases_included":false,
                "note":"Only editor-authored and legacy editor interactions are listed; opaque native source cases remain preserved by the native patcher."}),
            )
        }
        name if schema::GUI_TOOLS.contains(&name) => {
            Err(format!("{name} must be handled by the live GUI host"))
        }
        _ => Err(format!("Unknown RPShop tool: {}", call.name)),
    }
}

fn layer_tree(page: &Page, widgets: &[Widget], depth: usize) -> Vec<Value> {
    layers(widgets, depth)
        .map(|layer| match layer {
            Layer::Group { id, members } => json!({
                "type":"group", "group_id":id, "name":page.group_name(id),
                "widget_count":members.len(), "children":layer_tree(page, members, depth + 1)
            }),
            Layer::Widget(widget) => json!({
                "type":"widget", "widget_id":widget.id, "name":widget.name,
                "kind":widget.kind, "hidden":widget.hidden, "locked":widget.locked
            }),
        })
        .collect()
}

fn read_page<'a>(
    project: &'a Project,
    page: usize,
    args: &Value,
) -> Result<&'a crate::model::Page, String> {
    if let Some(id) = args.get("page_id") {
        let id = id
            .as_u64()
            .filter(|id| *id > 0)
            .ok_or("page_id must be a positive page ID")?;
        project
            .pages
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("Page {id} does not exist"))
    } else {
        project
            .pages
            .get(page)
            .ok_or_else(|| "Current page does not exist".into())
    }
}

fn optional_ids(args: &Value) -> Result<Option<Vec<u64>>, String> {
    args.get("widget_ids")
        .map(|value| {
            let ids: Vec<u64> = serde_json::from_value(value.clone())
                .map_err(|_| "widget_ids must be an array of positive IDs")?;
            if ids.len() > 5000 || ids.contains(&0) {
                return Err("widget_ids must contain at most 5000 positive IDs".into());
            }
            Ok(ids)
        })
        .transpose()
}

pub(super) fn fields(value: &Value, allowed: &[&str]) -> Result<(), String> {
    let object = value.as_object().ok_or("Expected an object")?;
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("Unsupported field: {key}"));
    }
    Ok(())
}

fn style(w: &Widget) -> Value {
    json!({"id":w.id,"fill":w.fill,"stroke":w.stroke,"text_color":w.text_color,
        "font_size":w.font_size,"text_layout":w.text_layout,"radius":w.radius,
        "opacity":w.opacity,"rotation":w.rotation,"hidden":w.hidden,"locked":w.locked})
}

#[cfg(test)]
mod library_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod wait_tests;
