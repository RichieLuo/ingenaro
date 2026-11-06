mod ai_reveal;
mod ai_workbench;
mod arrange;
mod canvas;
mod dialog_style;
#[cfg(test)]
mod dialog_tests;
mod dialogs;
mod document;
mod editing;
mod embedded_preview;
mod file_open;
mod keyboard;
mod layers;
mod library;
mod preview;
mod shell;
mod startup;
mod theme;
#[cfg(target_os = "windows")]
mod title_bar;
#[cfg(target_os = "windows")]
mod window_chrome;
use theme::setup_style;

use base64::Engine;
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, vec2,
};
use rpshop::manipulation::{self, Handle};
use rpshop::model::{Action, Page, Project, Widget, WidgetKind};
use rpshop::widget_library::{self, Category, LibraryItem};
use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    time::{Duration, Instant},
};

#[cfg(test)]
use eframe::egui::Vec2;

const PURPLE: Color32 = Color32::from_rgb(117, 83, 222);
const INK: Color32 = Color32::from_rgb(41, 43, 58);
const MUTED: Color32 = Color32::from_rgb(136, 140, 157);
const BORDER: Color32 = Color32::from_rgb(231, 232, 240);
const LAYER_CONTROL_WIDTH: f32 = 32.0;
const LAYER_COLUMN_SPACING: f32 = 4.0;

#[cfg(test)]
mod tests;

mod render;
use render::draw_widget;
mod viewport;
use viewport::{CanvasView, PanDrag, ViewCommand};

mod page_tree;

mod sidebar;

mod inspector;

mod interactions;

#[derive(Clone)]
struct Drag {
    before: Project,
    pointer: Pos2,
    mode: DragMode,
}

#[derive(Clone)]
enum DragMode {
    Move,
    Draw {
        id: u64,
        kind: WidgetKind,
        start: [f32; 2],
    },
    Resize {
        handle: Handle,
        bounds: Box<Widget>,
    },
    Rotate {
        pivot: [f32; 2],
        angle: f32,
        start: [f32; 2],
    },
    Marquee {
        initial: BTreeSet<u64>,
    },
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Session {
    project: Project,
    path: Option<PathBuf>,
    saved: Project,
    #[serde(default)]
    import_warnings: Vec<String>,
    #[serde(default)]
    native_state: Option<serde_json::Value>,
    #[serde(default)]
    ai_document_id: String,
}

struct OpenedDocument {
    project: Project,
    save_path: Option<PathBuf>,
    warnings: Vec<String>,
    native: Option<rpshop::rp_format::NativeDocument>,
}

fn is_native_path(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("rp") || e.eq_ignore_ascii_case("rplib"))
}

