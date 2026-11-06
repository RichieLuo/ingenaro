//! RPShop dialog system: one surface, custom headers, readable sections and clear actions.
use eframe::egui::{self, Color32, FontId, RichText, Stroke, StrokeKind, vec2};

pub(super) const ACCENT: Color32 = Color32::from_rgb(105, 79, 218);
pub(super) const MUTED: Color32 = Color32::from_rgb(112, 120, 138);
const INK: Color32 = Color32::from_rgb(37, 42, 57);

#[derive(Clone, Copy)]
pub(super) enum Symbol {
    Shield,
    Save,
    Text,
    Info,
    Settings,
    History,
    Link,
    Preview,
}

pub(super) fn frame(ctx: &egui::Context) -> egui::Frame {
    let dark = ctx.style().visuals.dark_mode;
    egui::Frame::new()
        .fill(if dark {
            Color32::from_rgb(30, 33, 41)
        } else {
            Color32::WHITE
        })
        .stroke(Stroke::new(
            1.0,
            if dark {
                Color32::from_white_alpha(24)
            } else {
                Color32::from_rgb(228, 231, 239)
            },
        ))
        .corner_radius(16)
        .inner_margin(24)
        .shadow(egui::epaint::Shadow {
            offset: [0, 12],
            blur: 44,
            spread: 0,
            color: Color32::from_black_alpha(if dark { 90 } else { 30 }),
        })
}
pub(super) fn modal(ctx: &egui::Context, id: &str) -> egui::Modal {
    egui::Modal::new(egui::Id::new(id))
        .frame(frame(ctx))
        .backdrop_color(Color32::from_rgba_unmultiplied(24, 30, 48, 92))
}
pub(super) fn window<'a>(ctx: &egui::Context, title: &'a str) -> egui::Window<'a> {
    egui::Window::new(title)
        .title_bar(false)
        .collapsible(false)
        .frame(frame(ctx))
        .default_pos(ctx.content_rect().center() - vec2(270.0, 230.0))
}
fn setup(ui: &mut egui::Ui) {
    let style = ui.style_mut();
    style.spacing.item_spacing = vec2(10.0, 10.0);
    style.spacing.button_padding = vec2(14.0, 9.0);
    style.spacing.interact_size.y = 36.0;
    style.visuals.override_text_color = Some(if style.visuals.dark_mode {
        Color32::from_gray(229)
    } else {
        INK
    });
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.corner_radius = 7.into();
    }
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(13.0));
}
/// Return true only for an explicit click on the close action.
pub(super) fn header(ui: &mut egui::Ui, symbol: Symbol, title: &str, subtitle: &str) -> bool {
    setup(ui);
    let mut close = false;
    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(42.0, 42.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 12.0, Color32::from_rgb(241, 237, 255));
        paint_symbol(ui.painter(), rect.shrink(11.0), symbol, ACCENT);
        let text_width = (ui.available_width() - 28.0 - ui.spacing().item_spacing.x).max(80.0);
        ui.allocate_ui_with_layout(
            vec2(text_width, 42.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_min_width(text_width);
                ui.spacing_mut().item_spacing.y = 5.0;
                ui.label(RichText::new(title).size(21.0).strong());
                if !subtitle.is_empty() {
                    ui.label(RichText::new(subtitle).size(12.0).color(MUTED));
                }
            },
        );
        let (rect, response) = ui.allocate_exact_size(vec2(28.0, 28.0), egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                ui.is_enabled(),
                format!("关闭{title}"),
            )
        });
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, 7.0, Color32::from_black_alpha(6));
        }
        for (a, b) in [
            (vec2(-4.0, -4.0), vec2(4.0, 4.0)),
            (vec2(-4.0, 4.0), vec2(4.0, -4.0)),
        ] {
            ui.painter().line_segment(
                [rect.center() + a, rect.center() + b],
                Stroke::new(1.4, MUTED),
            );
        }
        close = response.on_hover_text("关闭").clicked();
    });
    ui.add_space(14.0);
    close
}
pub(super) fn section<R>(
    ui: &mut egui::Ui,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let fill = if ui.visuals().dark_mode {
        Color32::from_rgb(38, 42, 53)
    } else {
        Color32::from_rgb(247, 248, 252)
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(
            1.0,
            if ui.visuals().dark_mode {
                Color32::from_gray(58)
            } else {
                Color32::from_rgb(235, 237, 244)
            },
        ))
        .corner_radius(10)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            content(ui)
        })
}
pub(super) fn detail(ui: &mut egui::Ui, title: &str, body: &str) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.label(RichText::new(title).strong().size(13.0));
        ui.label(RichText::new(body).color(MUTED).size(12.0));
    });
}
pub(super) fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(MUTED));
}
pub(super) fn actions<R>(
    ui: &mut egui::Ui,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 38.0),
        egui::Layout::right_to_left(egui::Align::Center),
        content,
    )
}
pub(super) fn primary(label: &str, color: Color32) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).color(Color32::WHITE))
        .fill(color)
        .stroke(Stroke::NONE)
        .corner_radius(8)
        .min_size(vec2(100.0, 38.0))
}
pub(super) fn secondary(label: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).color(INK))
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, Color32::from_rgb(220, 224, 234)))
        .corner_radius(8)
        .min_size(vec2(76.0, 38.0))
}
pub(super) fn danger(label: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).color(Color32::from_rgb(178, 65, 72)))
        .fill(Color32::from_rgb(255, 245, 245))
        .stroke(Stroke::NONE)
        .corner_radius(8)
        .min_size(vec2(88.0, 38.0))
}
fn paint_symbol(p: &egui::Painter, r: egui::Rect, symbol: Symbol, color: Color32) {
    let c = r.center();
    let v = |x: f32, y: f32| c + vec2(x, y);
    let s = Stroke::new(1.55, color);
    match symbol {
        Symbol::Shield => {
            p.add(egui::Shape::closed_line(
                vec![
                    v(0.0, -10.0),
                    v(8.0, -6.0),
                    v(7.0, 3.0),
                    v(0.0, 10.0),
                    v(-7.0, 3.0),
                    v(-8.0, -6.0),
                ],
                s,
            ));
            p.line_segment([v(-4.0, 0.0), v(-1.0, 3.0)], s);
            p.line_segment([v(-1.0, 3.0), v(4.0, -3.0)], s);
        }
        Symbol::Save => {
            p.rect_stroke(r, 3.0, s, StrokeKind::Inside);
            p.rect_stroke(
                egui::Rect::from_min_max(v(-5.0, -9.0), v(4.0, -2.0)),
                1.0,
                s,
                StrokeKind::Inside,
            );
            p.rect_stroke(
                egui::Rect::from_min_max(v(-5.0, 3.0), v(5.0, 9.0)),
                1.0,
                s,
                StrokeKind::Inside,
            );
        }
        Symbol::Text => {
            p.line_segment([v(-7.0, -7.0), v(7.0, -7.0)], s);
            p.line_segment([v(0.0, -7.0), v(0.0, 8.0)], s);
            p.line_segment([v(-4.0, 8.0), v(4.0, 8.0)], s);
        }
        Symbol::Info => {
            p.circle_stroke(c, 9.0, s);
            p.circle_filled(v(0.0, -4.0), 1.0, color);
            p.line_segment([v(0.0, 0.0), v(0.0, 5.0)], s);
        }
        Symbol::Settings => {
            for (y, x) in [(-6.0, 3.0), (0.0, -3.0), (6.0, 2.0)] {
                p.line_segment([v(-9.0, y), v(9.0, y)], s);
                p.circle(v(x, y), 2.3, Color32::from_rgb(241, 237, 255), s);
            }
        }
        Symbol::History => {
            p.circle_stroke(c, 9.0, s);
            p.line_segment([v(0.0, -5.0), v(0.0, 0.0)], s);
            p.line_segment([v(0.0, 0.0), v(5.0, 3.0)], s);
        }
        Symbol::Link => {
            for d in [-1.0, 1.0] {
                p.rect_stroke(
                    egui::Rect::from_center_size(v(d * 4.0, -d * 3.0), vec2(10.0, 7.0)),
                    3.0,
                    s,
                    StrokeKind::Inside,
                );
            }
            p.line_segment([v(-3.0, 2.0), v(3.0, -2.0)], s);
        }
        Symbol::Preview => {
            p.rect_stroke(r, 3.0, s, StrokeKind::Inside);
            p.add(egui::Shape::convex_polygon(
                vec![v(-2.0, -5.0), v(5.0, 0.0), v(-2.0, 5.0)],
                color,
                Stroke::NONE,
            ));
        }
    }
}
