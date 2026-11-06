use std::collections::BTreeSet;

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{grouping, model::*, selection::LayerMove};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Batch {
    operations: Vec<Operation>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    UpdateProject {
        name: String,
    },
    CreatePage {
        id: Option<u64>,
        name: String,
        width: Option<f32>,
        height: Option<f32>,
        background: Option<String>,
        canvas: Option<CanvasMode>,
    },
    UpdatePage {
        page_id: Option<u64>,
        properties: Value,
    },
    DeletePage {
        page_id: u64,
    },
    ReorderPages {
        page_ids: Vec<u64>,
    },
    CreateWidget {
        page_id: Option<u64>,
        id: Option<u64>,
        kind: WidgetKind,
        #[serde(default = "super::empty_object")]
        properties: Value,
    },
    CreateLibraryItem {
        page_id: Option<u64>,
        item_id: String,
        x: f32,
        y: f32,
        width: Option<f32>,
        height: Option<f32>,
        texts: Option<Vec<String>>,
        #[serde(default = "super::empty_object")]
        properties: Value,
    },
    UpdateWidget {
        page_id: Option<u64>,
        widget_id: u64,
        properties: Value,
    },
    DeleteWidget {
        page_id: Option<u64>,
        widget_id: u64,
    },
    ReorderWidgets {
        page_id: Option<u64>,
        widget_ids: Vec<u64>,
        direction: String,
    },
    Group {
        page_id: Option<u64>,
        widget_ids: Vec<u64>,
        name: Option<String>,
    },
    RenameGroup {
        page_id: Option<u64>,
        group_id: u64,
        name: String,
    },
    Ungroup {
        page_id: Option<u64>,
        widget_ids: Vec<u64>,
    },
    CreateInteraction {
        page_id: Option<u64>,
        owner: Option<u64>,
        trigger: EventTrigger,
        name: Option<String>,
        actions: Vec<Interaction>,
    },
    UpdateInteraction {
        event_id: String,
        properties: Value,
    },
    DeleteInteraction {
        event_id: String,
    },
}

pub(super) fn batch(
    project: &mut Project,
    native: bool,
    page: usize,
    args: &Value,
    page_only: bool,
) -> Result<Value> {
    let batch: Batch = serde_json::from_value(args.clone()).context("Invalid batch arguments")?;
    ensure!(
        !batch.operations.is_empty() && batch.operations.len() <= 512,
        "A batch must contain 1–512 operations"
    );
    // Preflight the complete batch before allocating IDs or cloning any document.
    if native {
        for operation in &batch.operations {
            native_preflight(operation)?;
        }
    }
    project.validate().context("Current document is invalid")?;
    let default_page = project
        .pages
        .get(page)
        .context("Current page does not exist")?
        .id;
    let mut candidate = project.clone();
    let mut results = Vec::with_capacity(batch.operations.len());
    for (index, operation) in batch.operations.into_iter().enumerate() {
        if page_only {
            require_page_scope(&candidate, default_page, &operation)
                .with_context(|| format!("Batch operation {} failed", index + 1))?;
        }
        results.push(
            apply(&mut candidate, default_page, operation)
                .with_context(|| format!("Batch operation {} failed", index + 1))?,
        );
    }
    if page_only {
        // Deleting a widget may prune references. Reject any such side effects
        // on another page before committing the candidate.
        ensure!(
            candidate
                .pages
                .iter()
                .filter(|p| p.id != default_page)
                .eq(project.pages.iter().filter(|p| p.id != default_page))
                && candidate
                    .interaction_events
                    .iter()
                    .filter(|e| e.page != default_page)
                    .eq(project
                        .interaction_events
                        .iter()
                        .filter(|e| e.page != default_page)),
            "The batch would change content outside the assigned page"
        );
    }
    candidate
        .validate()
        .context("Batch validation failed; no edits were committed")?;
    let changed = candidate != *project;
    *project = candidate;
    Ok(json!({"changed":changed,"results":results,"requires_native_validation":native}))
}

