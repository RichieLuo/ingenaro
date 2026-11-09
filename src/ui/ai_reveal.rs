//! Sparse, presentation-only AI transactions. The committed project is never animated in place.
use std::{borrow::Cow, collections::HashMap};

use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke, vec2};
use ingenaro::model::{Project, Widget};

mod naro;
use naro::NaroAnimation;

const DRAW_SECONDS: f64 = 0.52;
const BATCH_SECONDS: f64 = 2.8;

#[derive(Default)]
pub(super) struct DesignReveal {
    entries: HashMap<(u64, u64), Entry>,
    naro: Option<NaroAnimation>,
}

struct Entry {
    // Exact final-state fingerprint: unlike a partial geometry hash, this also
    // rejects newer human text, visibility, image, vector and interaction edits.
    expected: Widget,
    from: Option<Widget>,
    from_opacity: f32,
    start: f64,
    page_index: usize,
    widget_index: usize,
}

pub(super) struct Sample<'a> {
    pub widget: Cow<'a, Widget>,
    // Painter opacity also fades image placeholders and other fixed-color primitives.
    pub opacity: f32,
    pub revealing: bool,
    pub progress: Option<f32>,
}

impl Entry {
    fn progress(&self, time: f64) -> f32 {
        ((time - self.start) / DRAW_SECONDS).clamp(0.0, 1.0) as f32
    }
    fn unfinished(&self, time: f64) -> bool {
        time.is_finite() && time < self.start + DRAW_SECONDS
    }
}
impl DesignReveal {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(super) fn is_active(&self, time: f64) -> bool {
        self.entries.values().any(|entry| entry.unfinished(time))
    }

