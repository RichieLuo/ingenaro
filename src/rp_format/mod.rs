//! Import the editable content of native RP projects and widget libraries.
//!
//! The editor presents Ingenaro's smaller model. Native saving patches edited
//! fields and preserves untouched native objects and container parts.

mod converter;

pub mod binary;
pub mod container;
mod legacy;
mod legacy_package;
mod native;
mod native_events;
pub mod preview_program;
pub mod preview_scene;
mod vector;
pub use native::NativeDocument;
pub use preview_scene::{
    NativePreview, PreviewScene, SceneNode, SceneNodeKind, ScenePage, SceneState,
};

use anyhow::{Context, Result, ensure};
use base64::Engine;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::model::{
    Action, CanvasMode, FORMAT_VERSION, Interaction, Page, PageTreeNode, Project, TextLayout,
    Widget, WidgetKind,
};
use binary::{Graph, Node, reference_id};

const EMPTY_ID: &str = "00000000000000000000000000000000";
const MAX_OBJECTS: usize = 300_000;
const MAX_TOTAL_PART_BYTES: usize = 256 * 1024 * 1024;
// Native PName.ROTATION is Int32 50 in the installed assembly's constant table.
const ROTATION_PROPERTY: u32 = 50;

pub struct ImportResult {
    pub project: Project,
    pub warnings: Vec<String>,
}

/// Read a supported `.rp` or `.rplib` file and convert its pages/components.
pub fn import(path: &Path) -> Result<ImportResult> {
    let document = open(path)?;
    Ok(ImportResult {
        project: document.project,
        warnings: document.warnings,
    })
}

/// Open a native document with its source graph retained for native saving.
pub fn open(path: &Path) -> Result<NativeDocument> {
    let container = container::open(path)?;
    open_container(path, container)
}

fn open_container(path: &Path, container: container::Container) -> Result<NativeDocument> {
    let root = container.index.get("root").context("原生索引缺少根目录")?;
    let offset = root
        .pointer("/parts/DesignDocument")
        .and_then(Value::as_u64)
        .context("原生文件缺少 DesignDocument")?;
    let document = binary::parse(&container.part(offset)?).context("读取 原生文档信息失败")?;
    ensure!(
        document
            .root()
            .is_some_and(|n| n.kind == "Axure:DesignDocument"),
        "原生文档根对象类型无效"
    );
    let names = package_names(&document);
    let mut tree = native_page_tree(&document, "sitemap", &names);
    let mut order = page_order(&document, "sitemap", &names);
    // Libraries normally use sitemap too; mastermap is a useful fallback for
    // libraries saved by older RP editions.
    if order.is_empty() {
        tree = native_page_tree(&document, "mastermap", &names);
        order = page_order(&document, "mastermap", &names);
    }
    if order.is_empty() {
        order.extend(
            names
                .iter()
                .filter(|(_, n)| n.page)
                .map(|(id, n)| (id.clone(), n.name.clone())),
        );
    }
    ensure!(!order.is_empty(), "原生文件中没有可以导入的页面或组件");
    let library = path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("rplib"));
    if !library {
        ensure!(
            order.len() <= 128,
            "此原生项目有 {} 个页面，超过 Ingenaro 的 128 页上限；未截断或修改源文件",
            order.len()
        );
    }

    let mut graph = document.clone();
    let mut parts = BTreeMap::from([(offset, document.clone())]);
    let mut roots = BTreeMap::new();
    let mut total_bytes = 0;
    load_packages(
        &container,
        root,
        &names,
        &mut graph,
        &mut roots,
        &mut total_bytes,
        &mut parts,
        0,
    )?;
    let styles = Styles::new(&graph);
    let mut converter = Converter {
        graph: &graph,
        roots: &roots,
        styles: &styles,
        next_id: 1,
        warnings: BTreeMap::new(),
        pending_links: Vec::new(),
        bindings: BTreeMap::new(),
        preview_parents: BTreeMap::new(),
        group_bindings: BTreeMap::new(),
    };
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut project = Project {
        version: FORMAT_VERSION,
        name,
        pages: Vec::new(),
        page_tree: Vec::new(),
        interaction_events: Vec::new(),
        next_id: 1,
    };
    let mut page_ids = BTreeMap::new();
    if library {
        converter.library_pages(&order, &mut project, &mut page_ids)?;
    } else {
        for (package, name) in &order {
            let page = converter.page(package, name)?;
            page_ids.insert(package.clone(), page.id);
            project.pages.push(page);
        }
        project.page_tree = bind_page_tree(&tree, &page_ids);
    }
    for (widget_id, package) in &converter.pending_links {
        if let Some(page_id) = page_ids.get(package) {
            for page in &mut project.pages {
                if let Some(widget) = page.widgets.iter_mut().find(|w| w.id == *widget_id) {
                    widget.interaction = Interaction {
                        action: Action::Navigate,
                        target_page: Some(*page_id),
                        ..Default::default()
                    };
                    break;
                }
            }
        } else {
            *converter
                .warnings
                .entry("目标页面不在当前项目中的跳转未导入")
                .or_default() += 1;
        }
    }
    project.next_id = converter.next_id;
    if !library {
        project.interaction_events =
            native_events::read(&graph, &roots, &project, &converter.bindings, &page_ids);
    }
    project
        .validate()
        .context("原生导入结果超出 Ingenaro 文档限制，源文件保持不变")?;
    let mut warnings = converter
        .warnings
        .into_iter()
        .map(|(message, count)| format!("{message}（{count} 处）"))
        .collect::<Vec<_>>();
    if library {
        warnings.insert(
            0,
            format!(
                "已将 {} 个库组件排列为 {} 个组件展示页；可从画布复制元件使用。",
                order.len(),
                project.pages.len()
            ),
        );
    }
    Ok(NativeDocument::new(
        project,
        warnings,
        container,
        parts,
        converter.bindings,
        converter.preview_parents,
        converter.group_bindings,
        if library { BTreeMap::new() } else { page_ids },
        path.to_owned(),
        library,
    ))
}