fn require_page_scope(project: &Project, assigned: u64, operation: &Operation) -> Result<()> {
    let page_id = match operation {
        Operation::CreateWidget { page_id, id, .. } => {
            ensure!(
                id.is_none(),
                "Let RPShop allocate IDs in parallel page design"
            );
            *page_id
        }
        Operation::UpdatePage { page_id, .. }
        | Operation::CreateLibraryItem { page_id, .. }
        | Operation::UpdateWidget { page_id, .. }
        | Operation::DeleteWidget { page_id, .. }
        | Operation::ReorderWidgets { page_id, .. }
        | Operation::Group { page_id, .. }
        | Operation::RenameGroup { page_id, .. }
        | Operation::Ungroup { page_id, .. }
        | Operation::CreateInteraction { page_id, .. } => *page_id,
        Operation::UpdateInteraction { event_id, .. }
        | Operation::DeleteInteraction { event_id } => {
            ensure!(
                project
                    .interaction_events
                    .iter()
                    .any(|event| event.id == *event_id && event.page == assigned),
                "Interaction does not exist on the assigned page"
            );
            None
        }
        Operation::UpdateProject { .. }
        | Operation::CreatePage { .. }
        | Operation::DeletePage { .. }
        | Operation::ReorderPages { .. } => bail!("Page workers cannot change project structure"),
    };
    ensure!(
        page_id.is_none_or(|id| id == assigned),
        "Cannot edit outside the assigned page"
    );
    Ok(())
}

fn native_preflight(operation: &Operation) -> Result<()> {
    match operation {
        Operation::UpdateProject { .. } => {
            bail!("Native project names come from the filename; use GUI Save As")
        }
        Operation::CreatePage { .. }
        | Operation::DeletePage { .. }
        | Operation::ReorderPages { .. } => {
            bail!("Native documents do not support creating, deleting or reordering pages")
        }
        Operation::UpdatePage { properties, .. } => {
            super::fields(properties, &["name", "background"]).map_err(anyhow::Error::msg)?;
        }
        Operation::CreateWidget {
            kind: WidgetKind::Panel,
            ..
        } => bail!("Native documents do not support creating Panel containers"),
        Operation::CreateLibraryItem { item_id, .. } => super::library::native_preflight(item_id)?,
        Operation::CreateWidget { properties, .. } | Operation::UpdateWidget { properties, .. } => {
            let fields = properties
                .as_object()
                .context("Widget properties must be an object")?;
            // Added presets use exactly the same layout/vector metadata as GUI
            // creation. Existing source support is decided by host.preview_scene,
            // not by a blanket prohibition based only on the native flag.
            if let Some(kind) = fields.get("kind") {
                let kind: WidgetKind = serde_json::from_value(kind.clone())?;
                ensure!(
                    !matches!(
                        kind,
                        WidgetKind::Panel
                            | WidgetKind::Radio
                            | WidgetKind::TextArea
                            | WidgetKind::Droplist
                            | WidgetKind::ListBox
                    ),
                    "Native conversion to Panel or template-only form types is unsupported"
                );
            }
            if let Some(interaction) = fields.get("interaction") {
                let interaction: Interaction = serde_json::from_value(interaction.clone())?;
                ensure!(
                    matches!(interaction.action, Action::None | Action::Navigate),
                    "Use authored interactions for native SetText or ToggleVisibility actions"
                );
            }
            ensure!(
                !fields.get("image_data").is_some_and(Value::is_null),
                "Native image data cannot be removed through automation"
            );
        }
        _ => {}
    }
    Ok(())
}

const PAGE_FIELDS: &[&str] = &["name", "width", "height", "canvas", "background"];
const WIDGET_FIELDS: &[&str] = &[
    "name",
    "kind",
    "radio_group",
    "x",
    "y",
    "width",
    "height",
    "rotation",
    "text",
    "fill",
    "stroke",
    "text_color",
    "font_size",
    "text_layout",
    "radius",
    "opacity",
    "hidden",
    "locked",
    "interaction",
    "image_data",
    "vector_points",
];

