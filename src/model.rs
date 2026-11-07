mod validation;
pub(crate) use validation::validate_page;
use validation::*;

mod demo;

use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_GROUP_DEPTH: usize = 32;
const MAX_PROJECT_BYTES: u64 = 48 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 12 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub pages: Vec<Page>,
    /// Navigation order and folders. Empty retains the flat order of older files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub page_tree: Vec<PageTreeNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interaction_events: Vec<AuthoredEvent>,
    pub next_id: u64,
}

/// Editable authored cases. Native documents keep every other source case intact.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AuthoredEvent {
    pub id: String,
    pub page: u64,
    pub owner: Option<u64>,
    pub trigger: EventTrigger,
    pub name: String,
    pub actions: Vec<Interaction>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventTrigger {
    PageLoad,
    Click,
}

impl EventTrigger {
    pub fn native_type(self) -> i64 {
        match self {
            Self::PageLoad => 320,
            Self::Click => 16,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::PageLoad => "页面载入时",
            Self::Click => "单击时",
        }
    }
}

impl AuthoredEvent {
    pub fn new(page: u64, owner: Option<u64>, trigger: EventTrigger) -> Self {
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            ^ ((std::process::id() as u128) << 64)
            ^ SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as u128;
        Self {
            id: format!("72707368{:024x}", nonce & 0xffff_ffff_ffff_ffff_ffff_ffff),
            page,
            owner,
            trigger,
            name: "用例1".into(),
            actions: Vec::new(),
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PageTreeNode {
    Folder {
        id: String,
        name: String,
        #[serde(default)]
        children: Vec<PageTreeNode>,
    },
    Page {
        page_id: u64,
        #[serde(default)]
        children: Vec<PageTreeNode>,
    },
}

// RON's enum-tag reader does not roundtrip this internally tagged enum in
// desktop session storage. A typed record keeps the same JSON/RON output and
// reads the tag as a string without Serde's buffered enum representation.
impl<'de> Deserialize<'de> for PageTreeNode {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Record {
            #[serde(rename = "type")]
            kind: String,
            #[serde(default)]
            id: String,
            #[serde(default)]
            name: String,
            #[serde(default)]
            page_id: u64,
            #[serde(default)]
            children: Vec<PageTreeNode>,
        }
        let record = Record::deserialize(deserializer)?;
        match record.kind.as_str() {
            "folder" => Ok(Self::Folder {
                id: record.id,
                name: record.name,
                children: record.children,
            }),
            "page" => Ok(Self::Page {
                page_id: record.page_id,
                children: record.children,
            }),
            _ => Err(serde::de::Error::unknown_variant(
                &record.kind,
                &["folder", "page"],
            )),
        }
    }
}

impl PageTreeNode {
    pub fn children(&self) -> &[Self] {
        match self {
            Self::Folder { children, .. } | Self::Page { children, .. } => children,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Page {
    pub id: u64,
    pub name: String,
    pub width: f32,
    pub height: f32,
    /// Auto pages are an unbounded editor surface; width/height remain export bounds.
    #[serde(default)]
    pub canvas: CanvasMode,
    /// Optional minimum viewport width for a responsive page, in design pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responsive_min_width: Option<f32>,
    pub background: String,
    pub widgets: Vec<Widget>,
    /// Custom names for groups whose IDs occur in this page's widget paths.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub group_names: BTreeMap<u64, String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CanvasMode {
    #[default]
    Fixed,
    Auto,
    FixedWidth,
    FixedHeight,
    ResponsiveWidth,
}

impl CanvasMode {
    pub fn is_auto(self) -> bool {
        self.auto_width() || self.auto_height()
    }
    pub fn auto_width(self) -> bool {
        matches!(self, Self::Auto | Self::FixedHeight)
    }
    pub fn auto_height(self) -> bool {
        matches!(self, Self::Auto | Self::FixedWidth)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TextLayout {
    /// Native alignment: 0 = start, 1 = center, 2 = end.
    pub horizontal: u8,
    pub vertical: u8,
    /// Left, top, right, bottom in document pixels.
    pub padding: [f32; 4],
    pub wrap: bool,
    pub bold: bool,
}

impl Page {
    pub fn responsive_min_width(&self) -> Option<f32> {
        (self.canvas == CanvasMode::ResponsiveWidth)
            .then(|| self.responsive_min_width.unwrap_or(self.width.min(320.0)))
    }

    pub fn group_name(&self, id: u64) -> Cow<'_, str> {
        self.group_names.get(&id).map_or_else(
            || Cow::Owned(format!("分组 {id}")),
            |name| Cow::Borrowed(name.as_str()),
        )
    }

    pub fn prune_group_names(&mut self) {
        if !self.group_names.is_empty() {
            let groups = self
                .widgets
                .iter()
                .flat_map(|w| &w.groups)
                .collect::<HashSet<_>>();
            self.group_names.retain(|id, _| groups.contains(id));
        }
    }

    /// Finite preview/export dimensions for otherwise unbounded editor pages.
    pub fn size_for_output(&self) -> [f32; 2] {
        let mut size = [self.width, self.height];
        if self.canvas.auto_width() {
            size[0] = 64.0;
        }
        if self.canvas.auto_height() {
            size[1] = 64.0;
        }
        for widget in self.widgets.iter().filter(|widget| !widget.hidden) {
            let bounds = crate::manipulation::rotated_bounds(widget);
            if self.canvas.auto_width() {
                size[0] = size[0].max(bounds[2]);
            }
            if self.canvas.auto_height() {
                size[1] = size[1].max(bounds[3]);
            }
        }
        size
    }
}

#[cfg(test)]
mod canvas_metadata_tests;

/// Percentage of the viewport width change that each edge follows.
/// At the design width every component keeps its authored geometry.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct HorizontalLayout {
    pub left: f32,
    pub right: f32,
}

impl HorizontalLayout {
    pub const LEFT: Self = Self {
        left: 0.0,
        right: 0.0,
    };
    pub const RIGHT: Self = Self {
        left: 100.0,
        right: 100.0,
    };
    pub const STRETCH: Self = Self {
        left: 0.0,
        right: 100.0,
    };

    fn is_default(&self) -> bool {
        *self == Self::LEFT
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Widget {
    pub id: u64,
    pub name: String,
    pub kind: WidgetKind,
    /// None uses the independent editor's same-name radio groups. An explicit
    /// empty group preserves an ungrouped native radio button.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radio_group: Option<String>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default, skip_serializing_if = "HorizontalLayout::is_default")]
    pub horizontal_layout: HorizontalLayout,
    /// Clockwise degrees around the widget's center.
    #[serde(default)]
    pub rotation: f32,
    /// Group IDs from the outermost group to the innermost group.
    #[serde(default)]
    pub groups: Vec<u64>,
    /// Native normalized vector contours, when available for shape editing.
    #[serde(default)]
    pub vector_points: Option<serde_json::Value>,
    pub text: String,
    pub fill: String,
    pub stroke: String,
    pub text_color: String,
    pub font_size: f32,
    /// Optional source layout; absent keeps the independent editor's legacy defaults.
    #[serde(default)]
    pub text_layout: Option<TextLayout>,
    pub radius: f32,
    pub opacity: f32,
    pub hidden: bool,
    pub locked: bool,
    pub interaction: Interaction,
    pub image_data: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetKind {
    Rectangle,
    Ellipse,
    Text,
    Button,
    Input,
    Checkbox,
    Radio,
    TextArea,
    Droplist,
    ListBox,
    Line,
    Image,
    Panel,
}

impl WidgetKind {
    pub const ALL: [Self; 13] = [
        Self::Rectangle,
        Self::Ellipse,
        Self::Text,
        Self::Button,
        Self::Input,
        Self::Checkbox,
        Self::Radio,
        Self::TextArea,
        Self::Droplist,
        Self::ListBox,
        Self::Line,
        Self::Image,
        Self::Panel,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "矩形",
            Self::Ellipse => "椭圆",
            Self::Text => "文本",
            Self::Button => "按钮",
            Self::Input => "输入框",
            Self::Checkbox => "复选框",
            Self::Radio => "单选按钮",
            Self::TextArea => "文本域",
            Self::Droplist => "下拉列表",
            Self::ListBox => "列表框",
            Self::Line => "直线",
            Self::Image => "图片",
            Self::Panel => "容器",
        }
    }

    /// Native plain text controls store their text on the control itself.
    pub fn is_plain_text_control(self) -> bool {
        matches!(
            self,
            Self::Input | Self::TextArea | Self::Droplist | Self::ListBox
        )
    }

    pub fn is_option_list(self) -> bool {
        matches!(self, Self::Droplist | Self::ListBox)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Interaction {
    pub action: Action,
    pub target_page: Option<u64>,
    pub target_widget: Option<u64>,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Action {
    #[default]
    None,
    Navigate,
    ToggleVisibility,
    SetText,
}

impl Widget {
    pub fn new(id: u64, kind: WidgetKind, x: f32, y: f32) -> Self {
        let (width, height, text) = match kind {
            WidgetKind::Text => (180.0, 32.0, "双击编辑文本"),
            WidgetKind::Button => (140.0, 44.0, "按钮"),
            WidgetKind::Input => (240.0, 44.0, "请输入内容"),
            WidgetKind::Checkbox => (180.0, 32.0, "复选框"),
            WidgetKind::Radio => (180.0, 32.0, "选项"),
            WidgetKind::TextArea => (300.0, 140.0, "请输入多行内容"),
            WidgetKind::Droplist => (220.0, 40.0, "选项 1\n选项 2\n选项 3"),
            WidgetKind::ListBox => (220.0, 132.0, "选项 1\n选项 2\n选项 3"),
            WidgetKind::Line => (240.0, 2.0, ""),
            WidgetKind::Image => (240.0, 160.0, "图片"),
            WidgetKind::Panel => (360.0, 240.0, ""),
            WidgetKind::Ellipse => (120.0, 120.0, ""),
            WidgetKind::Rectangle => (180.0, 100.0, ""),
        };
        Self {
            id,
            name: format!("{} {}", kind.label(), id),
            kind,
            radio_group: None,
            x,
            y,
            width,
            height,
            horizontal_layout: HorizontalLayout::default(),
            rotation: 0.0,
            groups: Vec::new(),
            vector_points: None,
            text: text.to_owned(),
            fill: match kind {
                WidgetKind::Text | WidgetKind::Checkbox | WidgetKind::Radio => "#FFFFFF00",
                WidgetKind::Button => "#6759E8",
                WidgetKind::Line => "#DDE2EB",
                _ => "#FFFFFF",
            }
            .to_owned(),
            stroke: match kind {
                WidgetKind::Text
                | WidgetKind::Checkbox
                | WidgetKind::Radio
                | WidgetKind::Button
                | WidgetKind::Line => "#FFFFFF00",
                _ => "#DDE2EB",
            }
            .to_owned(),
            text_color: if kind == WidgetKind::Button {
                "#FFFFFF"
            } else {
                "#243047"
            }
            .to_owned(),
            font_size: 16.0,
            text_layout: None,
            radius: if matches!(kind, WidgetKind::Text | WidgetKind::Line) {
                0.0
            } else {
                10.0
            },
            opacity: 1.0,
            hidden: false,
            locked: false,
            interaction: Interaction::default(),
            image_data: None,
        }
    }
}

impl Project {
    pub fn blank() -> Self {
        Self {
            version: FORMAT_VERSION,
            name: "未命名项目".to_owned(),
            pages: vec![Page {
                id: 1,
                name: "页面 1".to_owned(),
                width: 1280.0,
                height: 800.0,
                canvas: CanvasMode::Fixed,
                responsive_min_width: None,
                background: "#FFFFFF".to_owned(),
                widgets: Vec::new(),
                group_names: BTreeMap::new(),
            }],
            page_tree: Vec::new(),
            interaction_events: Vec::new(),
            next_id: 2,
        }
    }

    pub fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).expect("项目 ID 已耗尽");
        id
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == FORMAT_VERSION,
            "不支持的项目版本：{}",
            self.version
        );
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= 1024,
            "项目名称不能为空且不能超过 1024 字节"
        );
        ensure!(
            !self.pages.is_empty() && self.pages.len() <= 128,
            "项目应包含 1–128 个页面"
        );
        let mut ids = HashSet::new();
        let mut page_ids = HashSet::new();
        let mut widget_ids = HashSet::new();
        let mut count = 0usize;
        for page in &self.pages {
            validate_page(page).with_context(|| format!("页面「{}」无效", page.name))?;
            ensure!(
                page.id > 0 && ids.insert(page.id),
                "页面 ID 重复或为零：{}",
                page.id
            );
            page_ids.insert(page.id);
            for widget in &page.widgets {
                ensure!(
                    widget.id > 0 && ids.insert(widget.id),
                    "元件 ID 重复或为零：{}",
                    widget.id
                );
                widget_ids.insert(widget.id);
                count += 1;
            }
        }
        ensure!(count <= 20_000, "项目最多包含 20000 个元件");
        validate_page_tree(&self.page_tree, &page_ids)?;
        let mut groups: BTreeMap<u64, (u64, Vec<u64>, usize)> = BTreeMap::new();
        for page in &self.pages {
            for (position, widget) in page.widgets.iter().enumerate() {
                for (depth, group) in widget.groups.iter().enumerate() {
                    ensure!(
                        !ids.contains(group),
                        "组合 ID 与页面或元件 ID 冲突：{group}"
                    );
                    let prefix = &widget.groups[..=depth];
                    if let Some((owner, path, previous)) = groups.get_mut(group) {
                        ensure!(*owner == page.id, "同一组合不能跨越多个页面：{group}");
                        ensure!(path.as_slice() == prefix, "组合的父级路径不一致：{group}");
                        ensure!(
                            *previous + 1 == position,
                            "同一组合的成员必须在图层中连续排列：{group}"
                        );
                        *previous = position;
                    } else {
                        groups.insert(*group, (page.id, prefix.to_vec(), position));
                    }
                }
            }
        }
        ids.extend(groups.keys().copied());
        let highest = ids.iter().copied().max().unwrap_or(0);
        ensure!(
            self.next_id > highest && self.next_id < u64::MAX,
            "next_id 必须大于所有页面、元件和组合 ID"
        );
        for page in &self.pages {
            for widget in &page.widgets {
                let interaction = &widget.interaction;
                match interaction.action {
                    Action::None => {}
                    Action::Navigate => ensure!(
                        interaction
                            .target_page
                            .is_some_and(|id| page_ids.contains(&id)),
                        "元件「{}」的跳转页面不存在",
                        widget.name
                    ),
                    Action::ToggleVisibility | Action::SetText => ensure!(
                        interaction
                            .target_widget
                            .is_some_and(|id| widget_ids.contains(&id)),
                        "元件「{}」的目标元件不存在",
                        widget.name
                    ),
                }
            }
        }
        ensure!(
            self.interaction_events.len() <= 4096,
            "最多创建4096个交互用例"
        );
        let mut event_ids = HashSet::new();
        for event in &self.interaction_events {
            ensure!(
                event.id.len() == 32
                    && event.id.bytes().all(|b| b.is_ascii_hexdigit())
                    && event_ids.insert(&event.id),
                "交互标识无效或重复"
            );
            ensure!(
                !event.name.trim().is_empty() && event.name.len() <= 1024,
                "交互用例名称不能为空且不能超过1024字节"
            );
            let page = self
                .pages
                .iter()
                .find(|page| page.id == event.page)
                .context("交互所在页面不存在")?;
            ensure!(
                matches!(
                    (event.trigger, event.owner),
                    (EventTrigger::PageLoad, None) | (EventTrigger::Click, Some(_))
                ),
                "交互事件与页面/元件目标不一致"
            );
            if let Some(owner) = event.owner {
                ensure!(
                    page.widgets.iter().any(|w| w.id == owner),
                    "交互所属元件不在当前页面"
                );
            }
            ensure!(event.actions.len() <= 256, "每个用例最多256个动作");
            for action in &event.actions {
                ensure!(action.value.len() <= 65536, "交互文字不能超过65536字节");
                match action.action {
                    Action::None => bail!("交互动作不能为空"),
                    Action::Navigate => ensure!(
                        action.target_page.is_none_or(|id| page_ids.contains(&id)),
                        "交互跳转目标不存在"
                    ),
                    Action::ToggleVisibility | Action::SetText => ensure!(
                        action
                            .target_widget
                            .is_some_and(|id| page.widgets.iter().any(|w| w.id == id)),
                        "交互目标元件不在当前页面"
                    ),
                }
            }
        }
        Ok(())
    }

    pub fn prune_interaction_references(&mut self) {
        let pages = self
            .pages
            .iter()
            .map(|page| {
                (
                    page.id,
                    page.widgets.iter().map(|w| w.id).collect::<HashSet<_>>(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        self.interaction_events.retain_mut(|event| {
            let Some(widgets) = pages.get(&event.page) else {
                return false;
            };
            if event.owner.is_some_and(|id| !widgets.contains(&id)) {
                return false;
            }
            event.actions.retain(|action| match action.action {
                Action::None => false,
                Action::Navigate => action.target_page.is_none_or(|id| pages.contains_key(&id)),
                Action::ToggleVisibility | Action::SetText => {
                    action.target_widget.is_some_and(|id| widgets.contains(&id))
                }
            });
            true
        });
    }

    pub fn duplicate_interaction_events(&mut self, page: u64, mapping: &BTreeMap<u64, u64>) {
        let copied = self
            .interaction_events
            .iter()
            .filter(|event| {
                event.page == page && event.owner.is_some_and(|id| mapping.contains_key(&id))
            })
            .map(|event| {
                let mut copied =
                    AuthoredEvent::new(page, event.owner.map(|id| mapping[&id]), event.trigger);
                copied.name = event.name.clone();
                copied.actions = event.actions.clone();
                for action in &mut copied.actions {
                    if let Some(target) = action
                        .target_widget
                        .and_then(|id| mapping.get(&id))
                        .copied()
                    {
                        action.target_widget = Some(target);
                    }
                }
                copied
            })
            .collect::<Vec<_>>();
        self.interaction_events.extend(copied);
    }

    /// Write to a sibling temporary file, flush it, then atomically replace the destination.
    /// A failed write never truncates an existing project.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self).context("无法序列化项目")?;
        ensure!(
            bytes.len() as u64 <= MAX_PROJECT_BYTES,
            "项目文件超过 48 MB 限制"
        );
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        ensure!(parent.is_dir(), "保存目录不存在：{}", parent.display());
        let name = path
            .file_name()
            .context("保存路径缺少文件名")?
            .to_string_lossy();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut temporary = None;
        for attempt in 0..16 {
            let candidate = parent.join(format!(
                ".{name}.{}.{nonce}.{attempt}.tmp",
                std::process::id()
            ));
            match OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&candidate)
            {
                Ok(file) => {
                    temporary = Some((candidate, file));
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error).context("无法创建临时项目文件"),
            }
        }
        let (temporary_path, mut file) = temporary.context("无法创建唯一临时项目文件")?;
        let result = (|| -> Result<()> {
            file.write_all(&bytes).context("写入项目失败")?;
            file.sync_all().context("同步项目失败")?;
            drop(file);
            atomic_replace(&temporary_path, path).context("替换项目文件失败")?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }

    pub fn load(path: &Path) -> Result<Self> {
        let file = fs::File::open(path).context("无法打开项目文件")?;
        let metadata = file.metadata().context("无法读取项目文件信息")?;
        ensure!(
            metadata.is_file() && metadata.len() <= MAX_PROJECT_BYTES,
            "项目必须是小于 48 MB 的普通文件"
        );
        let mut bytes = Vec::new();
        file.take(MAX_PROJECT_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("无法读取项目文件")?;
        ensure!(
            bytes.len() as u64 <= MAX_PROJECT_BYTES,
            "项目文件超过 48 MB 限制"
        );
        let project: Self =
            serde_json::from_slice(&bytes).context("项目格式无效，预期为 .ingenaro 或旧版 .rpshop JSON 文件")?;
        project.validate()?;
        Ok(project)
    }
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let mut source_wide: Vec<u16> = source.as_os_str().encode_wide().collect();
    let mut destination_wide: Vec<u16> = destination.as_os_str().encode_wide().collect();
    if source_wide.contains(&0) || destination_wide.contains(&0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "路径包含 NUL 字符",
        ));
    }
    source_wide.push(0);
    destination_wide.push(0);
    // Both paths share a directory, so this never falls back to a cross-volume copy.
    let result = unsafe { MoveFileExW(source_wide.as_ptr(), destination_wide.as_ptr(), 0x1 | 0x8) };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(test)]
mod tests;
