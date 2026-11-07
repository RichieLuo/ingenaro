//! Widget painting with one clockwise rotation about the widget's center.
//! The zero-angle path uses the original native egui primitives.

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, vec2};
use ingenaro::model::{Widget, WidgetKind};

const PURPLE: Color32 = Color32::from_rgb(117, 83, 222);

pub(super) fn draw_widget(
    painter: &egui::Painter,
    widget: &Widget,
    origin: Pos2,
    zoom: f32,
    texture: Option<&egui::TextureHandle>,
) {
    let rect = Rect::from_min_size(
        origin + vec2(widget.x, widget.y) * zoom,
        vec2(widget.width, widget.height) * zoom,
    );
    let angle = widget.rotation.rem_euclid(360.0).to_radians();
    let rotation = Rotation {
        center: rect.center(),
        angle,
    };
    let fade = |hex: &str| color(hex).gamma_multiply(widget.opacity);
    let fill = fade(&widget.fill);
    let stroke = Stroke::new(zoom.max(0.7), fade(&widget.stroke));
    match widget.kind {
        WidgetKind::Rectangle if widget.vector_points.is_some() => {
            for (points, closed) in ingenaro::shape_geometry::contours(widget) {
                let positions: Vec<_> = points
                    .iter()
                    .map(|point| {
                        rotation.point(
                            rect.min + vec2(point[0] * rect.width(), point[1] * rect.height()),
                        )
                    })
                    .collect();
                if positions.len() > 2 && fill.a() != 0 {
                    let mut mesh = egui::Mesh::default();
                    for pos in &positions {
                        mesh.colored_vertex(*pos, fill);
                    }
                    for triangle in ingenaro::shape_geometry::triangles(&points) {
                        mesh.add_triangle(
                            triangle[0] as u32,
                            triangle[1] as u32,
                            triangle[2] as u32,
                        );
                    }
                    painter.add(egui::Shape::mesh(mesh));
                }
                painter.add(egui::epaint::PathShape {
                    points: positions,
                    closed,
                    fill: Color32::TRANSPARENT,
                    stroke: stroke.into(),
                });
            }
        }
        WidgetKind::Ellipse => {
            if angle == 0.0 {
                painter.add(egui::epaint::EllipseShape::filled(
                    rect.center(),
                    rect.size() / 2.0,
                    fill,
                ));
                painter.add(egui::epaint::EllipseShape::stroke(
                    rect.center(),
                    rect.size() / 2.0,
                    stroke,
                ));
            } else {
                let radii = rect.size() / 2.0;
                let segments = (radii.max_elem().sqrt() * 8.0).clamp(32.0, 256.0) as usize;
                let points = (0..segments)
                    .map(|index| {
                        let theta = std::f32::consts::TAU * index as f32 / segments as f32;
                        rotation.point(
                            rect.center() + vec2(theta.cos() * radii.x, theta.sin() * radii.y),
                        )
                    })
                    .collect();
                painter.add(egui::Shape::convex_polygon(points, fill, stroke));
            }
        }
        WidgetKind::Line => {
            let points = ingenaro::vector_edit::line_endpoints(widget)
                .map(|point| rotation.point(origin + vec2(point[0], point[1]) * zoom));
            painter.line_segment(
                points,
                Stroke::new((widget.height.clamp(1.0, 4.0) * zoom).max(0.7), fill),
            );
        }
        WidgetKind::Image => {
            if let Some(texture) = texture {
                let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                let tint = Color32::WHITE.gamma_multiply(widget.opacity);
                if angle == 0.0 {
                    painter.image(texture.id(), rect, uv, tint);
                } else {
                    let mut mesh = egui::Mesh::with_texture(texture.id());
                    mesh.add_rect_with_uv(rect, uv, tint);
                    for vertex in &mut mesh.vertices {
                        vertex.pos = rotation.point(vertex.pos);
                    }
                    painter.add(egui::Shape::mesh(mesh));
                }
            } else {
                rounded_rect(
                    painter,
                    rect,
                    4.0,
                    Color32::from_rgb(239, 237, 250),
                    Stroke::NONE,
                    rotation,
                );
                let galley = painter.layout_no_wrap(
                    "▧ 图片".into(),
                    FontId::proportional(18.0 * zoom),
                    PURPLE,
                );
                let min = Align2::CENTER_CENTER
                    .anchor_size(rect.center(), galley.size())
                    .min;
                paint_text(painter, min, galley, PURPLE, rotation, None);
            }
        }
        WidgetKind::Text => {}
        WidgetKind::Checkbox | WidgetKind::Radio => {
            let checkbox = Rect::from_min_size(
                rect.left_center() - vec2(0.0, 8.0 * zoom),
                vec2(16.0, 16.0) * zoom,
            );
            rounded_rect(
                painter,
                checkbox,
                if widget.kind == WidgetKind::Radio {
                    8.0 * zoom
                } else {
                    2.0
                },
                fill,
                Stroke::new(zoom.max(0.7), fade(&widget.text_color)),
                rotation,
            );
        }
        WidgetKind::Droplist | WidgetKind::ListBox | WidgetKind::TextArea => {
            rounded_rect(
                painter,
                rect,
                (widget.radius * zoom).min(255.0),
                fill,
                stroke,
                rotation,
            );
            if widget.kind == WidgetKind::Droplist {
                let center = rect.right_center() - vec2(14.0 * zoom, 0.0);
                painter.line_segment(
                    [
                        rotation.point(center - vec2(4.0 * zoom, 2.0 * zoom)),
                        rotation.point(center + vec2(0.0, 2.0 * zoom)),
                    ],
                    Stroke::new(zoom.max(0.7), fade(&widget.text_color)),
                );
                painter.line_segment(
                    [
                        rotation.point(center + vec2(0.0, 2.0 * zoom)),
                        rotation.point(center + vec2(4.0 * zoom, -2.0 * zoom)),
                    ],
                    Stroke::new(zoom.max(0.7), fade(&widget.text_color)),
                );
            }
        }
        _ => rounded_rect(
            painter,
            rect,
            (widget.radius * zoom).clamp(0.0, 255.0) as u8 as f32,
            fill,
            stroke,
            rotation,
        ),
    }
    if !matches!(widget.kind, WidgetKind::Line | WidgetKind::Image) && !widget.text.is_empty() {
        if matches!(
            widget.kind,
            WidgetKind::Droplist | WidgetKind::ListBox | WidgetKind::TextArea
        ) {
            let text = if widget.kind == WidgetKind::Droplist {
                widget.text.lines().next().unwrap_or("").to_owned()
            } else {
                widget.text.clone()
            };
            let inset = 8.0 * zoom;
            let width = (rect.width()
                - inset * 2.0
                - if widget.kind == WidgetKind::Droplist {
                    20.0 * zoom
                } else {
                    0.0
                })
            .max(1.0);
            let galley = painter.layout(
                text,
                FontId::proportional((widget.font_size * zoom).max(1.0)),
                fade(&widget.text_color),
                width,
            );
            let y = if widget.kind == WidgetKind::Droplist {
                rect.center().y - galley.size().y / 2.0
            } else {
                rect.top() + inset
            };
            paint_text(
                painter,
                Pos2::new(rect.left() + inset, y),
                galley,
                fade(&widget.text_color),
                rotation,
                Some(rect),
            );
            return;
        }
        if let Some(layout) = &widget.text_layout {
            let padding = layout.padding.map(|value| value * zoom);
            let inner = Rect::from_min_max(
                rect.min + vec2(padding[0], padding[1]),
                (rect.max - vec2(padding[2], padding[3]))
                    .max(rect.min + vec2(padding[0] + 1.0, padding[1] + 1.0)),
            );
            let bold_family = egui::FontFamily::Name("canvas-bold".into());
            let family = if layout.bold
                && painter
                    .ctx()
                    .fonts(|fonts| fonts.families().contains(&bold_family))
            {
                bold_family
            } else {
                egui::FontFamily::Proportional
            };
            let font = FontId::new((widget.font_size * zoom).max(1.0), family);
            let text_color = fade(&widget.text_color);
            let galley = if layout.wrap {
                painter.layout(
                    widget.text.clone(),
                    font,
                    text_color,
                    inner.width().max(1.0),
                )
            } else {
                painter.layout_no_wrap(widget.text.clone(), font, text_color)
            };
            let offset = |alignment, remaining: f32| match alignment {
                1 => remaining * 0.5,
                2 => remaining,
                _ => 0.0,
            };
            let min = inner.min
                + vec2(
                    offset(layout.horizontal, inner.width() - galley.size().x),
                    offset(layout.vertical, inner.height() - galley.size().y),
                );
            // Auto-width source text must not be forced into a second row by
            // font fallback metrics, then cropped to the original short box.
            let clip = if layout.wrap {
                rect
            } else {
                rect.union(Rect::from_min_size(min, galley.size()))
            };
            paint_text(painter, min, galley, text_color, rotation, Some(clip));
            return;
        }
        let (anchor, position, width) = if matches!(
            widget.kind,
            WidgetKind::Text | WidgetKind::Input | WidgetKind::Checkbox | WidgetKind::Radio
        ) {
            (
                Align2::LEFT_CENTER,
                Pos2::new(
                    rect.left()
                        + if matches!(widget.kind, WidgetKind::Checkbox | WidgetKind::Radio) {
                            24.0 * zoom
                        } else {
                            8.0 * zoom
                        },
                    rect.center().y,
                ),
                rect.width() - 16.0 * zoom,
            )
        } else {
            (
                Align2::CENTER_CENTER,
                rect.center(),
                rect.width() - 12.0 * zoom,
            )
        };
        let text_color = fade(&widget.text_color);
        let galley = painter.layout(
            widget.text.clone(),
            FontId::proportional((widget.font_size * zoom).max(1.0)),
            text_color,
            width.max(1.0),
        );
        let min = anchor.anchor_size(position, galley.size()).min;
        paint_text(painter, min, galley, text_color, rotation, Some(rect));
    }
}