pub(super) fn patch<T: serde::Serialize + serde::de::DeserializeOwned>(
    target: &mut T,
    properties: &Value,
    allowed: &[&str],
) -> Result<()> {
    super::fields(properties, allowed).map_err(anyhow::Error::msg)?;
    let mut value = serde_json::to_value(&*target)?;
    for (key, value_to_set) in properties.as_object().unwrap() {
        value[key] = value_to_set.clone();
    }
    *target = serde_json::from_value(value).context("Invalid property value")?;
    Ok(())
}

fn page_index(project: &Project, default_page: u64, id: Option<u64>) -> Result<usize> {
    let id = id.unwrap_or(default_page);
    project
        .pages
        .iter()
        .position(|p| p.id == id)
        .with_context(|| format!("Page {id} does not exist"))
}

fn allocate(project: &mut Project, requested: Option<u64>) -> Result<u64> {
    let id = requested.unwrap_or(project.next_id);
    ensure!(
        id >= project.next_id,
        "Explicit IDs must be at least next_id ({})",
        project.next_id
    );
    let next = id
        .checked_add(1)
        .filter(|next| *next < u64::MAX)
        .context("Document IDs exhausted")?;
    project.next_id = next;
    Ok(id)
}

fn selected(project: &Project, page: usize, ids: Vec<u64>) -> Result<BTreeSet<u64>> {
    ensure!(
        !ids.is_empty() && ids.len() <= 5000,
        "Select 1–5000 widget IDs"
    );
    let set = ids.iter().copied().collect::<BTreeSet<_>>();
    ensure!(set.len() == ids.len(), "Duplicate widget IDs in selection");
    for id in &set {
        let widget = project.pages[page]
            .widgets
            .iter()
            .find(|w| w.id == *id)
            .with_context(|| format!("Widget {id} does not exist on this page"))?;
        ensure!(
            !widget.locked,
            "Widget {id} is locked; explicitly unlock it first"
        );
    }
    Ok(set)
}