fn restore_native_context(
    document: &mut rpshop::rp_format::NativeDocument,
    state: Option<&serde_json::Value>,
    draft: &mut Project,
    saved: &mut Project,
) -> anyhow::Result<bool> {
    let Some(state) = state else {
        anyhow::ensure!(document.project == *saved, "原文件与保存检查点不一致");
        return Ok(false);
    };
    let old_tree_missing = state["project"]
        .get("page_tree")
        .is_none_or(|tree| tree.as_array().is_some_and(Vec::is_empty));
    if state["version"].as_u64() == Some(2) {
        let previous: Project = serde_json::from_value(state["project"].clone())?;
        let fresh = &document.project;
        if previous == *fresh {
            document.restore_session_state(state)?;
            return Ok(false);
        }
        anyhow::ensure!(*saved == previous, "旧版草稿的保存检查点不匹配");
        let ids = |project: &Project| project.pages.iter().map(|p| p.id).collect::<Vec<_>>();
        anyhow::ensure!(
            ids(&previous) == ids(fresh) && ids(draft) == ids(&previous),
            "旧版草稿页面绑定与源文件不匹配"
        );
        let mut normalized = previous.clone();
        let mut migrated_draft = draft.clone();
        if old_tree_missing && !fresh.page_tree.is_empty() {
            anyhow::ensure!(draft.page_tree.is_empty(), "旧版草稿页面目录不匹配");
            normalized.page_tree = fresh.page_tree.clone();
            migrated_draft.page_tree = fresh.page_tree.clone();
            for ((page, old), source) in migrated_draft
                .pages
                .iter_mut()
                .zip(&previous.pages)
                .zip(&fresh.pages)
            {
                if page.name == old.name {
                    page.name = source.name.clone();
                }
            }
            for (page, source) in normalized.pages.iter_mut().zip(&fresh.pages) {
                page.name = source.name.clone();
            }
        }
        let raw_pages = state["project"]["pages"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("旧版原生页面元数据无效"))?;
        for (index, ((page, old), source)) in normalized
            .pages
            .iter_mut()
            .zip(&previous.pages)
            .zip(&fresh.pages)
            .enumerate()
        {
            let raw = &raw_pages[index];
            let draft_page = &mut migrated_draft.pages[index];
            if raw.get("group_names").is_none() {
                page.group_names = source.group_names.clone();
                for (id, name) in &source.group_names {
                    draft_page
                        .group_names
                        .entry(*id)
                        .or_insert_with(|| name.clone());
                }
                draft_page.prune_group_names();
            }
            // Before 0.4.1 native dimensions were a preview fallback. Only
            // upgrade metadata absent from that baseline, preserving draft edits.
            if raw.get("canvas").is_none() {
                page.canvas = source.canvas;
                page.width = source.width;
                page.height = source.height;
                if draft_page.canvas == old.canvas {
                    draft_page.canvas = source.canvas;
                }
                if draft_page.width == old.width {
                    draft_page.width = source.width;
                }
                if draft_page.height == old.height {
                    draft_page.height = source.height;
                }
            }
            let raw_widgets = raw["widgets"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("旧版原生组件元数据无效"))?;
            for (widget, raw_widget) in page.widgets.iter_mut().zip(raw_widgets) {
                if raw_widget.get("text_layout").is_none() {
                    let Some(source_widget) = source.widgets.iter().find(|w| w.id == widget.id)
                    else {
                        continue;
                    };
                    if let Some(draft_widget) =
                        draft_page.widgets.iter_mut().find(|w| w.id == widget.id)
                        && draft_widget.text_layout == widget.text_layout
                    {
                        draft_widget.text_layout = source_widget.text_layout.clone();
                    }
                    widget.text_layout = source_widget.text_layout.clone();
                }
            }
        }
        anyhow::ensure!(
            normalized == *fresh,
            "旧版保存检查点与源文件有显示元数据以外的差异"
        );
        let migrated_saved = fresh.clone();
        let mut migrated_state = state.clone();
        migrated_state["project"] = serde_json::to_value(fresh)?;
        // Commit the draft migration only after fingerprint, object, group, and
        // preview bindings have all passed the normal native restore checks.
        document.restore_session_state(&migrated_state)?;
        *draft = migrated_draft;
        *saved = migrated_saved;
        Ok(true)
    } else {
        document.restore_session_state(state)?;
        Ok(false)
    }
}

fn load_document(path: &std::path::Path) -> anyhow::Result<OpenedDocument> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "rp" | "rplib" => {
            let result = rpshop::rp_format::open(path)?;
            Ok(OpenedDocument {
                project: result.project.clone(),
                save_path: Some(path.to_path_buf()),
                warnings: result.warnings.clone(),
                native: Some(result),
            })
        }
        "rpshop" | "json" => Ok(OpenedDocument {
            project: Project::load(path)?,
            save_path: Some(path.to_path_buf()),
            warnings: Vec::new(),
            native: None,
        }),
        other => {
            anyhow::bail!("暂不支持 .{other} 文件；请选择 .rp、.rplib 或 .rpshop 工程")
        }
    }
}

