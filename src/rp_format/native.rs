//! Patch editable native properties while preserving every unedited source part.

mod patch;

mod groups;
use groups::*;

mod ordering;
use ordering::*;

mod interactions;
use interactions::*;

mod widgets;
use widgets::*;

mod text;
use text::*;

use super::*;
use anyhow::bail;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct Binding {
    pub object: String,
    pub scale: f32,
}

#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct GroupBinding {
    pub object: String,
}

#[derive(Clone)]
pub struct NativeDocument {
    pub project: Project,
    pub warnings: Vec<String>,
    baseline: Project,
    source: container::Container,
    parts: BTreeMap<u64, Graph>,
    bindings: BTreeMap<u64, Binding>,
    preview_parents: BTreeMap<u64, u64>,
    group_bindings: BTreeMap<u64, GroupBinding>,
    pages: BTreeMap<u64, String>,
    source_path: PathBuf,
    library: bool,
    preview_identity: Arc<()>,
}

impl NativeDocument {
    pub fn is_editable_widget(&self, id: u64) -> bool {
        editable_native_id(&self.baseline, &self.bindings, id)
    }
    pub fn selection_owner(&self, id: u64) -> u64 {
        self.preview_parents.get(&id).copied().unwrap_or(id)
    }
    pub fn is_bitmap_widget(&self, id: u64) -> bool {
        if !self.bindings.contains_key(&id) {
            return self.is_editable_widget(id);
        }
        self.bindings
            .get(&id)
            .and_then(|binding| {
                let (package, object) = binding.object.rsplit_once('/')?;
                self.parts
                    .values()
                    .find(|graph| graph.package == package)?
                    .node(object)
            })
            .is_some_and(|node| {
                node.kind.ends_with(":ImageBox") || node.kind.ends_with(":Screenshot")
            })
    }
    /// Refresh read-only master contents after an editor geometry operation.
    /// `before` must be the operation's starting project (the drag-start project
    /// for every drag frame), so repeated calls never compound the transform.
    pub fn refresh_previews(&self, project: &mut Project, before: &Project) {
        let parent_ids = self
            .preview_parents
            .values()
            .copied()
            .collect::<BTreeSet<_>>();
        let old = before
            .pages
            .iter()
            .flat_map(|page| &page.widgets)
            .map(|widget| (widget.id, widget))
            .collect::<BTreeMap<_, _>>();
        let parents = project
            .pages
            .iter()
            .flat_map(|page| &page.widgets)
            .filter(|widget| parent_ids.contains(&widget.id))
            .map(|widget| (widget.id, widget.clone()))
            .collect::<BTreeMap<_, _>>();
        for page in &mut project.pages {
            let count = page.widgets.len();
            page.widgets.retain(|widget| {
                self.preview_parents
                    .get(&widget.id)
                    .is_none_or(|parent| parents.contains_key(parent))
            });
            for widget in &mut page.widgets {
                let Some(parent) = self.preview_parents.get(&widget.id) else {
                    continue;
                };
                let (Some(previous), Some(old_parent), Some(new_parent)) =
                    (old.get(&widget.id), old.get(parent), parents.get(parent))
                else {
                    continue;
                };
                *widget = transform_preview(previous, old_parent, new_parent);
            }
            if page.widgets.len() != count {
                page.prune_group_names();
            }
        }
    }
    /// Source page packages remain stable when a native save reallocates editor IDs.
    pub fn preview_page_keys(&self) -> BTreeMap<u64, String> {
        self.pages.clone()
    }

    /// An in-process identity for immutable preview source snapshots. Clones
    /// share it; replacing verified source bindings allocates a new identity.
    pub fn preview_identity(&self) -> usize {
        Arc::as_ptr(&self.preview_identity) as usize
    }