fn apply(project: &mut Project, default_page: u64, operation: Operation) -> Result<Value> {
    match operation {
        Operation::UpdateProject { name } => {
            project.name = name;
            Ok(json!({"op":"update_project"}))
        }
        Operation::CreatePage {
            id,
            name,
            width,
            height,
            background,
            canvas,
        } => {
            let id = allocate(project, id)?;
            ensure!(project.pages.len() < 128, "Page count exceeds 128");
            project.pages.push(Page {
                id,
                name,
                width: width.unwrap_or(1280.0),
                height: height.unwrap_or(800.0),
                background: background.unwrap_or_else(|| "#FFFFFF".into()),
                canvas: canvas.unwrap_or_default(),
                widgets: Vec::new(),
                group_names: Default::default(),
            });
            if !project.page_tree.is_empty() {
                project.page_tree.push(PageTreeNode::Page {
                    page_id: id,
                    children: Vec::new(),
                });
            }
            Ok(json!({"op":"create_page","page_id":id}))
        }
        Operation::UpdatePage {
            page_id,
            properties,
        } => {
            let index = page_index(project, default_page, page_id)?;
            patch(&mut project.pages[index], &properties, PAGE_FIELDS)?;
            Ok(json!({"op":"update_page","page_id":project.pages[index].id}))
        }
        Operation::DeletePage { page_id } => {
            ensure!(project.pages.len() > 1, "Cannot delete the last page");
            let index = page_index(project, default_page, Some(page_id))?;
            project.pages.remove(index);
            remove_tree_page(&mut project.page_tree, page_id);
            prune_references(project);
            Ok(json!({"op":"delete_page","page_id":page_id}))
        }
        Operation::ReorderPages { page_ids } => {
            ensure!(
                project.page_tree.is_empty(),
                "Page reordering is not supported for documents with a folder tree"
            );
            let expected = project.pages.iter().map(|p| p.id).collect::<BTreeSet<_>>();
            ensure!(
                page_ids.len() == project.pages.len()
                    && page_ids.iter().copied().collect::<BTreeSet<_>>() == expected,
                "page_ids must contain every page exactly once"
            );
            project
                .pages
                .sort_by_key(|p| page_ids.iter().position(|id| *id == p.id).unwrap());
            Ok(json!({"op":"reorder_pages","page_ids":page_ids}))
        }
        Operation::CreateWidget {
            page_id,
            id,
            kind,
            properties,
        } => {
            let index = page_index(project, default_page, page_id)?;
            let id = allocate(project, id)?;
            ensure!(
                project.pages[index].widgets.len() < 5000,
                "Page widget count exceeds 5000"
            );
            let mut widget = Widget::new(id, kind, 0.0, 0.0);
            // kind is a required top-level input; prevent contradictory properties.
            let fields = WIDGET_FIELDS
                .iter()
                .copied()
                .filter(|field| *field != "kind")
                .collect::<Vec<_>>();
            patch(&mut widget, &properties, &fields)?;
            project.pages[index].widgets.push(widget);
            Ok(json!({"op":"create_widget","page_id":project.pages[index].id,"widget_id":id}))
        }
        Operation::CreateLibraryItem {
            page_id,
            item_id,
            x,
            y,
            width,
            height,
            texts,
            properties,
        } => {
            let index = page_index(project, default_page, page_id)?;
            super::library::create(
                project,
                index,
                &item_id,
                x,
                y,
                width,
                height,
                texts.as_deref(),
                &properties,
            )
        }
        Operation::UpdateWidget {
            page_id,
            widget_id,
            properties,
        } => {
            let index = page_index(project, default_page, page_id)?;
            let widget = project.pages[index]
                .widgets
                .iter_mut()
                .find(|w| w.id == widget_id)
                .context("Widget does not exist on this page")?;
            if widget.locked {
                ensure!(
                    properties == json!({"locked":false}),
                    "Widget {widget_id} is locked; explicitly unlock it first"
                );
            }
            patch(widget, &properties, WIDGET_FIELDS)?;
            Ok(json!({"op":"update_widget","widget_id":widget_id}))
        }
        Operation::DeleteWidget { page_id, widget_id } => {
            let index = page_index(project, default_page, page_id)?;
            selected(project, index, vec![widget_id])?;
            let page = &mut project.pages[index];
            page.widgets.retain(|w| w.id != widget_id);
            page.prune_group_names();
            prune_references(project);
            Ok(json!({"op":"delete_widget","widget_id":widget_id}))
        }
        Operation::ReorderWidgets {
            page_id,
            widget_ids,
            direction,
        } => {
            let index = page_index(project, default_page, page_id)?;
            let ids = selected(project, index, widget_ids)?;
            let movement = match direction.as_str() {
                "front" => LayerMove::ToFront,
                "forward" => LayerMove::Forward,
                "backward" => LayerMove::Backward,
                "back" => LayerMove::ToBack,
                _ => bail!("direction must be front, forward, backward or back"),
            };
            let changed = grouping::reorder(&mut project.pages[index], &ids, movement);
            // No-op at an edge is allowed, but mismatched group scopes are not.
            if !changed {
                validate_reorder_scope(&project.pages[index], &ids)?;
            }
            Ok(json!({"op":"reorder_widgets","changed":changed}))
        }
        Operation::Group {
            page_id,
            widget_ids,
            name,
        } => {
            let index = page_index(project, default_page, page_id)?;
            let ids = selected(project, index, widget_ids)?;
            let (group_id, ids) = grouping::group_named(project, index, &ids, name.as_deref())?;
            let page = &project.pages[index];
            Ok(json!({"op":"group","page_id":page.id,"group_id":group_id,
                "name":page.group_name(group_id),"widget_ids":ids}))
        }
        Operation::RenameGroup {
            page_id,
            group_id,
            name,
        } => {
            let index = page_index(project, default_page, page_id)?;
            let page = &mut project.pages[index];
            grouping::rename_group(page, group_id, &name)?;
            Ok(json!({"op":"rename_group","page_id":page.id,"group_id":group_id,"name":name}))
        }
        Operation::Ungroup {
            page_id,
            widget_ids,
        } => {
            let index = page_index(project, default_page, page_id)?;
            let ids = selected(project, index, widget_ids)?;
            let ids = grouping::ungroup(project, index, &ids)?;
            Ok(json!({"op":"ungroup","widget_ids":ids}))
        }
        Operation::CreateInteraction {
            page_id,
            owner,
            trigger,
            name,
            actions,
        } => {
            let index = page_index(project, default_page, page_id)?;
            let mut event = AuthoredEvent::new(project.pages[index].id, owner, trigger);
            if let Some(name) = name {
                event.name = name;
            }
            event.actions = actions;
            let id = event.id.clone();
            project.interaction_events.push(event);
            Ok(json!({"op":"create_interaction","event_id":id}))
        }
        Operation::UpdateInteraction {
            event_id,
            properties,
        } => {
            let event = project
                .interaction_events
                .iter_mut()
                .find(|e| e.id == event_id)
                .context("Authored interaction does not exist")?;
            patch(event, &properties, &["owner", "trigger", "name", "actions"])?;
            Ok(json!({"op":"update_interaction","event_id":event_id}))
        }
        Operation::DeleteInteraction { event_id } => {
            let index = project
                .interaction_events
                .iter()
                .position(|e| e.id == event_id)
                .context("Authored interaction does not exist")?;
            project.interaction_events.remove(index);
            Ok(json!({"op":"delete_interaction","event_id":event_id}))
        }
    }
}

