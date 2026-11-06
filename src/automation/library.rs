//! The same catalog and factories used by the GUI, never a second preset implementation.

use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

use crate::{
    model::{Project, Widget, WidgetKind},
    widget_library::{Category, ITEMS, LibraryItem},
};

/// Deliberately excludes identity, geometry, kind, text and vector/group metadata.
/// The schema filters its shared widget properties using this exact allowlist.
pub(super) const STYLE_FIELDS: &[&str] = &[
    "fill",
    "stroke",
    "text_color",
    "font_size",
    "radius",
    "opacity",
    "hidden",
    "locked",
    "text_layout",
];
pub(super) const MAX_TEXTS: usize = 12;
pub(super) fn item_ids() -> Vec<&'static str> {
    ITEMS.iter().map(|item| item.id).collect()
}

pub(super) fn category_ids() -> Vec<&'static str> {
    Category::ALL.into_iter().map(Category::id).collect()
}

fn item(id: &str) -> Result<LibraryItem> {
    ensure!(id.len() <= 128, "item_id exceeds 128 bytes");
    ITEMS
        .iter()
        .copied()
        .find(|item| item.id == id)
        .with_context(|| format!("Unknown library item: {id}; use rpshop_read_library"))
}

struct Template {
    widgets: Vec<Widget>,
    bounds: [f32; 4], // x, y, width, height; presets are unrotated document coordinates.
    allocation_count: u64,
}

fn template(item: LibraryItem) -> Result<Template> {
    // Never borrow the live allocator for reads or sizing/preflight.
    let mut sandbox = Project::blank();
    let first_id = sandbox.next_id;
    let widgets = item.create(&mut sandbox, 0.0, 0.0);
    ensure!(
        !widgets.is_empty() && widgets.len() <= 5000,
        "Library item has an invalid widget count"
    );
    sandbox.pages[0].widgets = widgets.clone();
    sandbox
        .validate()
        .context("Invalid built-in library item")?;
    Ok(Template {
        bounds: bounds(&widgets),
        widgets,
        allocation_count: sandbox.next_id - first_id,
    })
}

fn bounds(widgets: &[Widget]) -> [f32; 4] {
    let left = widgets.iter().map(|w| w.x).fold(f32::INFINITY, f32::min);
    let top = widgets.iter().map(|w| w.y).fold(f32::INFINITY, f32::min);
    let right = widgets
        .iter()
        .map(|w| w.x + w.width)
        .fold(f32::NEG_INFINITY, f32::max);
    let bottom = widgets
        .iter()
        .map(|w| w.y + w.height)
        .fold(f32::NEG_INFINITY, f32::max);
    [left, top, right - left, bottom - top]
}

fn bounds_value(bounds: [f32; 4]) -> Value {
    json!({"x":bounds[0],"y":bounds[1],"width":bounds[2],"height":bounds[3]})
}

fn groups(widgets: &[Widget]) -> BTreeSet<u64> {
    widgets
        .iter()
        .flat_map(|w| w.groups.iter().copied())
        .collect()
}

fn description(item: LibraryItem, widgets: &[Widget]) -> String {
    match item.id {
        "table" => "Editable 4-row × 3-column table: 12 grouped Rectangle cell widgets in row-major order; first row is the header. This is a built-in composite preset, not a Table WidgetKind.".into(),
        "horizontal-menu" => "Four grouped editable menu widgets, left to right; not a separate native Menu WidgetKind.".into(),
        "vertical-menu" => "Four grouped editable menu widgets, top to bottom; not a separate native Menu WidgetKind.".into(),
        "tree" => "Five grouped editable Text widgets with tree labels, top to bottom; not a separate native Tree WidgetKind or automatic hierarchy.".into(),
        _ if item.category == Category::Patterns => format!(
            "{}: a grouped interface pattern with individually editable members in paint order.", item.label
        ),
        _ if widgets.iter().any(|w| w.vector_points.is_some()) => format!(
            "{}: editable widget with the GUI preset's normalized vector paths; resizing preserves those paths.", item.label
        ),
        _ => format!("{}: the existing GUI component preset with editable default properties.", item.label),
    }
}