    /// Build a separate native runtime scene from an in-memory source snapshot.
    /// Staging supported editor edits uses the native patcher without writing a
    /// container or changing the editor/session's baseline and bindings.
    pub fn preview_scene(&self, edited: &Project) -> Result<super::preview_scene::NativePreview> {
        edited.validate()?;
        if self.library {
            // Gallery rendering is static, but automation still requires the same
            // source-dependent writeback checks as a native save, without writing.
            if edited != &self.baseline {
                let mut staged = self.parts.clone();
                self.patch_project(edited, &mut staged)?;
            }
            return Ok(super::preview_scene::NativePreview {
                scene: super::preview_scene::PreviewScene {
                    diagnostics: vec!["组件库预览展示静态组件，暂不执行库内原生交互".into()],
                    ..Default::default()
                },
                program: super::preview_program::PreviewProgram::default(),
            });
        }
        let mut staged;
        let parts = if edited == &self.baseline {
            &self.parts
        } else {
            staged = self.parts.clone();
            self.patch_project(edited, &mut staged)?;
            &staged
        };
        let (scene, graph, page_keys) = super::preview_scene::collect(parts, &self.pages, edited)?;
        let program = super::preview_program::compile(&graph, &scene.bindings, &page_keys);
        Ok(super::preview_scene::NativePreview { scene, program })
    }

    /// Persist source bindings separately from the editor's unsaved document.
    pub fn session_state(&self) -> Value {
        json!({"version":2,"fingerprint":self.source.original_fingerprint(),
            "project":self.baseline,"bindings":self.bindings,"pages":self.pages,
            "preview_parents":self.preview_parents,
            "group_bindings":self.group_bindings,
            "library":self.library})
    }

    pub fn restore_session_state(&mut self, state: &Value) -> Result<()> {
        ensure!(
            state["version"].as_u64() == Some(2),
            "原生会话绑定版本已更新，请重新打开源文件；草稿内容可继续保留"
        );
        ensure!(
            state["fingerprint"].as_str() == Some(self.source.original_fingerprint().as_str()),
            "原生文件已更改，会话绑定已失效，请重新打开文件"
        );
        ensure!(
            state["library"].as_bool() == Some(self.library),
            "原生会话文件类型不一致"
        );
        let baseline: Project =
            serde_json::from_value(state["project"].clone()).context("原生会话项目无效")?;
        baseline.validate()?;
        ensure!(
            baseline.page_tree == self.baseline.page_tree,
            "页面目录已更新，请重新打开源文件以恢复目录树；当前草稿可继续保留"
        );
        let bindings: BTreeMap<u64, Binding> =
            serde_json::from_value(state["bindings"].clone()).context("原生元件绑定无效")?;
        let pages: BTreeMap<u64, String> =
            serde_json::from_value(state["pages"].clone()).context("原生页面绑定无效")?;
        {
            let previews = state
                .get("preview_parents")
                .context("原生会话缺少母版预览绑定")?;
            let previews: BTreeMap<u64, u64> =
                serde_json::from_value(previews.clone()).context("母版预览绑定无效")?;
            ensure!(
                previews == self.preview_parents,
                "母版预览绑定与源文件不匹配"
            );
        }
        {
            let groups = state
                .get("group_bindings")
                .context("原生会话缺少组合绑定")?;
            let groups: BTreeMap<u64, GroupBinding> =
                serde_json::from_value(groups.clone()).context("原生组绑定无效")?;
            ensure!(groups == self.group_bindings, "原生组绑定与源文件不匹配");
        }
        let widget_ids = baseline
            .pages
            .iter()
            .flat_map(|p| p.widgets.iter().map(|w| w.id))
            .collect::<BTreeSet<_>>();
        let owners = owners(&self.parts);
        for (id, binding) in &bindings {
            ensure!(
                widget_ids.contains(id)
                    && owners.contains_key(&binding.object)
                    && binding.scale.is_finite()
                    && binding.scale > 0.0
                    && binding.scale <= 1.0,
                "原生会话元件绑定与源文件不匹配"
            );
        }
        if !self.library {
            let read_only = self
                .baseline
                .pages
                .iter()
                .flat_map(|p| &p.widgets)
                .filter(|w| !self.bindings.contains_key(&w.id))
                .map(|w| w.id)
                .collect::<BTreeSet<_>>();
            ensure!(
                widget_ids
                    .iter()
                    .all(|id| bindings.contains_key(id) || read_only.contains(id)),
                "原生会话缺少元件绑定"
            );
            ensure!(pages.len() == baseline.pages.len(), "原生会话缺少页面绑定");
            for page in &baseline.pages {
                ensure!(
                    pages.get(&page.id).is_some_and(|package| self
                        .parts
                        .values()
                        .any(|g| g.package == *package
                            && g.root().is_some_and(|n| n.kind == "Axure:Page"))),
                    "原生会话页面绑定与源文件不匹配"
                );
            }
        }
        self.project = baseline.clone();
        self.baseline = baseline;
        self.bindings = bindings;
        self.pages = pages;
        self.preview_identity = Arc::new(());
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        project: Project,
        warnings: Vec<String>,
        source: container::Container,
        parts: BTreeMap<u64, Graph>,
        bindings: BTreeMap<u64, Binding>,
        preview_parents: BTreeMap<u64, u64>,
        group_bindings: BTreeMap<u64, GroupBinding>,
        pages: BTreeMap<String, u64>,
        source_path: PathBuf,
        library: bool,
    ) -> Self {
        Self {
            baseline: project.clone(),
            project,
            warnings,
            source,
            parts,
            bindings,
            preview_parents,
            group_bindings,
            pages: pages
                .into_iter()
                .map(|(package, id)| (id, package))
                .collect(),
            source_path,
            library,
            preview_identity: Arc::new(()),
        }
    }