pub struct App {
    ai: ai_workbench::Workbench,
    ai_reveal: ai_reveal::DesignReveal,
    project: Project,
    saved: Project,
    page: usize,
    selected: BTreeSet<u64>,
    path: Option<PathBuf>,
    undo: Vec<Project>,
    redo: Vec<Project>,
    clipboard: editing::ComponentClipboard,
    expanded_layer_groups: BTreeSet<u64>,
    layer_name_dialog: Option<layers::NameDialog>,
    zoom: f32,
    view: CanvasView,
    sidebar_layout: sidebar::SidebarLayout,
    fit: bool,
    grid: bool,
    snap: bool,
    tab: usize,
    tool: Option<WidgetKind>,
    drag: Option<Drag>,
    pending_widget: Option<library::LibraryDrag>,
    library_query: String,
    library_category: Option<Category>,
    browser_preview: Option<rpshop::preview::PreviewServer>,
    textures: HashMap<u64, egui::TextureHandle>,
    logo: Option<egui::TextureHandle>,
    status: String,
    last_recovery: Instant,
    screenshot: Option<PathBuf>,
    screenshot_mode: bool,
    history_changed: bool,
    editing: Option<u64>,
    frames: u32,
    confirm: Option<String>,
    help: bool,
    import_warnings: Vec<String>,
    pending_open: Option<PathBuf>,
    native: Option<rpshop::rp_format::NativeDocument>,
    cached_native_preview_source:
        Option<(usize, std::sync::Arc<rpshop::rp_format::NativeDocument>)>,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.poll_shell_open(ctx);
        self.poll_ai(ctx);
        self.ai_handle_image_input(ctx);
        ctx.options_mut(|options| options.zoom_with_keyboard = false);
        self.frames += 1;
        for event in ctx.input(|i| i.events.clone()) {
            if let egui::Event::Screenshot { image, .. } = event
                && let Some(path) = self.screenshot.take()
            {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = image::save_buffer(
                    &path,
                    image.as_raw(),
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                ) {
                    eprintln!("截图失败: {e}");
                }
                if !std::env::args().any(|arg| arg == "--qa-interactive") {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        if self.layer_name_dialog.is_none()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.view.panning = None;
            self.pending_widget = None;
            if let Some(drag) = self.drag.take() {
                self.project = drag.before;
                self.publish_preview();
            }
            self.editing = None;
            self.tool = None;
        }
        if self.pending_widget.is_some()
            && ctx.input(|i| {
                !i.focused || (!i.pointer.primary_down() && !i.pointer.primary_released())
            })
        {
            self.pending_widget = None;
        }
        if self.confirm.is_none()
            && self.editing.is_none()
            && self.layer_name_dialog.is_none()
            && !self.ai_modal_active()
            && !self.ai_page_overview_visible()
        {
            self.viewport_shortcuts(ctx);
            self.keyboard(ctx);
        }
        if !self.screenshot_mode
            && self.confirm.is_none()
            && let Some(path) =
                ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()))
        {
            self.pending_open = Some(path);
            self.request_replace("open-path");
        }
        let before = self.project.clone();
        self.history_changed = false;
        let dragging = self.drag.is_some();
        self.header(ctx);
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(18, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        vec2((ui.available_width() - 330.0).max(120.0), 18.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&self.status).size(11.0).color(MUTED),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&self.status);
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} 个页面  ·  {} 个组件",
                                self.project.pages.len(),
                                self.project.pages[self.page].widgets.len()
                            ))
                            .size(11.0)
                            .color(MUTED),
                        );
                        self.ai_overview_return_button(ui);
                    });
                });
            });
        self.activity_bars(ctx);
        self.left_panel(ctx);
        if self.ai.visible {
            self.ai_chat_panel(ctx);
        } else if self.sidebar_layout.inspector_open {
            self.inspector(ctx);
        }
        self.load_textures(ctx);
        if self.ai_page_overview_visible() {
            self.pending_widget = None;
            self.ai_page_overview(ctx);
        } else {
            self.canvas(ctx);
        }
        self.ai_artifact_window(ctx, frame);
        self.ai_settings(ctx);
        self.ai_permissions(ctx);
        self.ai_full_permission_dialog(ctx);
        self.ai_image_preview(ctx);
        self.ai_palette_dialog(ctx);
        self.show_layer_name_dialog(ctx);
        self.dialogs(ctx);
        #[cfg(target_os = "windows")]
        window_chrome::paint_border(ctx);
        if !dragging && self.drag.is_none() && !self.history_changed {
            self.remember(before);
        }
        if ctx.input(|i| i.viewport().close_requested())
            && self.project != self.saved
            && !self.screenshot_mode
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some("close".into());
        }
        if self.last_recovery.elapsed() > Duration::from_secs(5) && !self.screenshot_mode {
            if let Some(dir) = directories::ProjectDirs::from("", "RPShop", "RPShop") {
                let _ = std::fs::create_dir_all(dir.data_local_dir());
                let mut draft = self.project.clone();
                normalize_draft(&mut draft);
                if let Err(e) = draft.save(&dir.data_local_dir().join("recovery.rpshop")) {
                    self.status = format!("恢复备份失败：{e:#}");
                }
            }
            self.last_recovery = Instant::now();
        }
        if std::env::args().any(|arg| arg == "--qa-interactive")
            && self.screenshot_mode
            && let Ok(path) = std::env::var("RPSHOP_QA_CAPTURE_REQUEST")
            && let Ok(destination) = std::fs::read_to_string(&path)
        {
            self.screenshot = Some(PathBuf::from(destination.trim()));
            let _ = std::fs::remove_file(path);
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        if self.frames == 5 && self.screenshot.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if !self.screenshot_mode {
            eframe::set_value(storage, sidebar::STORAGE_KEY, &self.sidebar_layout);
            eframe::set_value(
                storage,
                "rpshop-session",
                &Session {
                    project: self.project.clone(),
                    path: self.path.clone(),
                    saved: self.saved.clone(),
                    import_warnings: self.import_warnings.clone(),
                    native_state: self.native.as_ref().map(|n| n.session_state()),
                    ai_document_id: self.ai_document_identity(),
                },
            );
        }
    }
    fn auto_save_interval(&self) -> Duration {
        Duration::from_secs(5)
    }
}

