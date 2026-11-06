//! Canvas navigation in screen points. Document coordinates never change.
use eframe::egui::{self, Pos2, Rect, Vec2, vec2};

pub(super) const MIN_ZOOM: f32 = 0.1;
pub(super) const MAX_ZOOM: f32 = 10.0;
pub(super) const RULER_SIZE: f32 = 18.0;

pub(super) fn content_rect(viewport: Rect) -> Rect {
    Rect::from_min_max(
        (viewport.min + vec2(RULER_SIZE, RULER_SIZE)).min(viewport.max),
        viewport.max,
    )
}

fn ruler_rects(viewport: Rect) -> [Rect; 3] {
    let content = content_rect(viewport);
    [
        Rect::from_min_max(
            egui::pos2(content.left(), viewport.top()),
            egui::pos2(viewport.right(), content.top()),
        ),
        Rect::from_min_max(
            egui::pos2(viewport.left(), content.top()),
            egui::pos2(content.left(), viewport.bottom()),
        ),
        Rect::from_min_max(viewport.min, content.min),
    ]
}

fn nice_step(minimum: f64) -> f64 {
    let magnitude = 10.0_f64.powf(minimum.log10().floor());
    let fraction = minimum / magnitude;
    let step = if fraction <= 1.0 {
        1.0
    } else if fraction <= 2.0 {
        2.0
    } else if fraction <= 5.0 {
        5.0
    } else {
        10.0
    };
    step * magnitude
}

#[derive(Debug)]
struct RulerTick {
    value: f64,
    position: f32,
    major: bool,
}

fn ruler_ticks(min: f32, max: f32, origin: f32, zoom: f32) -> Vec<RulerTick> {
    if ![min, max, origin, zoom].into_iter().all(f32::is_finite) || max <= min || zoom <= 0.0 {
        return Vec::new();
    }
    let (min, max, origin, zoom) = (min as f64, max as f64, origin as f64, zoom as f64);
    let first = (min - origin) / zoom;
    let last = (max - origin) / zoom;
    // Leave room for signs and large coordinates, not just the usual 0..1000.
    let digits = (first.abs().max(last.abs()).ceil() as i64)
        .to_string()
        .len()
        + usize::from(first < 0.0);
    let major = nice_step((55.0_f64.max(digits as f64 * 6.0 + 12.0)) / zoom);
    let minor = nice_step(major / 5.0);
    let divisions = (major / minor).round() as i64;
    let start = (first / minor).ceil() as i64;
    let end = (last / minor).floor() as i64;
    (start..=end)
        .map(|index| RulerTick {
            value: index as f64 * minor,
            position: (origin + index as f64 * minor * zoom) as f32,
            major: index.rem_euclid(divisions) == 0,
        })
        .collect()
}

fn paint_ruler_axis(
    painter: &egui::Painter,
    strip: Rect,
    origin: f32,
    zoom: f32,
    horizontal: bool,
) {
    use egui::{Color32, FontId, Stroke, epaint::TextShape, pos2};
    if !strip.is_positive() {
        return;
    }
    let painter = painter.with_clip_rect(strip.intersect(painter.clip_rect()));
    let (min, max) = if horizontal {
        (strip.left(), strip.right())
    } else {
        (strip.top(), strip.bottom())
    };
    let ink = Color32::from_rgb(110, 110, 116);
    for tick in ruler_ticks(min, max, origin, zoom) {
        let length = if tick.major { 6.0 } else { 3.0 };
        let points = if horizontal {
            [
                pos2(tick.position, strip.bottom() - length),
                pos2(tick.position, strip.bottom()),
            ]
        } else {
            [
                pos2(strip.right() - length, tick.position),
                pos2(strip.right(), tick.position),
            ]
        };
        painter.line_segment(points, Stroke::new(1.0, Color32::from_rgb(176, 179, 187)));
        if !tick.major {
            continue;
        }
        let galley =
            painter.layout_no_wrap(format!("{:.0}", tick.value), FontId::proportional(9.0), ink);
        if tick.position + 3.0 + galley.size().x > max - 2.0 {
            continue;
        }
        let (position, angle) = if horizontal {
            (pos2(tick.position + 3.0, strip.top() + 1.0), 0.0)
        } else {
            (
                pos2(strip.left() + 1.0 + galley.size().y, tick.position + 3.0),
                std::f32::consts::FRAC_PI_2,
            )
        };
        painter.add(TextShape::new(position, galley, ink).with_angle(angle));
    }
}