#[derive(Clone)]
struct PackageName {
    name: String,
    page: bool,
}

fn package_names(graph: &Graph) -> BTreeMap<String, PackageName> {
    graph
        .objects
        .values()
        .filter(|n| n.kind == "Axure:PackageInfo")
        .filter_map(|n| {
            let p = &n.properties;
            let reference = p.get("package-id")?;
            let id = reference_id(reference)?.to_owned();
            let kind = reference.get("$type").and_then(Value::as_str).unwrap_or("");
            Some((
                id,
                PackageName {
                    name: p
                        .get("package-name")
                        .and_then(Value::as_str)
                        .unwrap_or("未命名页面")
                        .to_owned(),
                    page: kind == "Axure:Page",
                },
            ))
        })
        .collect()
}

#[derive(Clone)]
enum NativePageTreeNode {
    Folder {
        id: String,
        name: String,
        children: Vec<Self>,
    },
    Page {
        package: String,
        name: String,
        children: Vec<Self>,
    },
}

fn bind_page_tree(tree: &[NativePageTreeNode], ids: &BTreeMap<String, u64>) -> Vec<PageTreeNode> {
    tree.iter()
        .filter_map(|node| {
            Some(match node {
                NativePageTreeNode::Folder { id, name, children } => PageTreeNode::Folder {
                    id: id.clone(),
                    name: name.clone(),
                    children: bind_page_tree(children, ids),
                },
                NativePageTreeNode::Page {
                    package, children, ..
                } => PageTreeNode::Page {
                    page_id: *ids.get(package)?,
                    children: bind_page_tree(children, ids),
                },
            })
        })
        .collect()
}

fn page_order(
    graph: &Graph,
    field: &str,
    names: &BTreeMap<String, PackageName>,
) -> Vec<(String, String)> {
    fn flatten(tree: &[NativePageTreeNode], output: &mut Vec<(String, String)>) {
        for node in tree {
            let children = match node {
                NativePageTreeNode::Folder { children, .. } => children,
                NativePageTreeNode::Page {
                    package,
                    name,
                    children,
                } => {
                    output.push((package.clone(), name.clone()));
                    children
                }
            };
            flatten(children, output);
        }
    }
    let mut output = Vec::new();
    flatten(&native_page_tree(graph, field, names), &mut output);
    output
}