fn layer_title_width(row_width: f32) -> f32 {
    (row_width - 2.0 * LAYER_CONTROL_WIDTH - 2.0 * LAYER_COLUMN_SPACING).max(0.0)
}

#[derive(Clone, Copy)]
enum LayerControl {
    Visibility { hidden: bool },
    Lock { locked: bool },
}

fn layer_control(ui: &mut egui::Ui, control: LayerControl, name: &str) -> egui::Response {
    let (checked, emphasized, description) = match control {
        LayerControl::Visibility { hidden: true } => {
            (false, true, "当前已隐藏；点击显示。隐藏不会删除组件。")
        }
        LayerControl::Visibility { hidden: false } => (
            true,
            false,
            "当前已显示；点击隐藏。隐藏后画布和预览中都不可见。",
        ),
        LayerControl::Lock { locked: true } => (
            true,
            true,
            "当前已锁定，不能移动、缩放或修改；点击解锁以恢复编辑。",
        ),
        LayerControl::Lock { locked: false } => {
            (false, false, "当前未锁定，可以编辑；点击锁定以防止误改。")
        }
    };
    let label = format!("{name}：{description}");
    let (rect, response) = ui.allocate_exact_size(vec2(LAYER_CONTROL_WIDTH, 27.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), checked, &label)
    });
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let painter = ui.painter();
        painter.rect(
            rect.shrink(1.0),
            4.0,
            if emphasized {
                Color32::from_rgb(240, 235, 255)
            } else {
                visuals.weak_bg_fill
            },
            visuals.bg_stroke,
            StrokeKind::Inside,
        );
        let color = if !ui.is_enabled() {
            MUTED
        } else if emphasized {
            PURPLE
        } else {
            INK
        };
        let stroke = Stroke::new(1.4, color);
        let center = rect.center();
        match control {
            LayerControl::Visibility { hidden } => {
                for direction in [-1.0, 1.0] {
                    painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                        [
                            center + vec2(-8.0, 0.0),
                            center + vec2(-4.0, 7.0 * direction),
                            center + vec2(4.0, 7.0 * direction),
                            center + vec2(8.0, 0.0),
                        ],
                        false,
                        Color32::TRANSPARENT,
                        stroke,
                    ));
                }
                painter.circle_stroke(center, 2.3, stroke);
                if hidden {
                    painter.line_segment(
                        [center + vec2(-9.0, -7.0), center + vec2(9.0, 7.0)],
                        Stroke::new(1.8, color),
                    );
                }
            }
            LayerControl::Lock { locked } => {
                let shackle = if locked {
                    [
                        vec2(-4.0, -1.0),
                        vec2(-4.0, -9.0),
                        vec2(4.0, -9.0),
                        vec2(4.0, -1.0),
                    ]
                } else {
                    [
                        vec2(-3.0, -1.0),
                        vec2(-3.0, -9.0),
                        vec2(7.0, -9.0),
                        vec2(7.0, -2.0),
                    ]
                };
                painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                    shackle.map(|point| center + point),
                    false,
                    Color32::TRANSPARENT,
                    stroke,
                ));
                painter.rect_stroke(
                    Rect::from_min_max(center + vec2(-6.0, -1.0), center + vec2(6.0, 7.0)),
                    2.0,
                    stroke,
                    StrokeKind::Inside,
                );
                painter.circle_filled(center + vec2(0.0, 2.0), 1.1, color);
                painter.line_segment([center + vec2(0.0, 2.0), center + vec2(0.0, 4.5)], stroke);
            }
        }
    }
    response.on_hover_text(label)
}

