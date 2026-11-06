use super::*;

pub(super) const STORAGE_KEY: &str = "rpshop-sidebar-layout";

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) enum LeftPane {
    Pages,
    Library,
    Layers,
}

/// Workspace preferences are independent of document contents and undo history.
/// Defaults also migrate the old three-way splitter preference transparently.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct SidebarLayout {
    pub active: Option<LeftPane>,
    pub inspector_open: bool,
}

impl Default for SidebarLayout {
    fn default() -> Self {
        Self {
            active: Some(LeftPane::Pages),
            inspector_open: true,
        }
    }
}

#[derive(Clone, Copy)]
enum Icon {
    Pages,
    Library,
    Layers,
    Properties,
    Ai,
}

impl App {
    pub(super) fn activity_bars(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("left-activity-bar")
            .exact_width(48.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(rail_frame())
            .show(ctx, |ui| {
                if self.confirm.is_some() || self.editing.is_some() {
                    ui.disable();
                }
                for (pane, icon, label) in [
                    (LeftPane::Pages, Icon::Pages, "目录"),
                    (LeftPane::Library, Icon::Library, "组件"),
                    (LeftPane::Layers, Icon::Layers, "图层"),
                ] {
                    let active = self.sidebar_layout.active == Some(pane);
                    if activity_button(ui, icon, label, active, false).clicked() {
                        self.sidebar_layout.active = if active { None } else { Some(pane) };
                    }
                }
            });
        egui::SidePanel::right("right-activity-bar")
            .exact_width(48.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(rail_frame())
            .show(ctx, |ui| {
                if self.confirm.is_some() || self.editing.is_some() {
                    ui.disable();
                }
                let properties = !self.ai.visible && self.sidebar_layout.inspector_open;
                if activity_button(ui, Icon::Properties, "属性", properties, true).clicked() {
                    self.ai.visible = false;
                    self.sidebar_layout.inspector_open = !properties;
                }
                if activity_button(ui, Icon::Ai, "AI 设计", self.ai.visible, true).clicked() {
                    self.ai_toggle();
                }
            });
    }
}

fn rail_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_rgb(247, 248, 251))
        .stroke(Stroke::NONE)
        .inner_margin(egui::Margin::symmetric(4, 8))
}

fn activity_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    active: bool,
    right: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(40.0, 44.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            active,
            label,
        )
    });
    let painter = ui.painter();
    if active || response.hovered() {
        painter.rect_filled(
            rect.shrink2(vec2(2.0, 4.0)),
            6.0,
            if active {
                Color32::from_rgb(237, 232, 251)
            } else {
                Color32::from_rgb(237, 239, 245)
            },
        );
    }
    if active {
        let x = if right {
            rect.right() + 2.0
        } else {
            rect.left() - 4.0
        };
        painter.vline(
            x,
            rect.top() + 10.0..=rect.bottom() - 10.0,
            Stroke::new(2.5, PURPLE),
        );
    }
    let color = if !ui.is_enabled() {
        BORDER
    } else if active {
        PURPLE
    } else if response.hovered() {
        INK
    } else {
        MUTED
    };
    draw_icon(painter, rect.center(), icon, Stroke::new(1.6, color));
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if active {
            format!("{label} · 点击收起")
        } else {
            format!("{label} · 点击展开")
        })
}

pub(super) fn paint_disclosure(painter: &egui::Painter, center: Pos2, open: bool, color: Color32) {
    let offsets = if open {
        [vec2(-3.5, -2.0), vec2(3.5, -2.0), vec2(0.0, 3.0)]
    } else {
        [vec2(-2.0, -3.5), vec2(-2.0, 3.5), vec2(3.0, 0.0)]
    };
    let points = offsets.into_iter().map(|offset| center + offset).collect();
    painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

fn draw_icon(painter: &egui::Painter, center: Pos2, icon: Icon, stroke: Stroke) {
    let p = |x, y| center + vec2(x, y);
    let line = |a, b| {
        painter.line_segment([a, b], stroke);
    };
    let outline = |min, max| {
        painter.rect_stroke(
            Rect::from_min_max(min, max),
            1.5,
            stroke,
            StrokeKind::Middle,
        );
    };
    match icon {
        Icon::Pages => {
            painter.add(egui::Shape::line(
                vec![
                    p(-9.0, 7.0),
                    p(-9.0, -7.0),
                    p(-2.0, -7.0),
                    p(1.0, -4.0),
                    p(9.0, -4.0),
                    p(9.0, 7.0),
                    p(-9.0, 7.0),
                ],
                stroke,
            ));
            line(p(-5.0, 0.0), p(5.0, 0.0));
            line(p(-5.0, 3.5), p(2.0, 3.5));
        }
        Icon::Library => {
            for x in [-8.0, 2.0] {
                for y in [-8.0, 2.0] {
                    outline(p(x, y), p(x + 6.0, y + 6.0));
                }
            }
        }
        Icon::Layers => {
            painter.add(egui::Shape::line(
                vec![
                    p(-10.0, -3.0),
                    p(0.0, -8.0),
                    p(10.0, -3.0),
                    p(0.0, 2.0),
                    p(-10.0, -3.0),
                ],
                stroke,
            ));
            for y in [2.0, 7.0] {
                painter.add(egui::Shape::line(
                    vec![p(-10.0, y), p(0.0, y + 5.0), p(10.0, y)],
                    stroke,
                ));
            }
        }
        Icon::Properties => {
            for (y, x) in [(-7.0, -3.0), (0.0, 4.0), (7.0, -1.0)] {
                line(p(-9.0, y), p(x - 2.0, y));
                line(p(x + 2.0, y), p(9.0, y));
                outline(p(x - 2.0, y - 2.0), p(x + 2.0, y + 2.0));
            }
        }
        Icon::Ai => {
            painter.add(egui::Shape::line(
                vec![
                    p(-2.0, -9.0),
                    p(1.0, -2.0),
                    p(8.0, 1.0),
                    p(1.0, 4.0),
                    p(-2.0, 11.0),
                    p(-5.0, 4.0),
                    p(-12.0, 1.0),
                    p(-5.0, -2.0),
                    p(-2.0, -9.0),
                ],
                stroke,
            ));
            line(p(7.0, -10.0), p(7.0, -4.0));
            line(p(4.0, -7.0), p(10.0, -7.0));
        }
    }
}