#[derive(Clone, Copy)]
struct Rotation {
    center: Pos2,
    angle: f32,
}

impl Rotation {
    fn point(self, point: Pos2) -> Pos2 {
        if self.angle == 0.0 {
            return point;
        }
        let (sin, cos) = self.angle.sin_cos();
        let delta = point - self.center;
        self.center + vec2(delta.x * cos - delta.y * sin, delta.x * sin + delta.y * cos)
    }

    fn bounds(self, rect: Rect) -> Rect {
        Rect::from_points(&[
            self.point(rect.left_top()),
            self.point(rect.right_top()),
            self.point(rect.right_bottom()),
            self.point(rect.left_bottom()),
        ])
    }
}

fn rounded_rect(
    painter: &egui::Painter,
    rect: Rect,
    radius: f32,
    fill: Color32,
    stroke: Stroke,
    rotation: Rotation,
) {
    if rotation.angle == 0.0 {
        painter.rect(rect, radius as u8, fill, stroke, StrokeKind::Inside);
        return;
    }
    painter.add(egui::Shape::convex_polygon(
        rounded_points(rect, radius, rotation),
        fill,
        Stroke::NONE,
    ));
    if stroke.width > 0.0 && stroke.color != Color32::TRANSPARENT {
        let inset = stroke.width.min(rect.width().min(rect.height())) / 2.0;
        painter.add(egui::Shape::closed_line(
            rounded_points(rect.shrink(inset), (radius - inset).max(0.0), rotation),
            stroke,
        ));
    }
}