fn native_page_tree(
    graph: &Graph,
    field: &str,
    names: &BTreeMap<String, PackageName>,
) -> Vec<NativePageTreeNode> {
    let Some(map) = graph
        .root()
        .and_then(|r| r.properties.get(field))
        .and_then(|v| graph.resolve(v))
    else {
        return Vec::new();
    };
    let Some(table) = map.properties.get("node-table").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut seen = BTreeSet::new();
    let mut seen_pages = BTreeSet::new();
    #[allow(clippy::too_many_arguments)]
    fn visit(
        graph: &Graph,
        table: &serde_json::Map<String, Value>,
        names: &BTreeMap<String, PackageName>,
        node_id: &str,
        seen: &mut BTreeSet<String>,
        seen_pages: &mut BTreeSet<String>,
        depth: usize,
    ) -> Vec<NativePageTreeNode> {
        if depth > 64 || !seen.insert(node_id.to_owned()) {
            return Vec::new();
        }
        let mut children_out = Vec::new();
        if let Some(children) = graph
            .node(node_id)
            .and_then(|n| n.properties.get("children"))
            .and_then(Value::as_array)
        {
            for child in children {
                if let Some(id) = reference_id(child) {
                    children_out.extend(visit(
                        graph,
                        table,
                        names,
                        id,
                        seen,
                        seen_pages,
                        depth + 1,
                    ));
                }
            }
        }
        if let Some(package) = table.get(node_id).and_then(reference_id) {
            if let Some(info) = names.get(package).filter(|n| n.page) {
                if seen_pages.insert(package.to_owned()) {
                    return vec![NativePageTreeNode::Page {
                        package: package.to_owned(),
                        name: truncate(&info.name, 1024),
                        children: children_out,
                    }];
                }
            } else if let Some(folder) = graph.node(package) {
                let name = folder
                    .properties
                    .get("name")
                    .or_else(|| folder.properties.get("Name"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if !name.trim().is_empty() {
                    return vec![NativePageTreeNode::Folder {
                        id: node_id.to_owned(),
                        name: truncate(name, 1024),
                        children: children_out,
                    }];
                }
            }
        }
        children_out
    }
    if let Some(tree) = map.properties.get("tree").and_then(reference_id) {
        visit(graph, table, names, tree, &mut seen, &mut seen_pages, 0)
    } else {
        Vec::new()
    }
}

#[allow(clippy::too_many_arguments)]
fn load_packages(
    container: &container::Container,
    index: &Value,
    names: &BTreeMap<String, PackageName>,
    graph: &mut Graph,
    roots: &mut BTreeMap<String, String>,
    total_bytes: &mut usize,
    parts: &mut BTreeMap<u64, Graph>,
    depth: usize,
) -> Result<()> {
    ensure!(depth <= 64, "原生部件目录嵌套过深");
    if let Some(children) = index.get("children").and_then(Value::as_object) {
        for (package, child) in children {
            let known_non_page = names.get(package).is_some_and(|n| !n.page);
            if !known_non_page {
                if let Some(offset) = child.pointer("/parts/package").and_then(Value::as_u64) {
                    let bytes = container.part(offset)?;
                    *total_bytes = total_bytes
                        .checked_add(bytes.len())
                        .context("原生数据大小溢出")?;
                    ensure!(
                        *total_bytes <= MAX_TOTAL_PART_BYTES,
                        "原生所有对象包解压后超过 256 MiB 上限"
                    );
                    let part = binary::parse(&bytes)
                        .with_context(|| format!("读取 原生部件 {package} 失败"))?;
                    parts.insert(offset, part.clone());
                    let scoped = scope_package(&part);
                    roots.insert(package.clone(), scoped.root);
                    graph.objects.extend(scoped.objects);
                    ensure!(
                        graph.objects.len() <= MAX_OBJECTS,
                        "原生对象数量超过 300000 上限"
                    );
                }
                load_packages(
                    container,
                    child,
                    names,
                    graph,
                    roots,
                    total_bytes,
                    parts,
                    depth + 1,
                )?;
            }
        }
    }
    Ok(())
}

// Native page copies can deliberately reuse object IDs in distinct packages.
// Scope only local object references in the editor's read graph; native graphs
// retain their exact original IDs and binary provenance for saving.
fn scope_package(graph: &Graph) -> Graph {
    fn rewrite(value: &mut Value, package: &str, ids: &BTreeSet<String>) {
        match value {
            Value::Object(map) => {
                let local = map
                    .get("$token")
                    .and_then(Value::as_i64)
                    .is_some_and(|t| matches!(t, 3 | 20 | 21 | 22));
                if local
                    && let Some(id) = map
                        .get("$ref")
                        .and_then(Value::as_str)
                        .filter(|id| ids.contains(*id))
                        .map(str::to_owned)
                {
                    map.insert("$ref".into(), serde_json::json!(format!("{package}/{id}")));
                }
                let keys = map
                    .keys()
                    .filter(|key| ids.contains(*key))
                    .cloned()
                    .collect::<Vec<_>>();
                for key in keys {
                    if let Some(value) = map.remove(&key) {
                        map.insert(format!("{package}/{key}"), value);
                    }
                }
                for value in map.values_mut() {
                    rewrite(value, package, ids);
                }
            }
            Value::Array(array) => {
                for value in array {
                    rewrite(value, package, ids);
                }
            }
            _ => {}
        }
    }
    let ids = graph.objects.keys().cloned().collect::<BTreeSet<_>>();
    let objects = graph
        .objects
        .iter()
        .map(|(id, node)| {
            let mut node = node.clone();
            rewrite(&mut node.properties, &graph.package, &ids);
            (format!("{}/{id}", graph.package), node)
        })
        .collect();
    Graph::new(
        graph.version,
        format!("{}/{}", graph.package, graph.root),
        graph.package.clone(),
        objects,
    )
}

type Properties = BTreeMap<u32, Value>;

// Diagram object arrays can be registries containing both groups and every
// descendant. Layer arrays contain direct children. Only registered/reachable
// layers establish ownership; orphaned layers retained for lossless saving do
// not hide objects after ungrouping.
fn layer_registry_members(graph: &Graph, references: &[Value]) -> Result<Vec<Value>> {
    fn visit(
        graph: &Graph,
        reference: &Value,
        result: &mut Vec<Value>,
        members: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
        active: &mut BTreeSet<String>,
        depth: usize,
    ) -> Result<()> {
        ensure!(depth <= 64, "原生组合嵌套超过限制");
        let Some(id) = reference_id(reference) else {
            return Ok(());
        };
        let Some(node) = graph.node(id).filter(|node| node.kind.ends_with(":Layer")) else {
            return Ok(());
        };
        ensure!(!active.contains(id), "原生组合包含循环引用");
        if !visited.insert(id.to_owned()) {
            return Ok(());
        }
        active.insert(id.to_owned());
        for child in array(&node.properties["DiagramObjects"]) {
            if let Some(id) = reference_id(child)
                && members.insert(id.to_owned())
            {
                result.push(child.clone());
            }
            visit(graph, child, result, members, visited, active, depth + 1)?;
        }
        active.remove(id);
        Ok(())
    }
    let mut result = Vec::new();
    let mut members = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut active = BTreeSet::new();
    for reference in references {
        visit(
            graph,
            reference,
            &mut result,
            &mut members,
            &mut visited,
            &mut active,
            0,
        )?;
    }
    Ok(result)
}

fn direct_diagram_references(graph: &Graph, references: &[Value]) -> Result<Vec<Value>> {
    let members = layer_registry_members(graph, references)?
        .iter()
        .filter_map(reference_id)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    Ok(references
        .iter()
        .filter(|reference| reference_id(reference).is_none_or(|id| !members.contains(id)))
        .cloned()
        .collect())
}

fn props(list: &Value) -> Properties {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let number = entry.get("DOAName")?.as_i64()?;
            Some(((number as u32) & 0x3fff_ffff, entry.get("Value")?.clone()))
        })
        .collect()
}