    /// Save to the original native format. The source is checked immediately
    /// before its atomic replacement, so external edits are never overwritten.
    pub fn save(&mut self, project: &Project, path: &Path) -> Result<()> {
        project.validate()?;
        let extension = if self.library { "rplib" } else { "rp" };
        ensure!(
            path.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(extension)),
            "原生文件须保存为 .{extension}；不能通过更改扩展名转换项目与元件库"
        );
        let original_path = same_path(path, &self.source_path);
        ensure!(
            self.source.matches_original(&self.source_path)?,
            "原生文件已被其他程序修改，请重新打开后再编辑；本次未覆盖原文件"
        );
        let bytes = if *project == self.baseline {
            self.source.original_bytes()
        } else {
            let mut parts = self.parts.clone();
            self.patch_project(project, &mut parts)?;
            let mut replacements = BTreeMap::new();
            for (offset, graph) in &parts {
                let bytes = binary::encode(graph)
                    .with_context(|| format!("序列化原生部件 {offset} 失败"))?;
                // Reuse raw compression frames whenever the object graph did
                // not change. Native settings and unknown fields stay intact.
                if bytes != self.source.part(*offset)? {
                    replacements.insert(*offset, bytes);
                }
            }
            self.source.encode_with_parts(&replacements)?
        };
        // Verify the entire staged container, including a new Ingenaro read of its
        // native page content, before touching the destination file.
        let verified = super::open_container(path, container::from_bytes(&bytes)?)
            .context("原生保存结果无法重新读取，原文件保持不变")?;
        if original_path {
            self.source.write_atomic_checked(path, &bytes)?;
        } else {
            container::write_atomic(path, &bytes)?;
        }
        // A complete reopen refreshes native IDs and the coordinate mapping for
        // library galleries and newly created widgets on subsequent saves.
        *self = verified;
        Ok(())
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    if let (Ok(a), Ok(b)) = (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        return a == b;
    }
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

fn owners(parts: &BTreeMap<u64, Graph>) -> BTreeMap<String, u64> {
    parts
        .iter()
        .flat_map(|(offset, g)| {
            g.objects
                .keys()
                .map(move |id| (format!("{}/{id}", g.package), *offset))
        })
        .collect()
}

fn get_node<'a>(
    parts: &'a BTreeMap<u64, Graph>,
    owners: &BTreeMap<String, u64>,
    id: &str,
) -> Result<&'a Node> {
    owners
        .get(id)
        .and_then(|o| parts.get(o))
        .and_then(|g| g.node(raw_id(id)))
        .with_context(|| format!("原生对象 {id} 不存在"))
}