fn native_constraints(widgets: &[Widget]) -> Value {
    let panel = widgets.iter().any(|w| w.kind == WidgetKind::Panel);
    let form = widgets.iter().find(|w| {
        matches!(
            w.kind,
            WidgetKind::Radio | WidgetKind::TextArea | WidgetKind::Droplist | WidgetKind::ListBox
        )
    });
    json!({
        "creation":if panel {"unsupported"} else if form.is_some() {"requires_form_template"} else {"requires_host_validation"},
        "required_template_kind":form.map(|w|w.kind),
        "requires_host_native_validation":true,
        "note":if panel {
            "Native documents cannot create Panel containers; supported in RPShop projects."
        } else if form.is_some() {
            "The native host must find an actual template of this form type in the source document; availability is not guaranteed by the catalog."
        } else {
            "Generated text layout and vector metadata are allowed. Native source templates, galleries, hierarchy and read-only bindings still require native.preview_scene validation by the live GUI host."
        }
    })
}

pub(super) fn read(args: &Value, native: bool) -> Result<Value> {
    super::fields(args, &["query", "category", "item_id"]).map_err(anyhow::Error::msg)?;
    let query = args
        .get("query")
        .map(|value| {
            let query = value.as_str().context("query must be a string")?;
            ensure!(query.len() <= 256, "query exceeds 256 bytes");
            Ok::<_, anyhow::Error>(query)
        })
        .transpose()?;
    let category = args
        .get("category")
        .map(|value| {
            let id = value.as_str().context("category must be a category ID")?;
            Category::ALL
                .into_iter()
                .find(|category| category.id() == id)
                .with_context(|| format!("Unknown category; use {}", category_ids().join(", ")))
        })
        .transpose()?;
    let requested = args
        .get("item_id")
        .map(|value| item(value.as_str().context("item_id must be a string")?))
        .transpose()?;
    let mut items = Vec::new();
    for item in ITEMS.iter().copied().filter(|item| {
        requested.is_none_or(|selected| selected.id == item.id)
            && category.is_none_or(|category| category == item.category)
            && query.is_none_or(|query| item.matches(query))
    }) {
        let template = template(item)?;
        let groups = groups(&template.widgets);
        let representation = if !groups.is_empty() {
            "grouped_widgets"
        } else if template.widgets.iter().any(|w| w.vector_points.is_some()) {
            "vector_widget"
        } else {
            "widget"
        };
        let mut value = json!({
            "id":item.id,"label":item.label,"category":item.category.id(),
            "category_label":item.category.label(),"keywords":item.keywords,
            "representation":representation,"description":description(item,&template.widgets),
            "widget_count":template.widgets.len(),"group_count":groups.len(),
            "widget_kinds":template.widgets.iter().map(|w|w.kind).fold(Vec::new(),|mut kinds,kind|{
                if !kinds.contains(&kind) {kinds.push(kind);} kinds
            }),
            "default_bounds":bounds_value(template.bounds),
            "native_constraints":native_constraints(&template.widgets)
        });
        if requested.is_some() {
            value["default_widgets"] = json!(template.widgets);
            value["template_group_ids"] = json!(groups);
            value["template_ids_authoritative"] = json!(false);
            value["template_id_note"] = json!(
                "These default widget/group IDs exist only in an isolated example, not the live document. Use widget_ids/group_ids returned by create_library_item for all subsequent edits."
            );
        }
        items.push(value);
    }
    Ok(json!({
        "native":native,"catalog_total":ITEMS.len(),"count":items.len(),"items":items,
        "categories":Category::ALL.map(|category|json!({"id":category.id(),"label":category.label()})),
        "create_operation":"create_library_item",
        "text_order":"texts replaces a prefix of widgets in factory order (table row-major; menus/tree in visual order); trailing default texts remain unchanged.",
        "geometry_note":"x/y position the preset's bounding box; optional width/height independently scale member positions and sizes. Normalized vectors, groups, font size, padding and radius are preserved unless explicitly styled; each resulting widget must still satisfy document limits.",
        "style_fields":STYLE_FIELDS
    }))
}