/// Paint last, with the workspace painter rather than the content-clipped one.
/// Only ticks use document coordinates; the strips always use screen points.
pub(super) fn paint_rulers(painter: &egui::Painter, viewport: Rect, origin: Pos2, zoom: f32) {
    use egui::{Color32, Stroke};
    let [top, left, corner] = ruler_rects(viewport);
    for strip in [top, left, corner] {
        painter.rect_filled(strip, 0.0, Color32::from_rgb(250, 250, 250));
    }
    paint_ruler_axis(painter, top, origin.x, zoom, true);
    paint_ruler_axis(painter, left, origin.y, zoom, false);
    let border = Stroke::new(1.0, Color32::from_rgb(216, 216, 216));
    painter.line_segment([corner.left_bottom(), top.right_bottom()], border);
    painter.line_segment([corner.right_top(), left.right_bottom()], border);
}

#[derive(Clone, Copy)]
pub(super) enum ViewCommand {
    Step(f32),
    Zoom(f32),
    CenterPage,
    FitSelection,
    CenterSelection,
    Origin,
}
pub(super) struct PanDrag {
    pub pointer: Pos2,
    pub pan: Vec2,
    pub button: egui::PointerButton,
}
pub(super) struct CanvasView {
    pub pan: Vec2,
    pub pending: Option<ViewCommand>,
    pub panning: Option<PanDrag>,
}
impl Default for CanvasView {
    fn default() -> Self {
        Self {
            pan: vec2(40.0, 40.0),
            pending: None,
            panning: None,
        }
    }
}
impl CanvasView {
    pub fn zoom_at(&mut self, zoom: &mut f32, target: f32, anchor: Vec2) {
        if !target.is_finite() || !anchor.is_finite() {
            return;
        }
        let target = target.clamp(MIN_ZOOM, MAX_ZOOM);
        self.pan = anchor - (anchor - self.pan) * (target / *zoom);
        *zoom = target;
    }
    pub fn center(&mut self, zoom: f32, viewport: Rect, bounds: Rect) {
        self.pan =
            content_rect(viewport).center() - viewport.min - bounds.center().to_vec2() * zoom;
    }
    pub fn fit(&mut self, zoom: &mut f32, viewport: Rect, bounds: Rect) {
        let available = (content_rect(viewport).size() - vec2(80.0, 80.0)).max(vec2(1.0, 1.0));
        *zoom = (available.x / bounds.width().max(1.0))
            .min(available.y / bounds.height().max(1.0))
            .clamp(MIN_ZOOM, MAX_ZOOM);
        self.center(*zoom, viewport, bounds);
    }
    pub fn wheel(
        &mut self,
        zoom: &mut f32,
        unit: egui::MouseWheelUnit,
        delta: Vec2,
        modifiers: egui::Modifiers,
        anchor: Vec2,
        viewport: Rect,
    ) {
        if !delta.is_finite() {
            return;
        }
        if modifiers.ctrl || modifiers.command {
            let step = match unit {
                egui::MouseWheelUnit::Line => 0.05,
                egui::MouseWheelUnit::Point => 0.15 / 120.0,
                egui::MouseWheelUnit::Page => 0.15,
            };
            let delta = if delta.y != 0.0 { delta.y } else { delta.x };
            let target = *zoom + delta * step;
            let target = if (target - 1.0).abs() < 0.025 {
                1.0
            } else {
                target
            };
            self.zoom_at(zoom, target, anchor);
        } else {
            let scale = match unit {
                egui::MouseWheelUnit::Line => 40.0,
                egui::MouseWheelUnit::Point => 1.0,
                egui::MouseWheelUnit::Page => viewport.height(),
            };
            let delta = if modifiers.shift {
                vec2(delta.y, delta.x)
            } else {
                delta
            };
            self.pan += delta * scale;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reserved_strips_cover_the_viewport_without_changing_its_coordinate_origin() {
        let viewport = Rect::from_min_size(egui::pos2(37.0, 61.0), vec2(800.0, 600.0));
        let content = content_rect(viewport);
        let [top, left, corner] = ruler_rects(viewport);
        assert_eq!(content.min, viewport.min + vec2(RULER_SIZE, RULER_SIZE));
        assert_eq!(content.max, viewport.max);
        assert_eq!(top.height(), RULER_SIZE);
        assert_eq!(left.width(), RULER_SIZE);
        assert_eq!(corner, Rect::from_min_max(viewport.min, content.min));
        assert_eq!(top.left_bottom(), content.left_top());
        assert_eq!(left.right_top(), content.left_top());
        for size in [vec2(1.0, 1.0), vec2(12.0, 30.0), vec2(30.0, 12.0)] {
            let small = Rect::from_min_size(viewport.min, size);
            assert!(small.contains_rect(content_rect(small)));
            assert!(
                ruler_rects(small)
                    .into_iter()
                    .all(|strip| small.contains_rect(strip))
            );
        }
    }

    #[test]
    fn center_and_fit_use_reserved_content_but_keep_pan_relative_to_workspace() {
        let viewport = Rect::from_min_size(egui::pos2(37.0, 61.0), vec2(800.0, 600.0));
        let content = content_rect(viewport);
        let bounds = Rect::from_min_size(egui::pos2(-120.0, 80.0), vec2(640.0, 400.0));
        let mut view = CanvasView::default();
        let mut zoom = 0.5;
        view.fit(&mut zoom, viewport, bounds);
        let screen_center = viewport.min + view.pan + bounds.center().to_vec2() * zoom;
        assert!((screen_center - content.center()).length() < 0.0001);
        assert!((zoom - (content.width() - 80.0) / bounds.width()).abs() < 0.0001);
        let screen = Rect::from_min_size(
            viewport.min + view.pan + bounds.min.to_vec2() * zoom,
            bounds.size() * zoom,
        );
        assert!(content.shrink(39.99).contains_rect(screen));
    }

    #[test]
    fn adaptive_ticks_use_nice_intervals_and_preserve_negative_and_zero_coordinates() {
        let is_nice = |step: f64| {
            let normalized = step / 10.0_f64.powf(step.log10().floor());
            [1.0, 2.0, 5.0]
                .into_iter()
                .any(|nice| (normalized - nice).abs() < 0.0001)
        };
        for zoom in [0.1, 0.125, 0.2, 0.333, 0.5, 1.0, 2.0, 5.0, 10.0] {
            for origin in [287.0, -1273.0] {
                let ticks = ruler_ticks(55.0, 855.0, origin, zoom);
                assert!(ticks.len() > 10 && ticks.len() < 100);
                let minor = ticks[1].value - ticks[0].value;
                assert!(is_nice(minor));
                assert!((ticks[1].position - ticks[0].position) >= 10.99);
                let major: Vec<_> = ticks.iter().filter(|tick| tick.major).collect();
                assert!(major.len() >= 4);
                assert!(is_nice(major[1].value - major[0].value));
                assert!(major[1].position - major[0].position >= 54.99);
                for tick in &ticks {
                    assert!((tick.position - (origin + tick.value as f32 * zoom)).abs() < 0.002);
                    assert!(tick.position >= 54.999 && tick.position <= 855.001);
                }
                if origin > 55.0 {
                    assert!(ticks.iter().any(|tick| tick.value < 0.0));
                    let zero = ticks.iter().find(|tick| tick.value == 0.0).unwrap();
                    assert!(zero.major);
                    assert_eq!(zero.position, origin);
                }
            }
        }
    }

    #[test]
    fn painted_ruler_strips_are_screen_fixed_and_labels_point_to_document_ticks() {
        let ctx = egui::Context::default();
        let viewport = Rect::from_min_size(egui::pos2(37.0, 61.0), vec2(800.0, 600.0));
        let [top, left, corner] = ruler_rects(viewport);
        for zoom in [0.1, 0.65, 1.0, 3.0, 10.0] {
            for pan in [vec2(240.0, 210.0), vec2(-135.0, -80.0)] {
                let origin = viewport.min + pan;
                let output = ctx.run(Default::default(), |ctx| {
                    let painter = ctx
                        .layer_painter(egui::LayerId::background())
                        .with_clip_rect(viewport);
                    paint_rulers(&painter, viewport, origin, zoom);
                });
                let strips: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect) => Some(rect.rect),
                        _ => None,
                    })
                    .collect();
                assert_eq!(strips, vec![top, left, corner]);
                let mut labels = 0;
                for clipped in &output.shapes {
                    if let egui::Shape::Text(text) = &clipped.shape {
                        labels += 1;
                        let horizontal = clipped.clip_rect == top;
                        assert!(horizontal || clipped.clip_rect == left);
                        let value: f32 = text.galley.text().parse().unwrap();
                        let position = if horizontal {
                            text.pos.x - 3.0
                        } else {
                            text.pos.y - 3.0
                        };
                        let expected = if horizontal { origin.x } else { origin.y } + value * zoom;
                        assert!((position - expected).abs() < 0.002);
                        assert!(
                            (text.angle
                                - if horizontal {
                                    0.0
                                } else {
                                    std::f32::consts::FRAC_PI_2
                                })
                            .abs()
                                < 0.0001
                        );
                        assert!(
                            clipped
                                .clip_rect
                                .expand(0.5)
                                .contains_rect(text.visual_bounding_rect())
                        );
                        assert!(
                            output
                                .shapes
                                .iter()
                                .any(|candidate| match &candidate.shape {
                                    egui::Shape::LineSegment { points, .. }
                                        if candidate.clip_rect == clipped.clip_rect =>
                                    {
                                        let coordinate =
                                            if horizontal { points[1].x } else { points[1].y };
                                        (coordinate - position).abs() < 0.002
                                            && ((points[1] - points[0]).length() - 6.0).abs()
                                                < 0.001
                                    }
                                    _ => false,
                                })
                        );
                    }
                }
                assert!(labels >= 6);
            }
        }
    }

    #[test]
    fn zoom_keeps_the_pointer_document_point_and_clamps_limits() {
        let mut view = CanvasView::default();
        let mut zoom = 0.5;
        let pointer = vec2(300.0, 180.0);
        let point = (pointer - view.pan) / zoom;
        view.zoom_at(&mut zoom, 0.65, pointer);
        assert!(((pointer - view.pan) / zoom - point).length() < 0.0001);
        view.zoom_at(&mut zoom, 100.0, pointer);
        assert_eq!(zoom, MAX_ZOOM);
        view.zoom_at(&mut zoom, -10.0, pointer);
        assert_eq!(zoom, MIN_ZOOM);
    }
    #[test]
    fn wheel_modifiers_choose_navigation_without_touching_document_geometry() {
        let mut view = CanvasView::default();
        let mut zoom = 0.5;
        let viewport = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        let pointer = vec2(300.0, 200.0);
        view.wheel(
            &mut zoom,
            egui::MouseWheelUnit::Line,
            vec2(0.0, 3.0),
            egui::Modifiers::CTRL,
            pointer,
            viewport,
        );
        assert!((zoom - 0.65).abs() < 0.0001);
        let pan = view.pan;
        view.wheel(
            &mut zoom,
            egui::MouseWheelUnit::Point,
            vec2(0.0, 21.0),
            egui::Modifiers::SHIFT,
            pointer,
            viewport,
        );
        assert_eq!(view.pan, pan + vec2(21.0, 0.0));
        assert!((zoom - 0.65).abs() < 0.0001);
    }
}