fn set_property(style: &mut Value, id: u32, value: Value) {
    if !style["PropList"].is_array() {
        style["PropList"] = json!([]);
    }
    let list = style["PropList"].as_array_mut().unwrap();
    if let Some(entry) = list.iter_mut().find(|e| {
        e["DOAName"]
            .as_i64()
            .is_some_and(|n| (n as u32) & 0x3fff_ffff == id)
    }) {
        entry["DOAName"] = json!((0xc000_0000u32 | id) as i32);
        entry["Value"] = value;
    } else {
        list.push(json!({"DOAName":(0xc000_0000u32|id) as i32,"Value":value}));
    }
}

fn color_argb(color: &str) -> Result<i32> {
    let rgb = u32::from_str_radix(color.trim_start_matches('#'), 16)?;
    Ok(if color.len() == 9 {
        ((rgb & 255) << 24) | (rgb >> 8)
    } else {
        0xff00_0000 | rgb
    } as i32)
}

fn rename_page(parts: &mut BTreeMap<u64, Graph>, package: &str, name: &str) -> Result<()> {
    for graph in parts.values_mut() {
        for node in graph
            .objects
            .values_mut()
            .filter(|n| n.kind == "Axure:PackageInfo")
        {
            if node.properties.get("package-id").and_then(reference_id) == Some(package) {
                // The editor includes folder paths for browsing. Preserve the
                // native folder tree and store only the page's final segment.
                node.properties["package-name"] = json!(name.rsplit(" / ").next().unwrap_or(name));
                return Ok(());
            }
        }
    }
    bail!("原生页面名称对象不存在")
}

fn set_page_background(parts: &mut BTreeMap<u64, Graph>, package: &str, color: &str) -> Result<()> {
    let graph = parts
        .values_mut()
        .find(|g| g.package == package)
        .context("原生页面部件不存在")?;
    let style = graph
        .root()
        .and_then(|n| n.properties.get("page-style"))
        .and_then(reference_id)
        .context("原生页面样式不存在")?
        .to_owned();
    let node = graph
        .objects
        .get_mut(&style)
        .context("原生页面样式对象不存在")?;
    set_property(
        &mut node.properties,
        7,
        json!({"fill-type":"solid","fill-color":color_argb(color)?}),
    );
    Ok(())
}

fn raw_id(id: &str) -> &str {
    id.rsplit('/').next().unwrap_or(id)
}

fn editable_native_id(baseline: &Project, bindings: &BTreeMap<u64, Binding>, id: u64) -> bool {
    bindings.contains_key(&id)
        || !baseline
            .pages
            .iter()
            .any(|page| page.widgets.iter().any(|widget| widget.id == id))
}

fn validate_native_vector_edit(original: &Node, old: &Widget, changed: &Widget) -> Result<()> {
    ensure!(
        original.kind.ends_with(":VectorShape") && changed.kind == old.kind,
        "原生路径镜像仅适用于未改变类型的矢量元件"
    );
    let source = old
        .vector_points
        .as_ref()
        .context("该元件未保留可编辑原生路径")?;
    let target = changed
        .vector_points
        .as_ref()
        .context("不能删除现有原生矢量路径")?;
    ensure!(
        original.properties.get("ControlPoints") == Some(source),
        "原生矢量路径与编辑基线不一致"
    );
    crate::vector_edit::validate_points(source)?;
    crate::vector_edit::validate_points(target)?;
    // Two reflections also cover floating-point cancellation from flipping an
    // object twice before saving. Every candidate retains all point metadata.
    for horizontal in 0..=2 {
        for vertical in 0..=2 {
            let mut points = source.clone();
            for _ in 0..horizontal {
                points = crate::vector_edit::mirrored_points(&points, true)?;
            }
            for _ in 0..vertical {
                points = crate::vector_edit::mirrored_points(&points, false)?;
            }
            if points != *target {
                continue;
            }
            let mut preview = old.image_data.clone();
            for _ in 0..horizontal {
                preview = preview
                    .as_deref()
                    .map(|image| crate::bitmap::flip_data_uri(image, true))
                    .transpose()?;
            }
            for _ in 0..vertical {
                preview = preview
                    .as_deref()
                    .map(|image| crate::bitmap::flip_data_uri(image, false))
                    .transpose()?;
            }
            if same_preview_pixels(preview.as_deref(), changed.image_data.as_deref())? {
                return Ok(());
            }
        }
    }
    bail!("原生保存仅支持可验证的路径镜像及其对应预览，不能用图片替换矢量路径")
}