    /// Unfinished visible widgets, including those waiting for their stagger slot.
    pub(super) fn active_count(&self, time: f64) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.unfinished(time))
            .count()
    }

    pub(super) fn remaining_seconds(&self, time: f64) -> f64 {
        if !time.is_finite() {
            return 0.0;
        }
        self.entries.values().fold(0.0, |remaining, entry| {
            remaining.max(entry.start + DRAW_SECONDS - time)
        })
    }

    /// Called only after a validated transaction has committed once to history.
    /// Preserve unaffected in-flight entries; retarget changed ones from their
    /// currently displayed geometry instead of snapping back to a prior document.
    pub(super) fn record(&mut self, before: &Project, after: &Project, time: f64) {
        if !time.is_finite() {
            self.clear();
            return;
        }
        self.prune(before, time);
        let old: HashMap<_, _> = before
            .pages
            .iter()
            .flat_map(|page| {
                page.widgets
                    .iter()
                    .map(move |widget| ((page.id, widget.id), widget))
            })
            .collect();
        let count = after
            .pages
            .iter()
            .flat_map(|page| {
                page.widgets
                    .iter()
                    .map(move |widget| ((page.id, widget.id), widget))
            })
            .filter(|(key, widget)| {
                !widget.hidden && old.get(key).is_none_or(|old| *old != *widget)
            })
            .count();
        let stagger = if count > 1 {
            ((BATCH_SECONDS - DRAW_SECONDS) / (count - 1) as f64).min(0.11)
        } else {
            0.0
        };
        let mut slot = 0;
        for (page_index, page) in after.pages.iter().enumerate() {
            for (widget_index, widget) in page.widgets.iter().enumerate() {
                let key = (page.id, widget.id);
                if widget.hidden {
                    continue;
                }
                if old.get(&key).is_some_and(|old| *old == widget) {
                    if let Some(entry) = self.entries.get_mut(&key) {
                        entry.page_index = page_index;
                        entry.widget_index = widget_index;
                    }
                    continue;
                }
                let (from, from_opacity) =
                    old.get(&key)
                        .filter(|old| !old.hidden)
                        .map_or((None, 0.0), |old| {
                            let sample = self.sample(page.id, old, time);
                            (Some(sample.widget.into_owned()), sample.opacity)
                        });
                self.entries.insert(
                    key,
                    Entry {
                        expected: widget.clone(),
                        from,
                        from_opacity,
                        start: time + slot as f64 * stagger,
                        page_index,
                        widget_index,
                    },
                );
                slot += 1;
            }
        }
        self.prune(after, time);
    }

    /// O(changed widgets), without scanning or cloning whole pages each frame.
    /// Reordering/missing slots safely finish the affected presentation as well.
    pub(super) fn prune(&mut self, project: &Project, time: f64) {
        self.entries.retain(|(page_id, _), entry| {
            entry.unfinished(time)
                && project.pages.get(entry.page_index).is_some_and(|page| {
                    page.id == *page_id
                        && page
                            .widgets
                            .get(entry.widget_index)
                            .is_some_and(|widget| !widget.hidden && widget == &entry.expected)
                })
        });
    }

    /// Unchanged, stale, hidden and finished widgets are borrowed, not cloned.
    pub(super) fn sample<'a>(&'a self, page_id: u64, widget: &'a Widget, time: f64) -> Sample<'a> {
        let original = || Sample {
            widget: Cow::Borrowed(widget),
            opacity: 1.0,
            revealing: false,
            progress: None,
        };
        let Some(entry) = self.entries.get(&(page_id, widget.id)) else {
            return original();
        };
        if widget.hidden || widget != &entry.expected || !entry.unfinished(time) {
            return original();
        }
        let progress = entry.progress(time);
        if progress == 0.0 {
            return Sample {
                widget: Cow::Borrowed(entry.from.as_ref().unwrap_or(widget)),
                opacity: entry.from_opacity,
                revealing: true,
                progress: (time >= entry.start).then_some(0.0),
            };
        }
        let eased = progress * progress * (3.0 - 2.0 * progress);
        let mut drawn = widget.clone();
        if let Some(from) = &entry.from {
            drawn.x = mix(from.x, widget.x, eased);
            drawn.y = mix(from.y, widget.y, eased);
            drawn.width = mix(from.width, widget.width, eased);
            drawn.height = mix(from.height, widget.height, eased);
            drawn.opacity = mix(from.opacity, widget.opacity, eased);
            drawn.radius = mix(from.radius, widget.radius, eased);
            drawn.font_size = mix(from.font_size, widget.font_size, eased);
            let turn = (widget.rotation - from.rotation + 180.0).rem_euclid(360.0) - 180.0;
            drawn.rotation = from.rotation + turn * eased;
            drawn.fill = mix_color(&from.fill, &widget.fill, eased);
            drawn.stroke = mix_color(&from.stroke, &widget.stroke, eased);
            drawn.text_color = mix_color(&from.text_color, &widget.text_color, eased);
        } else {
            let scale = mix(0.975, 1.0, eased);
            drawn.width *= scale;
            drawn.height *= scale;
            drawn.x += (widget.width - drawn.width) * 0.5;
            drawn.y += (widget.height - drawn.height) * 0.5 + 7.0 * (1.0 - eased);
            drawn.font_size *= scale;
            drawn.radius *= scale;
            if let Some(layout) = &mut drawn.text_layout {
                layout.padding = layout.padding.map(|padding| padding * scale);
            }
        }
        Sample {
            widget: Cow::Owned(drawn),
            opacity: mix(entry.from_opacity, 1.0, eased),
            revealing: true,
            progress: Some(progress),
        }
    }

    /// A real local paint status, never a claimed model-generation percentage.
    pub(super) fn paint_status(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        page_id: u64,
        time: f64,
        focus: Option<(Rect, f32)>,
    ) {
        if !self
            .entries
            .iter()
            .any(|((page, _), entry)| *page == page_id && entry.unfinished(time))
        {
            return;
        }
        let clip = painter.clip_rect();
        if !clip.is_positive() {
            return;
        }
        let focus = focus
            .map(|(bounds, progress)| (bounds.expand(4.0).intersect(clip), progress))
            .filter(|(bounds, _)| bounds.is_positive());
        if let Some((bounds, progress)) = focus {
            let stroke = Stroke::new(1.3, super::PURPLE.gamma_multiply(0.65));
            let length = 11.0_f32
                .min(bounds.width() * 0.3)
                .min(bounds.height() * 0.3);
            for (corner, dx, dy) in [
                (bounds.left_top(), 1.0, 1.0),
                (bounds.right_top(), -1.0, 1.0),
                (bounds.left_bottom(), 1.0, -1.0),
                (bounds.right_bottom(), -1.0, -1.0),
            ] {
                painter.line_segment([corner + vec2(dx * length, 0.0), corner], stroke);
                painter.line_segment([corner, corner + vec2(0.0, dy * length)], stroke);
            }
            let y = mix(bounds.top(), bounds.bottom(), progress);
            let thread_end = mix(bounds.left(), bounds.right(), progress);
            painter.line_segment(
                [Pos2::new(bounds.left(), y), Pos2::new(thread_end, y)],
                Stroke::new(1.5, super::PURPLE.gamma_multiply(0.28)),
            );
            if progress > 0.12 {
                for fraction in [0.25_f32, 0.5, 0.75] {
                    if progress >= fraction {
                        let strand_y = mix(bounds.top(), bounds.bottom(), fraction);
                        painter.line_segment(
                            [
                                Pos2::new(bounds.left(), strand_y),
                                Pos2::new(bounds.right(), strand_y),
                            ],
                            Stroke::new(
                                0.9,
                                egui::Color32::from_rgba_unmultiplied(97, 41, 232, 34),
                            ),
                        );
                    }
                }
            }
            self.paint_naro(ctx, painter, bounds, time);
        }
        let galley = painter.layout_no_wrap(
            "Naro 正在编织…".into(),
            FontId::proportional(11.0),
            super::PURPLE,
        );
        let size = galley.size() + vec2(22.0, 12.0);
        let anchor = focus.map_or(clip.min + vec2(12.0, 12.0), |(bounds, _)| {
            bounds.min - vec2(0.0, size.y + 8.0)
        });
        let min = anchor
            .max(clip.min + vec2(5.0, 5.0))
            .min((clip.max - size - vec2(5.0, 5.0)).max(clip.min));
        painter.rect_filled(
            Rect::from_min_size(min + vec2(0.0, 2.0), size),
            6.0,
            Color32::from_black_alpha(9),
        );
        painter.rect_filled(
            Rect::from_min_size(min, size),
            6.0,
            Color32::from_rgba_unmultiplied(250, 248, 255, 245),
        );
        painter.galley(min + vec2(11.0, 6.0), galley, super::PURPLE);
    }

    fn paint_naro(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        bounds: Rect,
        time: f64,
    ) {
        let clip = painter.clip_rect();
        let size = 36.0_f32.min(clip.width().min(clip.height()) - 6.0);
        if size <= 0.0 {
            return;
        }
        let min = (bounds.right_top() + vec2(5.0, -20.0))
            .max(clip.min + vec2(3.0, 3.0))
            .min(clip.max - vec2(size + 3.0, size + 3.0));
        let sprite = Rect::from_min_size(min, vec2(size, size));
        let thread = Stroke::new(1.15, Color32::from_rgba_unmultiplied(173, 141, 240, 150));
        painter.line_segment([sprite.center(), bounds.left_top()], thread);
        painter.line_segment([sprite.center(), bounds.right_top()], thread);
        let animation = self.naro.get_or_insert_with(|| NaroAnimation::load(ctx));
        painter.image(
            animation.frame_at(time).id(),
            sprite,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}

fn mix(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

fn mix_color(from: &str, to: &str, progress: f32) -> String {
    if from == to {
        return to.to_owned();
    }
    let parse = |value: &str| -> Option<[u8; 4]> {
        let value = value.strip_prefix('#')?;
        if !value.is_ascii() || !matches!(value.len(), 6 | 8) {
            return None;
        }
        let mut channels = [255; 4];
        for (index, channel) in channels.iter_mut().enumerate().take(value.len() / 2) {
            *channel = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
        }
        Some(channels)
    };
    let (Some(from), Some(to_color)) = (parse(from), parse(to)) else {
        return to.to_owned();
    };
    let rgba = std::array::from_fn::<_, 4, _>(|index| {
        mix(from[index] as f32, to_color[index] as f32, progress).round() as u8
    });
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        rgba[0], rgba[1], rgba[2], rgba[3]
    )
}

#[cfg(test)]
#[path = "ai_reveal/tests.rs"]
mod tests;