fn validate_reorder_scope(page: &Page, ids: &BTreeSet<u64>) -> Result<()> {
    let mut scope: Option<Vec<u64>> = None;
    for widget in page.widgets.iter().filter(|w| ids.contains(&w.id)) {
        let depth = widget
            .groups
            .iter()
            .position(|group| {
                page.widgets
                    .iter()
                    .filter(|w| w.groups.contains(group))
                    .all(|w| ids.contains(&w.id))
            })
            .unwrap_or(widget.groups.len());
        let path = &widget.groups[..depth];
        ensure!(
            scope.as_ref().is_none_or(|scope| scope == path),
            "Cannot reorder widgets across different group scopes"
        );
        scope = Some(path.to_vec());
    }
    Ok(())
}

fn remove_tree_page(nodes: &mut Vec<PageTreeNode>, id: u64) {
    let mut result = Vec::new();
    for mut node in std::mem::take(nodes) {
        let children = match &mut node {
            PageTreeNode::Folder { children, .. } | PageTreeNode::Page { children, .. } => children,
        };
        remove_tree_page(children, id);
        if matches!(node,PageTreeNode::Page {page_id,..} if page_id==id) {
            if let PageTreeNode::Page { children, .. } = node {
                result.extend(children);
            }
        } else {
            result.push(node);
        }
    }
    *nodes = result;
}

fn prune_references(project: &mut Project) {
    project.prune_interaction_references();
    let pages = project.pages.iter().map(|p| p.id).collect::<BTreeSet<_>>();
    let widgets = project
        .pages
        .iter()
        .flat_map(|p| p.widgets.iter().map(|w| w.id))
        .collect::<BTreeSet<_>>();
    for widget in project.pages.iter_mut().flat_map(|p| &mut p.widgets) {
        let interaction = &widget.interaction;
        let missing = match interaction.action {
            Action::None => false,
            Action::Navigate => interaction
                .target_page
                .is_none_or(|id| !pages.contains(&id)),
            Action::SetText | Action::ToggleVisibility => interaction
                .target_widget
                .is_none_or(|id| !widgets.contains(&id)),
        };
        if missing {
            widget.interaction = Interaction::default();
        }
    }
}