fn rounded_points(rect: Rect, radius: f32, rotation: Rotation) -> Vec<Pos2> {
    let radius = radius.min(rect.width().min(rect.height()) / 2.0).max(0.0);
    if radius == 0.0 {
        return vec![
            rotation.point(rect.left_top()),
            rotation.point(rect.right_top()),
            rotation.point(rect.right_bottom()),
            rotation.point(rect.left_bottom()),
        ];
    }
    let corners = [
        (
            Pos2::new(rect.right() - radius, rect.top() + radius),
            -std::f32::consts::FRAC_PI_2,
        ),
        (
            Pos2::new(rect.right() - radius, rect.bottom() - radius),
            0.0,
        ),
        (
            Pos2::new(rect.left() + radius, rect.bottom() - radius),
            std::f32::consts::FRAC_PI_2,
        ),
        (
            Pos2::new(rect.left() + radius, rect.top() + radius),
            std::f32::consts::PI,
        ),
    ];
    let steps = (radius.sqrt() * 2.0).clamp(4.0, 24.0) as usize;
    corners
        .into_iter()
        .flat_map(|(center, start)| {
            (0..=steps).map(move |index| {
                let theta = start + std::f32::consts::FRAC_PI_2 * index as f32 / steps as f32;
                rotation.point(center + vec2(theta.cos(), theta.sin()) * radius)
            })
        })
        .collect()
}