struct Styles {
    named: BTreeMap<String, Properties>,
    default: Properties,
    default_page: Properties,
}

impl Styles {
    fn new(graph: &Graph) -> Self {
        let mut named = BTreeMap::new();
        for node in graph
            .objects
            .values()
            .filter(|n| matches!(n.kind.as_str(), "Axure:Style" | "Axure:PageStyle"))
        {
            if let Some(id) = node.properties.get("Id").and_then(reference_id) {
                named.insert(id.to_owned(), props(&node.properties["PropList"]));
            }
        }
        // Cached styles contain the resolved properties for external widget
        // libraries, so an installed/default library is not required to import.
        for node in graph.objects.values() {
            if let Some(styles) = node
                .properties
                .pointer("/StyleCache/Styles")
                .and_then(Value::as_array)
            {
                for style in styles {
                    if let Some(id) = style.pointer("/Info/Id").and_then(reference_id) {
                        let entry = named.entry(id.to_owned()).or_insert_with(BTreeMap::new);
                        entry.extend(props(&style["Props"]));
                    }
                }
            }
        }
        let default = graph
            .root()
            .and_then(|r| graph.resolve(&r.properties["StyleSheet"]))
            .and_then(|sheet| graph.resolve(&sheet.properties["DefaultStyle"]))
            .map(|style| props(&style.properties["PropList"]))
            .unwrap_or_default();
        let default_page = graph
            .root()
            .and_then(|r| graph.resolve(&r.properties["PageStyleSheet"]))
            .and_then(|sheet| graph.resolve(&sheet.properties["DefaultPageStyle"]))
            .map(|style| props(&style.properties["PropList"]))
            .unwrap_or_default();
        Self {
            named,
            default,
            default_page,
        }
    }

