//! Permission modes only; model-dependent effort lives with the model picker.
use super::*;

pub(super) const MUTED: Color32 = Color32::from_rgb(123, 128, 137);
pub(super) const WIDTH: f32 = 292.0;

impl App {
    pub(super) fn ai_modes_popup(&mut self, anchor: &egui::Response) {
        // The editor handles Escape before drawing its children; retain popup dismissal
        // even when that global handler has already consumed the filtered key event.
        let popup_id = egui::Popup::default_response_id(anchor);
        if egui::Popup::is_id_open(&anchor.ctx, popup_id)
            && anchor.ctx.input(|input| {
                input.raw.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::Escape,
                            pressed: true,
                            ..
                        }
                    )
                })
            })
        {
            egui::Popup::close_id(&anchor.ctx, popup_id);
            return;
        }
        egui::Popup::menu(anchor)
            .align(egui::RectAlign::TOP_START)
            .gap(6.0)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1.0, Color32::from_gray(217)))
                    .corner_radius(8)
                    .inner_margin(4)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 3],
                        blur: 12,
                        spread: 0,
                        color: Color32::from_black_alpha(14),
                    }),
            )
            .show(|ui| {
                ui.set_width(WIDTH);
                ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                let (header, _) = ui.allocate_exact_size(vec2(WIDTH, 28.0), Sense::hover());
                ui.painter().text(
                    header.left_center() + vec2(8.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    "权限模式",
                    FontId::proportional(11.0),
                    MUTED,
                );
                let mode = self.ai.authorization.mode();
                for (value, description) in [
                    (
                        permissions::PermissionMode::Ask,
                        "每次修改设计前，先征求你的确认",
                    ),
                    (
                        permissions::PermissionMode::ReadOnly,
                        "探索设计与讨论方案，不修改画布",
                    ),
                    (
                        permissions::PermissionMode::Full,
                        "当前工程与会话内连续设计，不再逐次询问",
                    ),
                ] {
                    if mode_row(ui, value, description, mode == value).clicked() {
                        // Presentation must not bypass the existing full-access confirmation.
                        if mode != value {
                            self.ai_change_permission(value);
                        }
                        ui.close();
                    }
                }
            });
    }
}

fn mode_row(
    ui: &mut egui::Ui,
    value: permissions::PermissionMode,
    description: &str,
    selected: bool,
) -> egui::Response {
    let title = value.label();
    let (rect, response) = ui.allocate_exact_size(vec2(WIDTH, 54.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, selected, title)
    });
    let p = ui.painter();
    if selected || response.hovered() || response.has_focus() {
        p.rect_filled(
            rect,
            4.0,
            if selected {
                PURPLE
            } else {
                Color32::from_rgb(243, 245, 248)
            },
        );
    }
    let foreground = if selected { Color32::WHITE } else { INK };
    let secondary = if selected { Color32::WHITE } else { MUTED };
    mode_icon(
        p,
        Rect::from_center_size(rect.left_center() + vec2(18.0, 0.0), vec2(18.0, 18.0)),
        value,
        secondary,
    );
    p.text(
        rect.min + vec2(38.0, 7.0),
        egui::Align2::LEFT_TOP,
        title,
        FontId::proportional(12.0),
        foreground,
    );
    let description = p.layout(
        description.into(),
        FontId::proportional(10.5),
        secondary,
        WIDTH - 70.0,
    );
    p.galley(rect.min + vec2(38.0, 26.0), description, secondary);
    if selected {
        let c = rect.right_center() - vec2(16.0, 0.0);
        p.add(egui::Shape::line(
            vec![
                c + vec2(-4.0, 0.0),
                c + vec2(-1.0, 3.0),
                c + vec2(5.0, -4.0),
            ],
            Stroke::new(1.5, Color32::WHITE),
        ));
    }
    if response.has_focus() {
        p.rect_stroke(
            rect.shrink(1.0),
            4.0,
            Stroke::new(1.0, PURPLE),
            StrokeKind::Inside,
        );
    }
    #[cfg(test)]
    ui.ctx().data_mut(|data| {
        data.insert_temp(egui::Id::new(("ai-mode-row-test", title)), rect);
    });
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(super) fn mode_icon(
    p: &egui::Painter,
    rect: Rect,
    value: permissions::PermissionMode,
    color: Color32,
) {
    let c = rect.center();
    let point = |x: f32, y: f32| c + vec2(x, y);
    let stroke = Stroke::new(1.1, color);
    match value {
        permissions::PermissionMode::Full => {
            p.add(egui::Shape::closed_line(
                vec![
                    point(1.0, -8.0),
                    point(-5.0, 1.0),
                    point(0.0, 1.0),
                    point(-1.0, 8.0),
                    point(5.0, -2.0),
                    point(1.0, -2.0),
                ],
                stroke,
            ));
        }
        permissions::PermissionMode::ReadOnly => {
            p.rect_stroke(
                Rect::from_center_size(c, vec2(13.0, 13.0)),
                1.5,
                stroke,
                StrokeKind::Inside,
            );
            for (a, b) in [
                ((-6.0, -3.0), (6.0, -3.0)),
                ((-3.0, 0.0), (3.0, 0.0)),
                ((-3.0, 3.0), (1.0, 3.0)),
            ] {
                p.line_segment([point(a.0, a.1), point(b.0, b.1)], stroke);
            }
        }
        permissions::PermissionMode::Ask => {
            // Raised hand: an outline, not a font glyph, at all display scales.
            p.add(egui::Shape::line(
                vec![
                    point(-3.0, 1.0),
                    point(-3.0, -5.0),
                    point(-1.0, -6.0),
                    point(-1.0, 0.0),
                    point(-1.0, -7.0),
                    point(1.0, -7.0),
                    point(1.0, 0.0),
                    point(1.0, -6.0),
                    point(3.0, -5.0),
                    point(3.0, 1.0),
                    point(3.0, -3.0),
                    point(5.0, -3.0),
                    point(5.0, 4.0),
                    point(2.0, 7.0),
                    point(-2.0, 7.0),
                    point(-6.0, 2.0),
                    point(-6.0, -1.0),
                    point(-4.0, -1.0),
                    point(-3.0, 2.0),
                ],
                stroke,
            ));
        }
    }
}