fn same_preview_pixels(expected: Option<&str>, actual: Option<&str>) -> Result<bool> {
    if expected == actual {
        return Ok(true);
    }
    let (Some(expected), Some(actual)) = (expected, actual) else {
        return Ok(false);
    };
    let decode = |data: &str| -> Result<image::RgbaImage> {
        let (_, payload) = data.split_once(',').context("矢量预览图片格式无效")?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(payload)?;
        Ok(image::load_from_memory(&bytes)?.to_rgba8())
    };
    Ok(decode(expected)? == decode(actual)?)
}

type NativeListKey = (u64, String, String);
type NativeListOrders = BTreeMap<NativeListKey, Vec<Value>>;

struct NativeOrderProjection<'a> {
    parts: &'a BTreeMap<u64, Graph>,
    widgets: BTreeMap<String, u64>,
    packages: BTreeMap<String, u64>,
    previews: BTreeMap<u64, Vec<u64>>,
    lists: std::cell::RefCell<BTreeSet<NativeListKey>>,
}

impl NativeOrderProjection<'_> {
    fn object(
        &self,
        offset: u64,
        id: &str,
        orders: &NativeListOrders,
        visiting: &mut BTreeSet<(u64, String)>,
        depth: usize,
    ) -> Result<Vec<u64>> {
        ensure!(depth <= 64, "原生层级嵌套超过限制");
        ensure!(
            visiting.insert((offset, id.to_owned())),
            "原生层级包含循环引用"
        );
        let graph = self.parts.get(&offset).context("原生层级部件不存在")?;
        let node = graph.node(id).context("原生层级对象不存在")?;
        let field = if node.kind.ends_with(":Layer") {
            Some("DiagramObjects")
        } else if matches!(node.kind.as_str(), "Axure:Diagram" | "Axure:PanelDiagram")
            || node.kind.ends_with(":Table")
            || node.kind.ends_with(":MenuObject")
        {
            Some("diagram-objects")
        } else {
            None
        };
        let mut result = Vec::new();
        if let Some(field) = field {
            let key = (offset, id.to_owned(), field.to_owned());
            if node.kind.ends_with(":Layer")
                || matches!(node.kind.as_str(), "Axure:Diagram" | "Axure:PanelDiagram")
            {
                self.lists.borrow_mut().insert(key.clone());
            }
            let references = orders
                .get(&key)
                .map(Vec::as_slice)
                .unwrap_or_else(|| array(&node.properties[field]));
            let references = if matches!(node.kind.as_str(), "Axure:Diagram" | "Axure:PanelDiagram")
            {
                direct_diagram_references(graph, references)?
            } else {
                references.to_vec()
            };
            for reference in &references {
                let child = reference_id(reference).context("原生层级包含无法识别的引用")?;
                result.extend(self.object(offset, child, orders, visiting, depth + 1)?);
            }
        } else {
            if let Some(widget) = self.widgets.get(&format!("{}/{id}", graph.package)) {
                result.push(*widget);
                if let Some(previews) = self.previews.get(widget) {
                    result.extend(previews);
                }
            }
            if node.kind.ends_with(":DynamicPanel") {
                if let Some(package) = array(&node.properties["panel-diagrams"])
                    .first()
                    .and_then(reference_id)
                {
                    let state_offset = *self
                        .packages
                        .get(package)
                        .context("动态面板状态层级不存在")?;
                    let state = &self.parts[&state_offset];
                    let diagram = state
                        .root()
                        .and_then(|root| root.properties.get("diagram"))
                        .and_then(reference_id)
                        .unwrap_or(&state.root);
                    result.extend(self.object(
                        state_offset,
                        diagram,
                        orders,
                        visiting,
                        depth + 1,
                    )?);
                }
            } else if node.kind.ends_with(":Repeater")
                && let Some(diagram) = node.properties.get("BaseDiagram").and_then(reference_id)
            {
                result.extend(self.object(offset, diagram, orders, visiting, depth + 1)?);
            }
        }
        visiting.remove(&(offset, id.to_owned()));
        Ok(result)
    }
}