fn action_label(action: Action) -> &'static str {
    match action {
        Action::None => "无交互",
        Action::Navigate => "跳转页面",
        Action::ToggleVisibility => "切换组件显示",
        Action::SetText => "设置组件文本",
    }
}

fn normalize_draft(project: &mut Project) {
    if project.name.trim().is_empty() {
        project.name = "未命名项目".into();
    }
    for (i, page) in project.pages.iter_mut().enumerate() {
        if page.name.trim().is_empty() {
            page.name = format!("页面 {}", i + 1);
        }
    }
}

fn color(hex: &str) -> Color32 {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 && h.len() != 8 {
        return Color32::GRAY;
    }
    let read = |i| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0);
    Color32::from_rgba_unmultiplied(
        read(0),
        read(2),
        read(4),
        if h.len() == 8 { read(6) } else { 255 },
    )
}

fn color_edit(ui: &mut egui::Ui, label: &str, hex: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        let c = color(hex);
        let mut rgba = c.to_srgba_unmultiplied();
        if ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed() {
            *hex = format!(
                "#{:02X}{:02X}{:02X}{:02X}",
                rgba[0], rgba[1], rgba[2], rgba[3]
            );
        }
        ui.label(RichText::new(hex.to_uppercase()).size(11.0).color(MUTED));
    });
}

fn document_pos(point: [f32; 2], origin: Pos2, zoom: f32) -> Pos2 {
    origin + vec2(point[0], point[1]) * zoom
}

fn selection_geometry(page: &Page, ids: &BTreeSet<u64>) -> Option<Widget> {
    let mut widgets = page.widgets.iter().filter(|w| ids.contains(&w.id));
    let first = widgets.next()?.clone();
    let rest: Vec<_> = widgets.collect();
    if rest.is_empty() {
        return Some(first);
    }
    let mut bounds = manipulation::rotated_bounds(&first);
    for w in rest {
        let other = manipulation::rotated_bounds(w);
        bounds = [
            bounds[0].min(other[0]),
            bounds[1].min(other[1]),
            bounds[2].max(other[2]),
            bounds[3].max(other[3]),
        ];
    }
    let mut frame = Widget::new(0, WidgetKind::Rectangle, bounds[0], bounds[1]);
    frame.width = (bounds[2] - bounds[0]).max(1.0);
    frame.height = (bounds[3] - bounds[1]).max(1.0);
    Some(frame)
}

