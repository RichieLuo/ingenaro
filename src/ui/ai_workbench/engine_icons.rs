//! Crisp local engine marks drawn as vectors, so menus do not rely on missing font glyphs.
use super::*;

pub(super) fn paint(p: &egui::Painter, rect: Rect, kind: ai::EngineKind) {
    let c = rect.center();
    let scale = rect.width().min(rect.height()) / 24.0;
    let at = |x: f32, y: f32| c + vec2(x, y) * scale;
    match kind {
        ai::EngineKind::Claude => {
            let color = Color32::from_rgb(202, 117, 85);
            for index in 0..12 {
                let angle = index as f32 * std::f32::consts::TAU / 12.0;
                let (sin, cos) = angle.sin_cos();
                let length = if index % 3 == 0 { 10.0 } else { 8.5 };
                p.line_segment(
                    [at(cos * 3.5, sin * 3.5), at(cos * length, sin * length)],
                    Stroke::new(1.8 * scale, color),
                );
            }
            p.circle_filled(c, 2.5 * scale, color);
        }
        ai::EngineKind::Codex => {
            let color = Color32::from_rgb(40, 49, 62);
            let stroke = Stroke::new(1.7 * scale, color);
            p.rect_stroke(
                Rect::from_center_size(c, vec2(21.0, 18.0) * scale),
                4.0 * scale,
                stroke,
                StrokeKind::Inside,
            );
            p.line_segment([at(-6.0, -4.0), at(-2.0, 0.0)], stroke);
            p.line_segment([at(-2.0, 0.0), at(-6.0, 4.0)], stroke);
            p.line_segment([at(1.0, 4.0), at(6.0, 4.0)], stroke);
        }
        ai::EngineKind::Antigravity => {
            let stroke = Stroke::new(2.2 * scale, Color32::from_rgb(66, 111, 225));
            p.line_segment([at(-9.0, 7.0), at(0.0, -8.0)], stroke);
            p.line_segment([at(0.0, -8.0), at(9.0, 7.0)], stroke);
            p.line_segment([at(-5.5, 1.0), at(5.5, 1.0)], stroke);
        }
        _ => {
            p.circle_stroke(c, 8.0 * scale, Stroke::new(scale, MUTED));
        }
    }
}
pub(super) fn chevron(p: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.right_center() - vec2(10.0, 0.0);
    p.line_segment(
        [c + vec2(-3.0, -1.5), c + vec2(0.0, 1.5)],
        Stroke::new(1.2, color),
    );
    p.line_segment(
        [c + vec2(0.0, 1.5), c + vec2(3.0, -1.5)],
        Stroke::new(1.2, color),
    );
}

pub(super) fn inline(ui: &mut egui::Ui, kind: ai::EngineKind, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    paint(ui.painter(), rect, kind);
}
pub(super) fn choice(
    ui: &mut egui::Ui,
    kind: ai::EngineKind,
    name: &str,
    selected: bool,
) -> egui::Response {
    let desired = vec2(ui.available_width().max(170.0), 42.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            name,
        )
    });
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            7.0,
            if selected {
                Color32::from_rgb(241, 237, 255)
            } else {
                Color32::from_rgb(247, 248, 251)
            },
        );
    }
    paint(
        ui.painter(),
        Rect::from_center_size(rect.left_center() + vec2(21.0, 0.0), vec2(21.0, 21.0)),
        kind,
    );
    ui.painter().text(
        rect.left_center() + vec2(40.0, 0.0),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(13.0),
        INK,
    );
    if selected {
        let c = rect.right_center() - vec2(17.0, 0.0);
        ui.painter().line_segment(
            [c + vec2(-4.0, 0.0), c + vec2(-1.0, 3.0)],
            Stroke::new(1.6, PURPLE),
        );
        ui.painter().line_segment(
            [c + vec2(-1.0, 3.0), c + vec2(5.0, -4.0)],
            Stroke::new(1.6, PURPLE),
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