fn paint_text(
    painter: &egui::Painter,
    min: Pos2,
    galley: std::sync::Arc<egui::Galley>,
    color: Color32,
    rotation: Rotation,
    clip: Option<Rect>,
) {
    let painter = if let Some(clip) = clip {
        painter.with_clip_rect(rotation.bounds(clip).intersect(painter.clip_rect()))
    } else {
        painter.clone()
    };
    if rotation.angle == 0.0 {
        painter.galley(min, galley, color);
    } else {
        let mut text = egui::epaint::TextShape::new(rotation.point(min), galley, color);
        text.angle = rotation.angle;
        painter.add(text);
    }
}

fn color(hex: &str) -> Color32 {
    let value = hex.trim_start_matches('#');
    if !matches!(value.len(), 6 | 8) || !value.is_ascii() {
        return Color32::GRAY;
    }
    let read = |offset| u8::from_str_radix(&value[offset..offset + 2], 16).unwrap_or(0);
    Color32::from_rgba_unmultiplied(
        read(0),
        read(2),
        read(4),
        if value.len() == 8 { read(6) } else { 255 },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_auto_width_text_uses_source_insets_and_keeps_full_first_row() {
        let context = egui::Context::default();
        let mut widget = Widget::new(1, WidgetKind::Rectangle, 19.0, 20.0);
        widget.width = 80.0;
        widget.height = 22.0;
        widget.font_size = 18.0;
        widget.text = "one complete source title on one row".into();
        widget.text_layout = Some(ingenaro::model::TextLayout {
            horizontal: 0,
            vertical: 0,
            padding: [0.0; 4],
            wrap: false,
            bold: true,
        });
        let output = context.run(Default::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            draw_widget(&painter, &widget, Pos2::new(100.0, 100.0), 1.0, None);
        });
        let (text, clip) = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some((text, shape.clip_rect)),
                _ => None,
            })
            .unwrap();
        assert_eq!(text.pos, Pos2::new(119.0, 120.0));
        assert_eq!(text.galley.rows.len(), 1);
        assert!(clip.contains(text.pos));
        assert!(clip.contains(text.pos + text.galley.size()));
    }

    #[test]
    fn painter_records_rotated_text_and_image_with_unchanged_texture_coordinates() {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "rotation-test",
            egui::ColorImage::filled([2, 2], Color32::WHITE),
            egui::TextureOptions::LINEAR,
        );
        let mut image = Widget::new(2, WidgetKind::Image, 10.0, 20.0);
        image.width = 100.0;
        image.height = 40.0;
        image.rotation = 90.0;
        let mut text = Widget::new(3, WidgetKind::Text, 10.0, 20.0);
        text.rotation = 90.0;
        let output = context.run(egui::RawInput::default(), |context| {
            let painter = context.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("rotation-test"),
            ));
            draw_widget(&painter, &image, Pos2::ZERO, 1.0, Some(&texture));
            draw_widget(&painter, &text, Pos2::ZERO, 1.0, None);
        });
        let mesh = output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Mesh(mesh) => Some(mesh),
                _ => None,
            })
            .unwrap();
        assert_eq!(mesh.texture_id, texture.id());
        assert_eq!(mesh.vertices.len(), 4);
        assert!((mesh.vertices[0].pos - Pos2::new(80.0, -10.0)).length() < 0.001);
        assert_eq!(mesh.vertices[0].uv, Pos2::ZERO);
        assert!(
            mesh.vertices
                .iter()
                .any(|vertex| vertex.uv == Pos2::new(1.0, 1.0))
        );
        assert!(output.shapes.iter().any(|clipped| matches!(&clipped.shape, egui::Shape::Text(shape) if (shape.angle - std::f32::consts::FRAC_PI_2).abs() < 0.001)));
    }

    #[test]
    fn rotation_keeps_center_and_rotates_line_endpoints() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), vec2(100.0, 40.0));
        let rotation = Rotation {
            center: rect.center(),
            angle: std::f32::consts::FRAC_PI_2,
        };
        assert_eq!(rotation.point(rect.center()), rect.center());
        assert!((rotation.point(rect.left_top()) - Pos2::new(80.0, -10.0)).length() < 0.001);
        assert!((rotation.point(rect.right_bottom()) - Pos2::new(40.0, 90.0)).length() < 0.001);
        let bounds = rotation.bounds(rect);
        assert!((bounds.width() - 40.0).abs() < 0.001);
        assert!((bounds.height() - 100.0).abs() < 0.001);
    }
}