struct NativeGroupAtom {
    reference: Value,
    groups: Vec<u64>,
    widgets: Vec<u64>,
}

struct NativeGroupMetadata {
    layers: BTreeMap<String, u64>,
    references: BTreeMap<u64, Value>,
}

enum NativeGroupEntry {
    Object(Value),
    Group(u64, Vec<NativeGroupEntry>),
}

// Objects with no editor projection still occupy a source sibling slot. Keep
// their exact reference in their original parent; a newly wrapped visible block
// takes the last selected source slot, so intervening opaque siblings precede it.

fn transform_preview(child: &Widget, before: &Widget, after: &Widget) -> Widget {
    let mut result = child.clone();
    let same_geometry = before.x == after.x
        && before.y == after.y
        && before.width == after.width
        && before.height == after.height
        && before.rotation == after.rotation;
    if !same_geometry {
        let sx = after.width / before.width;
        let sy = after.height / before.height;
        let (old_sin, old_cos) = (-before.rotation).to_radians().sin_cos();
        let (new_sin, new_cos) = after.rotation.to_radians().sin_cos();
        let dx = child.x + child.width * 0.5 - before.x - before.width * 0.5;
        let dy = child.y + child.height * 0.5 - before.y - before.height * 0.5;
        let local_x = (dx * old_cos - dy * old_sin) * sx;
        let local_y = (dx * old_sin + dy * old_cos) * sy;
        let (sin, cos) = (child.rotation - before.rotation).to_radians().sin_cos();
        let width_scale = (sx * cos).hypot(sy * sin);
        let height_scale = (sx * sin).hypot(sy * cos);
        result.width = (child.width * width_scale).clamp(1.0, 20_000.0);
        result.height = (child.height * height_scale).clamp(1.0, 20_000.0);
        result.x = after.x + after.width * 0.5 + local_x * new_cos
            - local_y * new_sin
            - result.width * 0.5;
        result.y = after.y + after.height * 0.5 + local_x * new_sin + local_y * new_cos
            - result.height * 0.5;
        result.rotation =
            (after.rotation + (sy * sin).atan2(sx * cos).to_degrees()).rem_euclid(360.0);
        result.font_size = (child.font_size * (sx * sy).sqrt()).clamp(4.0, 512.0);
        result.radius = (child.radius * sx.min(sy)).clamp(0.0, 20_000.0);
    }
    if before.hidden != after.hidden {
        result.hidden = after.hidden || (child.hidden && !before.hidden);
    }
    if before.groups != after.groups && child.groups.starts_with(&before.groups) {
        result.groups = after
            .groups
            .iter()
            .copied()
            .chain(child.groups[before.groups.len()..].iter().copied())
            .collect();
    }
    result
}

fn matching_preview(actual: &Widget, expected: &Widget) -> bool {
    let mut compared = actual.clone();
    for (actual, expected) in [
        (&mut compared.x, expected.x),
        (&mut compared.y, expected.y),
        (&mut compared.width, expected.width),
        (&mut compared.height, expected.height),
        (&mut compared.rotation, expected.rotation),
        (&mut compared.font_size, expected.font_size),
        (&mut compared.radius, expected.radius),
    ] {
        if (*actual - expected).abs() > expected.abs().max(1.0) * 0.00001 {
            return false;
        }
        *actual = expected;
    }
    compared == *expected
}

#[cfg(test)]
mod compatibility_tests;
