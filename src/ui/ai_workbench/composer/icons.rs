use super::*;

#[derive(Clone, Copy)]
pub(in crate::ui::ai_workbench) enum Icon {
    Page,
    Close,
    Undo,
    Send,
    Stop,
    Plus,
    Canvas,
    More,
}

pub(in crate::ui::ai_workbench) fn icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    enabled: bool,
    color: Color32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        vec2(23.0, 25.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    if response.hovered() && enabled {
        ui.painter()
            .rect_filled(rect.shrink(1.0), 4.0, Color32::from_black_alpha(9));
    }
    paint_icon(
        ui.painter(),
        rect.shrink(4.0),
        icon,
        if enabled {
            color
        } else {
            Color32::from_gray(187)
        },
    );
    let response = response.on_hover_text(label);
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

pub(in crate::ui::ai_workbench) fn paint_icon(
    painter: &egui::Painter,
    rect: Rect,
    icon: Icon,
    color: Color32,
) {
    let center = rect.center();
    let point = |x: f32, y: f32| center + vec2(x, y) * 0.83;
    let stroke = Stroke::new(1.25, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        painter.line_segment([point(a.0, a.1), point(b.0, b.1)], stroke)
    };
    match icon {
        Icon::More => {
            for x in [-5.0, 0.0, 5.0] {
                painter.circle_filled(point(x, 0.0), 1.2, color);
            }
        }
        Icon::Page => {
            painter.rect_stroke(
                Rect::from_min_max(point(-5.0, -7.0), point(5.0, 7.0)),
                1.0,
                stroke,
                StrokeKind::Inside,
            );
            line((-2.5, -2.0), (2.5, -2.0));
            line((-2.5, 1.0), (2.5, 1.0));
            line((-2.5, 4.0), (1.5, 4.0));
        }
        Icon::Close => {
            line((-4.0, -4.0), (4.0, 4.0));
            line((-4.0, 4.0), (4.0, -4.0));
        }
        Icon::Undo => {
            line((-6.0, -1.0), (-6.0, -5.0));
            line((-6.0, -1.0), (-2.0, -1.0));
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [
                    point(-6.0, -1.0),
                    point(5.0, -11.0),
                    point(12.0, 5.0),
                    point(-1.0, 5.0),
                ],
                false,
                Color32::TRANSPARENT,
                stroke,
            ));
        }
        Icon::Send => {
            painter.add(egui::Shape::closed_line(
                vec![
                    point(-6.0, -7.0),
                    point(8.0, 0.0),
                    point(-6.0, 7.0),
                    point(-3.0, 0.0),
                ],
                stroke,
            ));
            line((-3.0, 0.0), (5.0, 0.0));
        }
        Icon::Stop => {
            painter.rect_filled(
                Rect::from_min_max(point(-5.0, -5.0), point(5.0, 5.0)),
                2.0,
                color,
            );
        }
        Icon::Plus => {
            line((-5.0, 0.0), (5.0, 0.0));
            line((0.0, -5.0), (0.0, 5.0));
        }
        Icon::Canvas => {
            painter.rect_stroke(
                Rect::from_min_max(point(-7.0, -5.0), point(7.0, 5.0)),
                1.0,
                stroke,
                StrokeKind::Inside,
            );
            line((-3.0, -5.0), (-3.0, 5.0));
            line((2.0, 1.0), (5.0, -2.0));
        }
    }
}