    fn for_node(&self, graph: &Graph, node: &Node) -> Properties {
        let mut output = self.default.clone();
        if let Some(style) = graph.resolve(&node.properties["MyStyle"]) {
            let local = props(&style.properties["PropList"]);
            if let Some(id) = local.get(&0).and_then(reference_id).or_else(|| {
                style
                    .properties
                    .pointer("/MatchInfo/Id")
                    .and_then(reference_id)
            }) {
                self.extend_named(id, &mut output, &mut BTreeSet::new());
            }
            output.extend(local);
        }
        output
    }

    fn for_page(&self, graph: &Graph, node: &Node) -> Properties {
        let mut output = self.default_page.clone();
        if let Some(style) = graph.resolve(&node.properties["page-style"]) {
            let local = props(&style.properties["PropList"]);
            if let Some(id) = local.get(&0).and_then(reference_id) {
                self.extend_named(id, &mut output, &mut BTreeSet::new());
            }
            output.extend(local);
        }
        output
    }

    fn extend_named(&self, id: &str, output: &mut Properties, seen: &mut BTreeSet<String>) {
        if seen.len() > 32 || !seen.insert(id.to_owned()) {
            return;
        }
        if let Some(properties) = self.named.get(id) {
            if let Some(parent) = properties
                .get(&0)
                .and_then(reference_id)
                .filter(|p| *p != id)
            {
                self.extend_named(parent, output, seen);
            }
            output.extend(properties.clone());
        }
    }
}

struct Converter<'a> {
    graph: &'a Graph,
    roots: &'a BTreeMap<String, String>,
    styles: &'a Styles,
    next_id: u64,
    warnings: BTreeMap<&'static str, usize>,
    pending_links: Vec<(u64, String)>,
    bindings: BTreeMap<u64, native::Binding>,
    preview_parents: BTreeMap<u64, u64>,
    group_bindings: BTreeMap<u64, native::GroupBinding>,
}

fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn number(value: &Value) -> Option<f32> {
    value.as_f64().filter(|v| v.is_finite()).map(|v| v as f32)
}
fn finite(value: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        min
    }
}

fn argb_color(value: &Value) -> Option<String> {
    let color = value.as_i64()? as u32;
    let alpha = color >> 24;
    let rgb = color & 0x00ff_ffff;
    Some(if alpha == 255 {
        format!("#{rgb:06X}")
    } else {
        format!("#{rgb:06X}{alpha:02X}")
    })
}