pub(super) fn native_preflight(item_id: &str) -> Result<()> {
    ensure!(
        item(item_id)?.kind() != WidgetKind::Panel,
        "Native documents do not support creating Panel containers"
    );
    // Forms and preset metadata are source-dependent, not blanket unsupported.
    // The GUI stages the entire candidate through NativeDocument::preview_scene.
    Ok(())
}

fn bounded(value: f32, min: f32, max: f32, field: &str) -> Result<()> {
    ensure!(
        value.is_finite() && value >= min && value <= max,
        "{field} must be a finite number between {min} and {max}"
    );
    Ok(())
}

/// Applied only to edit::batch's disposable candidate. All counts and allocator
/// headroom are checked before invoking the shared, infallible GUI factory.
#[allow(clippy::too_many_arguments)]
pub(super) fn create(
    project: &mut Project,
    page: usize,
    item_id: &str,
    x: f32,
    y: f32,
    width: Option<f32>,
    height: Option<f32>,
    texts: Option<&[String]>,
    properties: &Value,
) -> Result<Value> {
    let item = item(item_id)?;
    bounded(x, -100000.0, 100000.0, "x")?;
    bounded(y, -100000.0, 100000.0, "y")?;
    for (field, value) in [("width", width), ("height", height)] {
        if let Some(value) = value {
            bounded(value, 1.0, 20000.0, field)?;
        }
    }
    super::fields(properties, STYLE_FIELDS).map_err(anyhow::Error::msg)?;
    if let Some(layout) = properties
        .get("text_layout")
        .filter(|value| !value.is_null())
    {
        super::fields(
            layout,
            &["horizontal", "vertical", "padding", "wrap", "bold"],
        )
        .map_err(anyhow::Error::msg)?;
    }
    let template = template(item)?;
    if let Some(texts) = texts {
        ensure!(
            texts.len() <= MAX_TEXTS && texts.len() <= template.widgets.len(),
            "texts must contain at most {} strings for this preset",
            template.widgets.len().min(MAX_TEXTS)
        );
        ensure!(
            texts.iter().all(|text| text.len() <= 100000),
            "Each preset text must be at most 100000 bytes"
        );
    }
    let page_len = project
        .pages
        .get(page)
        .context("Page does not exist")?
        .widgets
        .len();
    ensure!(
        page_len
            .checked_add(template.widgets.len())
            .is_some_and(|count| count <= 5000),
        "Page widget count exceeds 5000"
    );
    let total: usize = project.pages.iter().map(|page| page.widgets.len()).sum();
    ensure!(
        total
            .checked_add(template.widgets.len())
            .is_some_and(|count| count <= 20000),
        "Project widget count exceeds 20000"
    );
    let expected_next = project
        .next_id
        .checked_add(template.allocation_count)
        .filter(|next| *next < u64::MAX)
        .context("Document IDs exhausted")?;
    let mut widgets = item.create(project, 0.0, 0.0);
    ensure!(
        project.next_id == expected_next && widgets.len() == template.widgets.len(),
        "Library factory allocation changed unexpectedly"
    );
    let sx = width.unwrap_or(template.bounds[2]) / template.bounds[2];
    let sy = height.unwrap_or(template.bounds[3]) / template.bounds[3];
    for (index, widget) in widgets.iter_mut().enumerate() {
        widget.x = x + (widget.x - template.bounds[0]) * sx;
        widget.y = y + (widget.y - template.bounds[1]) * sy;
        widget.width *= sx;
        widget.height *= sy;
        if let Some(text) = texts.and_then(|texts| texts.get(index)) {
            widget.text.clone_from(text);
        }
        super::edit::patch(widget, properties, STYLE_FIELDS)?;
    }
    let result = json!({
        "op":"create_library_item","item_id":item.id,"page_id":project.pages[page].id,
        "widget_ids":widgets.iter().map(|widget|widget.id).collect::<Vec<_>>(),
        "group_ids":groups(&widgets),"bounds":bounds_value(bounds(&widgets))
    });
    project.pages[page].widgets.extend(widgets);
    Ok(result)
}
