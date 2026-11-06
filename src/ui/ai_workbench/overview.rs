//! A view-only overview of page work and its results, painted with the native renderer.
use super::*;
use page_activity::{ActivityPage, Outcome};

const OVERVIEW_GAP: f32 = 16.0;
const CARD_CHROME: f32 = 94.0;
const COMPLETED: Color32 = Color32::from_rgb(21, 128, 61);

#[derive(Clone, Copy)]
struct OverviewLayout {
    columns: usize,
    rows: usize,
    card_size: egui::Vec2,
}

impl OverviewLayout {
    fn new(available: egui::Vec2, count: usize) -> Self {
        let width = available.x.max(1.0);
        let columns = if width >= 930.0 {
            3
        } else if width >= 480.0 {
            2
        } else {
            1
        }
        .min(count.max(1));
        let rows = count.div_ceil(columns);
        let card_width = (width - OVERVIEW_GAP * (columns - 1) as f32) / columns as f32;
        let mut preview_height = (card_width * 0.68).clamp(144.0, 260.0);
        // Fit a small set together when possible, without making large sets illegible.
        if (1..=3).contains(&rows) {
            let fit = (available.y - OVERVIEW_GAP * (rows - 1) as f32) / rows as f32 - CARD_CHROME;
            preview_height = preview_height.min(fit.max(144.0));
        }
        Self {
            columns,
            rows,
            card_size: vec2(card_width, preview_height + CARD_CHROME),
        }
    }

    fn height(self) -> f32 {
        self.rows as f32 * self.card_size.y + self.rows.saturating_sub(1) as f32 * OVERVIEW_GAP
    }

    fn card(self, origin: Pos2, index: usize) -> Rect {
        let offset = vec2(
            (index % self.columns) as f32 * (self.card_size.x + OVERVIEW_GAP),
            (index / self.columns) as f32 * (self.card_size.y + OVERVIEW_GAP),
        );
        Rect::from_min_size(origin + offset, self.card_size)
    }
}

impl App {
    pub(in crate::ui) fn ai_overview_return_button(&mut self, ui: &mut egui::Ui) {
        if self.ai.page_activity.can_return_to_overview()
            && ui
                .add_enabled(
                    self.confirm.is_none() && self.editing.is_none() && self.drag.is_none(),
                    egui::Button::new(RichText::new("多页总览").size(11.0).color(PURPLE)).small(),
                )
                .clicked()
        {
            self.ai.page_activity.return_to_overview();
        }
    }