fn draw_selection_frame(
    painter: &egui::Painter,
    w: &Widget,
    origin: Pos2,
    zoom: f32,
    handles: bool,
) {
    let points = manipulation::handle_positions(w);
    let screen = |i: usize| document_pos(points[i].1, origin, zoom);
    painter.add(egui::Shape::closed_line(
        vec![screen(0), screen(2), screen(4), screen(6)],
        Stroke::new(1.0, PURPLE),
    ));
    if !handles {
        return;
    }
    painter.line_segment([screen(1), screen(8)], Stroke::new(1.0, PURPLE));
    for (i, (handle, _)) in points.iter().enumerate() {
        let pos = screen(i);
        if *handle == Handle::Rotate {
            painter.circle(pos, 4.5, Color32::WHITE, Stroke::new(1.5, PURPLE));
        } else {
            let rect = Rect::from_center_size(pos, vec2(6.0, 6.0));
            painter.rect_filled(rect, 0.0, Color32::WHITE);
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0, PURPLE), StrokeKind::Inside);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_drag(
    page: &mut Page,
    before: &Page,
    ids: &BTreeSet<u64>,
    mode: &DragMode,
    delta: [f32; 2],
    shift: bool,
    alt: bool,
    snap: bool,
) {
    if let DragMode::Draw { id, kind, start } = mode {
        if let Some(widget) = page.widgets.iter_mut().find(|w| w.id == *id) {
            let mut delta = delta;
            if snap {
                delta = delta.map(|v| (v / 8.0).round() * 8.0);
            }
            if shift {
                if *kind == WidgetKind::Line {
                    let length = delta[0].hypot(delta[1]);
                    let angle = (delta[1].atan2(delta[0]) / std::f32::consts::FRAC_PI_4).round()
                        * std::f32::consts::FRAC_PI_4;
                    delta = [length * angle.cos(), length * angle.sin()];
                } else {
                    let side = delta[0].abs().max(delta[1].abs());
                    delta = [
                        side * if delta[0] < 0.0 { -1.0 } else { 1.0 },
                        side * if delta[1] < 0.0 { -1.0 } else { 1.0 },
                    ];
                }
            }
            let from = if alt {
                [start[0] - delta[0], start[1] - delta[1]]
            } else {
                *start
            };
            let to = [start[0] + delta[0], start[1] + delta[1]];
            widget.x = from[0].min(to[0]);
            widget.y = from[1].min(to[1]);
            widget.width = (to[0] - from[0]).abs().clamp(1.0, 20000.0);
            widget.height = (to[1] - from[1]).abs().clamp(1.0, 20000.0);
            if *kind == WidgetKind::Line {
                let x = if to[0] >= from[0] {
                    [0.0, 1.0]
                } else {
                    [1.0, 0.0]
                };
                let y = if (to[1] - from[1]).abs() < 0.001 {
                    [0.5, 0.5]
                } else if to[1] >= from[1] {
                    [0.0, 1.0]
                } else {
                    [1.0, 0.0]
                };
                widget.vector_points = Some(
                    serde_json::json!([[{"Path":{"X":x[0],"Y":y[0]},"HigherSegmentExists":true},{"Path":{"X":x[1],"Y":y[1]},"HigherSegmentExists":false}]]),
                );
            }
        }
        return;
    }
    let grid = if snap { 8.0 } else { 0.0 };
    let movement = rpshop::selection::constrained_delta(delta, shift);
    let movement = movement.map(|v| if snap { (v / 8.0).round() * 8.0 } else { v });
    for w in &mut page.widgets {
        if !ids.contains(&w.id) || w.locked {
            continue;
        }
        let Some(old) = before.widgets.iter().find(|old| old.id == w.id) else {
            continue;
        };
        match mode {
            DragMode::Move => {
                w.x = old.x + movement[0];
                w.y = old.y + movement[1];
            }
            DragMode::Resize { handle, bounds } => {
                let resized = manipulation::resize(bounds, *handle, delta, shift, alt, grid);
                if ids.len() == 1 {
                    *w = resized;
                } else {
                    let sx = resized.width / bounds.width;
                    let sy = resized.height / bounds.height;
                    w.x = resized.x + (old.x - bounds.x) * sx;
                    w.y = resized.y + (old.y - bounds.y) * sy;
                    w.width = (old.width * sx).max(1.0);
                    w.height = (old.height * sy).max(1.0);
                }
            }
            DragMode::Rotate {
                pivot,
                angle,
                start,
            } => {
                let mut degrees = (start[1] + delta[1] - pivot[1])
                    .atan2(start[0] + delta[0] - pivot[0])
                    .to_degrees()
                    - angle;
                if shift {
                    let base = if ids.len() == 1 { old.rotation } else { 0.0 };
                    degrees = ((base + degrees) / 15.0).round() * 15.0 - base;
                }
                *w = manipulation::rotate(old, degrees, *pivot);
            }
            DragMode::Marquee { .. } => {}
            DragMode::Draw { .. } => {}
        }
    }
}
