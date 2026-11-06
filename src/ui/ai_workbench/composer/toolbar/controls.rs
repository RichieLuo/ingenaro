//! Fixed-height toolbar controls. Text is optional presentation, not the hit target.
use super::*;

const ICON_WIDTH: f32 = 30.0;
const GAP: f32 = 7.0;
const CONTROL_COUNT: usize = 4;
const GAP_COUNT: f32 = (CONTROL_COUNT - 1) as f32;

#[derive(Clone, Copy)]
pub(super) enum Mark {
    Basic(Icon),
    Engine(Option<ai::EngineKind>),
    Permission(permissions::PermissionMode),
    Model(Option<ai::ReasoningEffort>),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ToolbarLayout {
    pub widths: [f32; CONTROL_COUNT],
    pub labels: [bool; 3],
    pub gap: f32,
}
impl ToolbarLayout {
    pub fn new(available: f32, text_widths: [f32; 3]) -> Self {
        let mut widths = [ICON_WIDTH; CONTROL_COUNT];
        for (control_width, text_width) in widths.iter_mut().zip(text_widths) {
            *control_width = (text_width + 54.0).clamp(72.0, 182.0);
        }
        let mut labels = [true; 3];
        let gap = GAP.min(((available - ICON_WIDTH * CONTROL_COUNT as f32) / GAP_COUNT).max(0.0));
        let budget = available - gap * GAP_COUNT;
        // Compact widths use icons, never a second row or smaller type.
        if available < 310.0 {
            widths = [ICON_WIDTH; CONTROL_COUNT];
            labels = [false; 3];
        } else {
            // Keep permission and model readable at the default sidebar width.
            // Collapse engine text first, then elide the model before hiding permission.
            if widths.iter().sum::<f32>() > budget {
                widths[0] = ICON_WIDTH;
                labels[0] = false;
            }
            let excess = (widths.iter().sum::<f32>() - budget).max(0.0);
            widths[2] -= excess.min((widths[2] - 96.0).max(0.0));
            if widths.iter().sum::<f32>() > budget {
                widths[1] = ICON_WIDTH;
                labels[1] = false;
            }
            let rest = widths.iter().sum::<f32>() - widths[2];
            if rest + widths[2] > budget {
                let model_width = budget - rest;
                if model_width >= 86.0 {
                    widths[2] = model_width;
                } else {
                    widths[2] = ICON_WIDTH;
                    labels[2] = false;
                }
            }
        }
        Self {
            widths,
            labels,
            gap,
        }
    }
    #[cfg(test)]
    pub fn used_width(self) -> f32 {
        self.widths.iter().sum::<f32>() + self.gap * GAP_COUNT
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn control(
    ui: &mut egui::Ui,
    key: &'static str,
    width: f32,
    mark: Mark,
    text: Option<&str>,
    tooltip: &str,
    color: Color32,
    enabled: bool,
    highlighted: bool,
) -> egui::Response {
    let (_, rect) = ui.allocate_space(vec2(width, CONTROL_HEIGHT));
    let response = ui.interact(
        rect,
        ui.make_persistent_id(("ai-toolbar", key)),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tooltip));
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    if highlighted || (enabled && response.hovered()) || response.has_focus() {
        painter.rect_filled(
            rect,
            6.0,
            if highlighted {
                Color32::from_rgb(255, 247, 237)
            } else {
                Color32::from_white_alpha(170)
            },
        );
    }
    if response.has_focus() {
        painter.rect_stroke(
            rect.shrink(0.5),
            6.0,
            Stroke::new(1.0, BLUE),
            StrokeKind::Inside,
        );
    }
    let color = if enabled {
        color
    } else {
        Color32::from_rgb(161, 168, 181)
    };
    let icon_rect = Rect::from_center_size(
        if text.is_some() {
            rect.left_center() + vec2(16.0, 0.0)
        } else {
            rect.center()
        },
        vec2(18.0, 18.0),
    );
    paint_mark(&painter, icon_rect, mark, color);
    if let Some(text) = text {
        // Leave a clear gap between text ink and the chevron, including rounding.
        let budget = (width - 50.0).max(1.0);
        let galley = if let Mark::Model(effort) = mark {
            model_galley(&painter, text, effort, budget, color)
        } else {
            let mut job = egui::text::LayoutJob::simple_singleline(
                text.into(),
                FontId::proportional(12.0),
                color,
            );
            job.wrap = egui::epaint::text::TextWrapping::truncate_at_width(budget);
            painter.layout_job(job)
        };
        painter.galley(
            rect.left_center() + vec2(30.0, -galley.size().y / 2.0),
            galley,
            color,
        );
        engine_icons::chevron(&painter, rect, color);
    }
    #[cfg(test)]
    ui.ctx().data_mut(|data| {
        let id = egui::Id::new(("ai-toolbar-test", key));
        data.insert_temp(id, rect);
    });
    let response = response.on_hover_text(tooltip);
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

pub(super) fn paint_mark(p: &egui::Painter, rect: Rect, mark: Mark, color: Color32) {
    let c = rect.center();
    let point = |x: f32, y: f32| c + vec2(x, y);
    let stroke = Stroke::new(1.35, color);
    match mark {
        Mark::Basic(icon) => paint_icon(p, rect, icon, color),
        Mark::Engine(Some(kind)) => engine_icons::paint(p, rect, kind),
        Mark::Engine(None) => {
            p.circle_stroke(c, 6.0, stroke);
            for (a, b) in [((-8.0, 0.0), (8.0, 0.0)), ((0.0, -8.0), (0.0, 8.0))] {
                p.line_segment([point(a.0, a.1), point(b.0, b.1)], stroke);
            }
        }
        Mark::Permission(mode) => modes::mode_icon(p, rect, mode, color),
        Mark::Model(_) => {
            p.rect_stroke(
                Rect::from_center_size(c, vec2(10.0, 10.0)),
                2.0,
                stroke,
                StrokeKind::Inside,
            );
            for offset in [-3.0, 3.0] {
                p.line_segment([point(offset, -8.0), point(offset, -5.0)], stroke);
                p.line_segment([point(offset, 5.0), point(offset, 8.0)], stroke);
                p.line_segment([point(-8.0, offset), point(-5.0, offset)], stroke);
                p.line_segment([point(5.0, offset), point(8.0, offset)], stroke);
            }
        }
    }
}

pub(super) fn model_galley(
    painter: &egui::Painter,
    model: &str,
    effort: Option<ai::ReasoningEffort>,
    budget: f32,
    color: Color32,
) -> std::sync::Arc<egui::Galley> {
    let suffix = format!("({})", effort_short_label(effort));
    let mut name = model.to_owned();
    let mut shortened = false;
    loop {
        let label = format!("{name}{}{suffix}", if shortened { "…" } else { "" });
        let galley = painter.layout_no_wrap(label, FontId::proportional(12.0), color);
        if galley.size().x.max(galley.mesh_bounds.right()) <= budget {
            return galley;
        }
        if name.pop().is_none() {
            // At the smallest readable width the suffix takes priority.
            return painter.layout_no_wrap(suffix, FontId::proportional(12.0), color);
        }
        shortened = true;
    }
}

pub(super) fn effort_suffix_width(ui: &egui::Ui) -> f32 {
    [
        "(自动)", "(最小)", "(低)", "(中)", "(高)", "(很高)", "(最高)",
    ]
    .into_iter()
    .map(|label| {
        ui.painter()
            .layout_no_wrap(label.into(), FontId::proportional(12.0), INK)
            .size()
            .x
    })
    .fold(0.0, f32::max)
}

#[cfg(test)]
pub(in crate::ui::ai_workbench) fn bounds(ctx: &egui::Context, key: &'static str) -> Rect {
    ctx.data(|data| data.get_temp(egui::Id::new(("ai-toolbar-test", key))))
        .expect("toolbar control rendered")
}