    /// Occupies only the normal canvas region; page navigation and chat stay in place.
    pub(in crate::ui) fn ai_page_overview(&mut self, ctx: &egui::Context) {
        let time = ctx.input(|input| input.time);
        self.ai_reveal.prune(&self.project, time);
        // Page work stays active between document updates too. Keep the activity
        // indicators smooth even when there is no native reveal animation.
        if self.ai.page_activity.has_work() || self.ai_reveal.is_active(time) {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        let pages = self.ai.page_activity.pages();
        let focus = self
            .ai
            .page_activity
            .last_focus()
            .filter(|id| pages.iter().any(|page| page.id == *id))
            .or_else(|| {
                self.project
                    .pages
                    .get(self.page)
                    .map(|page| page.id)
                    .filter(|id| pages.iter().any(|page| page.id == *id))
            });
        let mut clicked = None;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(243, 244, 248))
                    .inner_margin(20),
            )
            .show(ctx, |ui| {
                if self.confirm.is_some() || self.editing.is_some() {
                    ui.disable();
                }
                overview_header(ui, pages, self.ai.page_agents.has_active(), time);
                egui::ScrollArea::vertical()
                    .id_salt("ai-active-page-overview-scroll")
                    .auto_shrink([false, false])
                    .show_viewport(ui, |ui, viewport| {
                        let available = vec2(ui.available_width(), viewport.height());
                        let layout = OverviewLayout::new(available, pages.len());
                        // Allocate one deterministic grid, rather than relying on measured
                        // column widths from an earlier frame. No document clones are needed.
                        let (_, grid) =
                            ui.allocate_space(vec2(available.x.max(1.0), layout.height()));
                        for (index, page) in pages.iter().enumerate() {
                            let card = layout.card(grid.min, index);
                            if self
                                .ai_overview_card(ui, card, page, focus == Some(page.id), time)
                                .is_some_and(|response| response.clicked())
                            {
                                clicked = Some(page.id);
                            }
                        }
                    });
            });
        if let Some(id) = clicked
            && self.ai_show_page(id).is_ok()
        {
            self.ai.page_activity.open_canvas();
            ctx.request_repaint();
        }
    }

    fn ai_overview_card(
        &self,
        ui: &mut egui::Ui,
        card: Rect,
        activity: &ActivityPage,
        focused: bool,
        time: f64,
    ) -> Option<egui::Response> {
        let page = self
            .project
            .pages
            .iter()
            .find(|page| page.id == activity.id)?;
        let response = ui.interact(
            card,
            ui.make_persistent_id(("ai-active-page-card", page.id)),
            Sense::click(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                ui.is_enabled(),
                focused,
                format!("{} · {}", page.name, activity.label()),
            )
        });
        if ui.is_rect_visible(card) {
            let painter = ui.painter().with_clip_rect(card.intersect(ui.clip_rect()));
            painter.rect_filled(card, 11.0, Color32::WHITE);
            painter.rect_stroke(
                card,
                11.0,
                Stroke::new(
                    if focused { 1.5 } else { 1.0 },
                    if focused || response.has_focus() {
                        PURPLE
                    } else if response.hovered() {
                        PURPLE.gamma_multiply(0.45)
                    } else {
                        BORDER
                    },
                ),
                StrokeKind::Inside,
            );
            let inset = 12.0_f32.min(card.width() * 0.08);
            let page_icon =
                Rect::from_min_size(card.min + vec2(inset + 1.0, 16.0), vec2(10.0, 13.0));
            paint_page_icon(&painter, page_icon, if focused { PURPLE } else { MUTED });
            paint_line(
                &painter,
                Pos2::new(page_icon.right() + 8.0, card.top() + 22.5),
                card.width() - inset * 2.0 - 35.0,
                &page.name,
                12.0,
                INK,
            );
            if focused {
                painter.circle_filled(
                    Pos2::new(card.right() - inset - 3.0, card.top() + 22.5),
                    3.0,
                    PURPLE,
                );
            }

            let stage =
                Rect::from_min_max(card.min + vec2(inset, 44.0), card.max - vec2(inset, 50.0));
            painter.rect_filled(stage, 6.0, Color32::from_rgb(247, 248, 251));
            painter.rect_stroke(
                stage,
                6.0,
                Stroke::new(1.0, Color32::from_rgb(237, 239, 245)),
                StrokeKind::Inside,
            );
            let (page_rect, zoom) = thumbnail_geometry(page, stage);
            let stage_painter = painter.with_clip_rect(stage.intersect(painter.clip_rect()));
            stage_painter.rect_filled(
                page_rect.translate(vec2(0.0, 2.0)).expand(1.0),
                1.0,
                Color32::from_black_alpha(8),
            );
            stage_painter.rect_filled(page_rect, 0.0, color(&page.background));
            // Images, rotated shapes and animated geometry must not bleed into the
            // letterbox, neighbouring card, or surrounding editor panels.
            let clipped =
                stage_painter.with_clip_rect(page_rect.intersect(stage_painter.clip_rect()));
            for widget in page.widgets.iter().filter(|widget| !widget.hidden) {
                let sample = self.ai_reveal.sample(page.id, widget, time);
                if sample.opacity > 0.0 {
                    let mut widget_painter = clipped.clone();
                    widget_painter.multiply_opacity(sample.opacity);
                    draw_widget(
                        &widget_painter,
                        &sample.widget,
                        page_rect.min,
                        zoom,
                        self.textures.get(&widget.id),
                    );
                }
            }
            stage_painter.rect_stroke(
                page_rect,
                0.0,
                Stroke::new(0.7, Color32::from_black_alpha(17)),
                StrokeKind::Inside,
            );

            let status = Pos2::new(card.left() + inset + 5.0, card.bottom() - 30.0);
            let tint = match activity.outcome() {
                Some(Outcome::Completed) => {
                    paint_completed(&painter, status);
                    COMPLETED
                }
                Some(Outcome::Incomplete) => {
                    painter.circle_filled(status, 3.0, MUTED);
                    MUTED
                }
                None => {
                    paint_activity(&painter, status, 4.5, time);
                    PURPLE
                }
            };
            paint_line(
                &painter,
                status + vec2(12.0, 0.0),
                card.width() - inset * 2.0 - 18.0,
                activity.label(),
                11.0,
                tint,
            );
            let [width, height] = page.size_for_output();
            paint_line(
                &painter,
                Pos2::new(card.left() + inset, card.bottom() - 13.0),
                card.width() - inset * 2.0,
                &format!(
                    "{} · {width:.0} × {height:.0}",
                    if page.canvas.is_auto() {
                        "Auto 画布"
                    } else {
                        "原生画布"
                    }
                ),
                9.5,
                MUTED,
            );
        }
        Some(
            response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(format!(
                    "{}\n{}\n点击打开页面画布",
                    page.name,
                    activity.label()
                )),
        )
    }
}