fn fill_color(value: &Value) -> Option<String> {
    if value["fill-type"].as_str() == Some("none") {
        return Some("#FFFFFF00".into());
    }
    argb_color(&value["fill-color"]).or_else(|| {
        value
            .get("gradient-stops")
            .and_then(Value::as_array)
            .and_then(|s| s.first())
            .and_then(|stop| argb_color(&stop["color"]))
    })
}

// Older project versions store a shape's text in a contained text object.
// Resolve it only in the read graph; native saving retains the source layout.
fn text_properties<'a>(properties: &'a Value, graph: &'a Graph) -> &'a Value {
    if !widget_text(properties).is_empty() {
        return properties;
    }
    array(&properties["diagram-objects"])
        .iter()
        .filter_map(|reference| graph.resolve(reference))
        .filter(|node| node.kind.ends_with(":RichTextPanel"))
        .map(|node| &node.properties)
        .find(|child| !widget_text(child).is_empty())
        .unwrap_or(properties)
}

fn text_paragraphs(properties: &Value) -> &[Value] {
    properties
        .pointer("/RichTextBlock/attributed-string/Paragraphs")
        .or_else(|| properties.pointer("/rtf-block/attributed-string/Paragraphs"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn widget_text(properties: &Value) -> String {
    let mut text = text_paragraphs(properties)
        .iter()
        .map(|paragraph| {
            array(&paragraph["Inlines"])
                .iter()
                .filter_map(|inline| inline["Text"].as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() {
        text = properties
            .pointer("/TextBlock/text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
    }
    if text.is_empty() {
        text = properties["PlaceholderText"]
            .as_str()
            .unwrap_or("")
            .to_owned();
    }
    text
}

/// RP 11 stores list items as SelectObject references, preserving their order.
/// Reject unknown records rather than inventing visible options.
fn list_options_text(properties: &Value, graph: &Graph) -> Option<String> {
    let options = properties.get("list-options")?.as_array()?;
    options
        .iter()
        .map(|option| {
            let option = graph.resolve(option)?;
            (option.kind == "Axure:SelectObject")
                .then(|| option.properties["object"].as_str())?
                .map(str::to_owned)
        })
        .collect::<Option<Vec<_>>>()
        .map(|options| options.join("\n"))
}

fn first_inline(properties: &Value) -> Option<&Value> {
    text_paragraphs(properties)
        .iter()
        .flat_map(|p| array(&p["Inlines"]))
        .find(|v| v["Text"].as_str().is_some_and(|s| !s.trim().is_empty()))
}

fn embedded_image(value: &Value, graph: &Graph) -> Option<String> {
    fn bytes(value: &Value, graph: &Graph, depth: usize) -> Option<Vec<u8>> {
        if depth > 8 {
            return None;
        }
        if let Some(binary) = value["$binary"].as_str() {
            return base64::engine::general_purpose::STANDARD
                .decode(binary)
                .ok();
        }
        if let Some(node) = graph.resolve(value) {
            return bytes(&node.properties, graph, depth + 1);
        }
        for field in ["Stream", "Image", "Data", "OriginalImage", "ImageData"] {
            if let Some(nested) = value.get(field)
                && let Some(result) = bytes(nested, graph, depth + 1)
            {
                return Some(result);
            }
        }
        None
    }
    let bytes = bytes(value, graph, 0)?;
    if bytes.len() > 12 * 1024 * 1024 {
        return None;
    }
    let format = image::guess_format(&bytes).ok()?;
    let mime = match format {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpeg",
        image::ImageFormat::Gif => "gif",
        image::ImageFormat::WebP => "webp",
        _ => return None,
    };
    let dimensions = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format)
        .into_dimensions()
        .ok()?;
    if dimensions.0 == 0
        || dimensions.1 == 0
        || dimensions.0 > 8192
        || dimensions.1 > 8192
        || u64::from(dimensions.0) * u64::from(dimensions.1) > 32_000_000
    {
        return None;
    }
    Some(format!(
        "data:image/{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn truncate(text: &str, bytes: usize) -> String {
    let mut end = text.len().min(bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[cfg(test)]
mod import_tests;

#[cfg(test)]
mod form_import_tests;

#[cfg(test)]
mod legacy_text_tests;
