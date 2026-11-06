//! A discrete, model-backed slider: auto first, then only recognized supported levels.
use super::modes::{MUTED, WIDTH};
use super::*;

fn steps(supported: &[String]) -> Vec<Option<ai::ReasoningEffort>> {
    let mut values = vec![None];
    values.extend(
        [
            ai::ReasoningEffort::Minimal,
            ai::ReasoningEffort::Low,
            ai::ReasoningEffort::Medium,
            ai::ReasoningEffort::High,
            ai::ReasoningEffort::Xhigh,
            ai::ReasoningEffort::Max,
        ]
        .into_iter()
        .filter(|value| supported.iter().any(|name| name == value.as_str()))
        .map(Some),
    );
    values
}

pub(super) fn row(
    ui: &mut egui::Ui,
    configured: Option<ai::ReasoningEffort>,
    supported: &[String],
) -> Option<ai::ReasoningEffort> {
    let values = steps(supported);
    let enabled = values.len() > 1;
    let current = values
        .iter()
        .position(|value| *value == configured)
        .unwrap_or(0);
    let mut selected = current;
    let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, 38.0), Sense::hover());
    let track = Rect::from_center_size(rect.right_center() - vec2(62.0, 0.0), vec2(108.0, 20.0));
    let mut response = ui.interact(
        track.expand2(vec2(0.0, 5.0)),
        ui.make_persistent_id("ai-effort-track"),
        if enabled {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        },
    );
    let left = track.left() + 10.0;
    let stride = (track.width() - 20.0) / (values.len() - 1).max(1) as f32;
    let closest = |x: f32| (((x - left) / stride).round().max(0.0) as usize).min(values.len() - 1);
    let mut interacted = false;
    if enabled {
        if (response.clicked() || response.dragged())
            && let Some(pos) = response.interact_pointer_pos()
        {
            selected = closest(pos.x);
            interacted = true;
            response.request_focus();
        }
        if response.has_focus() {
            ui.ctx().memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        ..Default::default()
                    },
                );
            });
            let mut keyboard = false;
            ui.input_mut(|input| {
                for key in [
                    egui::Key::ArrowLeft,
                    egui::Key::ArrowRight,
                    egui::Key::Home,
                    egui::Key::End,
                ] {
                    if input.consume_key(egui::Modifiers::NONE, key) {
                        keyboard = true;
                        selected = match key {
                            egui::Key::ArrowLeft => selected.saturating_sub(1),
                            egui::Key::ArrowRight => (selected + 1).min(values.len() - 1),
                            egui::Key::Home => 0,
                            egui::Key::End => values.len() - 1,
                            _ => selected,
                        };
                    }
                }
            });
            if keyboard {
                // A focus filter takes effect one frame after gaining focus. Cancel any
                // already-scheduled navigation too, so the first arrow cannot escape.
                ui.ctx()
                    .memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
                interacted = true;
            }
        }
    }
    if interacted && values[selected] != configured {
        response.mark_changed();
    }
    let label = format!("思考（{}）", effort_short_label(values[selected]));
    response.widget_info(|| egui::WidgetInfo::slider(enabled, selected as f64, &label));
    let p = ui.painter();
    thinking_icon(p, rect.left_center() + vec2(18.0, 0.0), MUTED);
    p.text(
        rect.left_center() + vec2(38.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.0),
        MUTED,
    );
    p.rect_filled(track, 10.0, Color32::from_gray(233));
    if enabled && selected > 0 {
        // Clip a full capsule rather than rounding a shorter rectangle: the fill
        // ends under the thumb, keeps the left cap, and reaches the end at max.
        let end = if selected == values.len() - 1 {
            track.right()
        } else {
            left + selected as f32 * stride
        };
        p.with_clip_rect(p.clip_rect().intersect(Rect::from_min_max(
            track.min,
            Pos2::new(end, track.bottom()),
        )))
        .rect_filled(track, 10.0, PURPLE);
    }
    for (index, value) in values.iter().enumerate() {
        let center = Pos2::new(left + index as f32 * stride, track.center().y);
        if index == selected {
            p.circle_filled(
                center,
                7.0,
                if enabled {
                    PURPLE
                } else {
                    Color32::from_gray(180)
                },
            );
            p.circle_stroke(
                center,
                7.0,
                Stroke::new(1.5, if enabled { Color32::WHITE } else { MUTED }),
            );
        } else {
            p.circle_filled(
                center,
                2.0,
                if enabled && index < selected {
                    Color32::from_white_alpha(190)
                } else {
                    Color32::from_gray(177)
                },
            );
        }
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::new(("ai-effort-step-test", value.map_or("auto", |v| v.as_str()))),
                Rect::from_center_size(center, vec2(stride.min(20.0), 28.0)),
            );
        });
        #[cfg(not(test))]
        let _ = value;
    }
    if response.has_focus() {
        p.rect_stroke(
            track.expand(3.0),
            12.0,
            Stroke::new(1.0, PURPLE),
            StrokeKind::Inside,
        );
    }
    let tooltip = if enabled {
        let hover = response
            .hover_pos()
            .map(|pos| closest(pos.x))
            .unwrap_or(selected);
        format!(
            "{}\n点击或拖动切换 · ← → 调节\n高强度通常需要等待更久；生成期间可随时停止。",
            effort_label(values[hover]),
        )
    } else {
        "当前模型未声明可选思考档位，沿用引擎默认。".into()
    };
    response.on_hover_text(tooltip).on_hover_cursor(if enabled {
        egui::CursorIcon::PointingHand
    } else {
        egui::CursorIcon::Default
    });
    if interacted {
        values[selected]
    } else {
        configured
    }
}

#[cfg(test)]
pub(in crate::ui::ai_workbench) fn bounds(
    ctx: &egui::Context,
    value: Option<ai::ReasoningEffort>,
) -> Rect {
    ctx.data(|data| {
        data.get_temp(egui::Id::new((
            "ai-effort-step-test",
            value.map_or("auto", |value| value.as_str()),
        )))
    })
    .expect("effort step rendered")
}

fn thinking_icon(p: &egui::Painter, center: Pos2, color: Color32) {
    let stroke = Stroke::new(1.1, color);
    for (x, height) in [(-6.0, 7.0), (-3.0, 12.0), (3.0, 12.0), (6.0, 7.0)] {
        p.rect_stroke(
            Rect::from_center_size(center + vec2(x, 0.0), vec2(3.0, height)),
            1.0,
            stroke,
            StrokeKind::Inside,
        );
    }
    p.line_segment([center - vec2(1.5, 0.0), center + vec2(1.5, 0.0)], stroke);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_steps_are_ordered_deduplicated_and_ignore_unknown_values() {
        let supported = ["max", "low", "unknown", "high", "low"].map(str::to_owned);
        assert_eq!(
            steps(&supported),
            vec![
                None,
                Some(ai::ReasoningEffort::Low),
                Some(ai::ReasoningEffort::High),
                Some(ai::ReasoningEffort::Max)
            ],
        );
        assert_eq!(steps(&[]), vec![None]);
        assert_eq!(steps(&["unknown".into()]), vec![None]);
    }
}