fn overview_header(ui: &mut egui::Ui, pages: &[ActivityPage], parallel: bool, time: f64) {
    let working = pages.iter().filter(|page| page.is_working()).count();
    let completed = pages
        .iter()
        .filter(|page| page.outcome() == Some(Outcome::Completed))
        .count();
    let incomplete = pages.len() - working - completed;
    let (_, rect) = ui.allocate_space(vec2(ui.available_width().max(1.0), 78.0));
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let badge = Rect::from_min_size(rect.min + vec2(0.0, 2.0), vec2(36.0, 36.0));
    painter.rect_filled(badge, 10.0, Color32::from_rgb(234, 229, 249));
    paint_page_icon(
        &painter,
        Rect::from_min_size(badge.min + vec2(9.0, 8.0), vec2(12.0, 16.0)),
        PURPLE.gamma_multiply(0.35),
    );
    paint_page_icon(
        &painter,
        Rect::from_min_size(badge.min + vec2(14.0, 12.0), vec2(12.0, 16.0)),
        PURPLE,
    );
    paint_line(
        &painter,
        rect.min + vec2(49.0, 12.0),
        rect.width() - 49.0,
        if completed == pages.len() {
            "多页设计已完成"
        } else if working == 0 {
            "多页设计已结束"
        } else if parallel {
            "多页并行设计中"
        } else {
            "多页设计中"
        },
        18.0,
        INK,
    );
    let indicator = rect.min + vec2(53.5, 34.0);
    if working > 0 {
        paint_activity(&painter, indicator, 4.0, time);
    } else if incomplete == 0 {
        paint_completed(&painter, indicator);
    } else {
        painter.circle_filled(indicator, 3.0, MUTED);
    }
    let mut caption = format!("进行中 {working} 个 · 已完成 {completed} 个");
    if incomplete > 0 {
        caption.push_str(&format!(" · 未完成 {incomplete} 个"));
    }
    paint_line(
        &painter,
        rect.min + vec2(65.0, 34.0),
        rect.width() - 65.0,
        &caption,
        11.0,
        MUTED,
    );
    paint_line(
        &painter,
        rect.min + vec2(0.0, 60.0),
        rect.width(),
        "点击缩略图打开页面画布",
        10.5,
        MUTED,
    );
}

/// The same finite output bounds as native preview/export. The document origin
/// remains (0, 0), so negative-origin content is clipped rather than recentered.
fn thumbnail_geometry(page: &Page, stage: Rect) -> (Rect, f32) {
    let [width, height] = page.size_for_output();
    let size = vec2(width.max(1.0), height.max(1.0));
    let padding = 10.0_f32.min(stage.width() * 0.1).min(stage.height() * 0.1);
    let available = stage.shrink(padding).size();
    let zoom = (available.x / size.x).min(available.y / size.y);
    (Rect::from_center_size(stage.center(), size * zoom), zoom)
}

fn paint_page_icon(painter: &egui::Painter, rect: Rect, tint: Color32) {
    painter.rect_filled(rect, 2.0, Color32::from_rgb(251, 249, 255));
    painter.rect_stroke(rect, 2.0, Stroke::new(1.0, tint), StrokeKind::Inside);
    let y = rect.center().y + 1.0;
    painter.line_segment(
        [
            Pos2::new(rect.left() + 3.0, y),
            Pos2::new(rect.right() - 3.0, y),
        ],
        Stroke::new(0.8, tint),
    );
    painter.line_segment(
        [
            Pos2::new(rect.left() + 3.0, y + 3.0),
            Pos2::new(rect.right() - 5.0, y + 3.0),
        ],
        Stroke::new(0.8, tint),
    );
}

fn paint_completed(painter: &egui::Painter, center: Pos2) {
    painter.circle_filled(center, 5.0, COMPLETED);
    painter.add(egui::Shape::line(
        vec![
            center + vec2(-2.4, 0.0),
            center + vec2(-0.7, 1.8),
            center + vec2(2.5, -1.9),
        ],
        Stroke::new(1.2, Color32::WHITE),
    ));
}

/// A neutral activity glyph, not a completion meter. Continuous time keeps every
/// indicator moving smoothly, including while waiting for the next page update.
fn paint_activity(painter: &egui::Painter, center: Pos2, radius: f32, time: f64) {
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(1.3, PURPLE.gamma_multiply(0.15)),
    );
    let angle = (time.rem_euclid(8.0) * std::f64::consts::TAU / 8.0) as f32;
    let points = (0..12)
        .map(|index| {
            let angle = angle + index as f32 / 11.0 * std::f32::consts::PI * 1.2;
            center + vec2(angle.cos(), angle.sin()) * radius
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(1.3, PURPLE)));
}

fn paint_line(
    painter: &egui::Painter,
    anchor: Pos2,
    max_width: f32,
    text: &str,
    size: f32,
    tint: Color32,
) {
    if max_width <= 0.0 {
        return;
    }
    let mut job =
        egui::text::LayoutJob::simple_singleline(text.to_owned(), FontId::proportional(size), tint);
    job.wrap.max_width = max_width;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = painter.layout_job(job);
    let top_left = anchor - vec2(0.0, galley.size().y * 0.5);
    painter.galley(top_left, galley, tint);
}

#[cfg(test)]
#[path = "overview/tests.rs"]
mod tests;
