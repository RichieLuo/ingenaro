use super::*;
use rpshop::model::WidgetKind;

fn document(count: usize) -> Project {
    let mut project = Project::blank();
    for index in 0..count {
        project.pages[0].widgets.push(Widget::new(
            2 + index as u64,
            WidgetKind::Rectangle,
            index as f32 * 210.0,
            30.0,
        ));
    }
    project.next_id = count as u64 + 2;
    project
}

fn close(left: f32, right: f32) {
    assert!((left - right).abs() < 0.001, "{left} != {right}");
}

#[test]
fn new_widgets_stagger_then_return_exact_borrowed_final_without_mutation() {
    let before = document(0);
    let after = document(3);
    let committed = after.clone();
    let mut reveal = DesignReveal::default();
    reveal.record(&before, &after, 10.0);
    assert_eq!(reveal.active_count(10.0), 3);
    let first = reveal.sample(1, &after.pages[0].widgets[0], 10.055);
    let waiting = reveal.sample(1, &after.pages[0].widgets[1], 10.055);
    assert!(first.opacity > 0.0 && first.opacity < 1.0);
    assert_eq!(waiting.opacity, 0.0);
    assert!(waiting.revealing && waiting.progress.is_none());
    let original = &after.pages[0].widgets[0];
    assert!(first.widget.width < original.width && first.widget.y > original.y);
    let half = reveal.sample(1, original, 10.26);
    close(half.opacity, 0.5);
    let done = reveal.sample(1, original, 11.0);
    assert!(matches!(done.widget, Cow::Borrowed(_)));
    assert_eq!(&*done.widget, original);
    assert!(!done.revealing && done.progress.is_none());
    assert!(!reveal.is_active(11.0));
    assert_eq!(reveal.active_count(11.0), 0);
    assert_eq!(after, committed);
}

#[test]
fn modified_geometry_colors_opacity_and_shortest_rotation_interpolate() {
    let mut before = document(2);
    let old = &mut before.pages[0].widgets[0];
    old.fill = "#00000000".into();
    old.rotation = 350.0;
    old.opacity = 0.2;
    let mut after = before.clone();
    let target = &mut after.pages[0].widgets[0];
    target.x += 120.0;
    target.y += 80.0;
    target.width *= 2.0;
    target.height *= 3.0;
    target.fill = "#FFFFFF".into();
    target.rotation = 10.0;
    target.opacity = 0.8;
    let mut reveal = DesignReveal::default();
    reveal.record(&before, &after, 0.0);
    let initial = reveal.sample(1, &after.pages[0].widgets[0], 0.0);
    assert_eq!(&*initial.widget, &before.pages[0].widgets[0]);
    let half = reveal.sample(1, &after.pages[0].widgets[0], DRAW_SECONDS / 2.0);
    close(half.widget.x, 60.0);
    close(half.widget.y, 70.0);
    close(half.widget.width, 270.0);
    close(half.widget.height, 200.0);
    close(half.widget.opacity, 0.5);
    close(half.widget.rotation, 360.0);
    assert_eq!(half.widget.fill, "#80808080");
    assert_eq!(half.opacity, 1.0);
    assert!(matches!(
        reveal.sample(1, &after.pages[0].widgets[1], 0.2).widget,
        Cow::Borrowed(_)
    ));
}

#[test]
fn cancel_finishes_immediately_and_noop_batches_stay_idle() {
    let before = document(0);
    let after = document(2);
    let mut reveal = DesignReveal::default();
    reveal.record(&before, &after, 0.0);
    assert!(reveal.is_active(0.1));
    reveal.clear();
    assert!(!reveal.is_active(0.1));
    assert_eq!(reveal.remaining_seconds(0.1), 0.0);
    let sample = reveal.sample(1, &after.pages[0].widgets[1], 0.1);
    assert_eq!(sample.opacity, 1.0);
    assert!(!sample.revealing);
    assert!(matches!(sample.widget, Cow::Borrowed(_)));
    reveal.record(&after, &after, 0.1);
    assert!(!reveal.is_active(0.1));
}

#[test]
fn hidden_widgets_and_deleted_widgets_never_reappear() {
    let mut before = document(2);
    before.pages[0].widgets[1].hidden = true;
    let mut after = before.clone();
    after.pages[0].widgets[1].x += 20.0;
    after.pages[0].widgets.remove(0);
    let mut reveal = DesignReveal::default();
    reveal.record(&before, &after, 0.0);
    assert!(!reveal.is_active(0.0));
    let hidden = reveal.sample(1, &after.pages[0].widgets[0], 0.1);
    assert!(hidden.widget.hidden);
    let mut visible = after.clone();
    visible.pages[0].widgets[0].hidden = false;
    reveal.record(&after, &visible, 0.2);
    assert_eq!(
        reveal.sample(1, &visible.pages[0].widgets[0], 0.2).opacity,
        0.0
    );
}

#[test]
fn stale_text_image_visibility_and_geometry_edits_are_never_masked() {
    let before = document(0);
    let after = document(1);
    let mut reveal = DesignReveal::default();
    reveal.record(&before, &after, 0.0);
    for variant in 0..4 {
        let mut edited = after.clone();
        let widget = &mut edited.pages[0].widgets[0];
        match variant {
            0 => widget.text = "human edit".into(),
            1 => widget.image_data = Some("new image".into()),
            2 => widget.hidden = true,
            _ => widget.x += 1.0,
        }
        let sample = reveal.sample(1, widget, 0.1);
        assert!(matches!(sample.widget, Cow::Borrowed(_)));
        assert_eq!(&*sample.widget, widget);
        assert!(!sample.revealing);
        assert_eq!(sample.opacity, 1.0);
    }
    reveal.prune(&before, 0.1); // undo without a stale deleted-widget ghost
    assert!(!reveal.is_active(0.1));
}

#[test]
fn multi_page_reveals_use_stable_ids_and_every_batch_is_under_three_seconds() {
    let before = document(0);
    let mut after = document(80);
    let mut second = after.pages[0].clone();
    second.id = 100;
    for widget in &mut second.widgets {
        widget.id += 100;
    }
    after.pages.push(second);
    let committed = after.clone();
    let mut reveal = DesignReveal::default();
    reveal.record(&before, &after, 20.0);
    assert_eq!(reveal.active_count(20.0), 160);
    assert!(reveal.remaining_seconds(20.0) <= 3.0);
    assert!(!reveal.sample(1, &after.pages[1].widgets[0], 20.1).revealing);
    let second_page = reveal.sample(100, &after.pages[1].widgets[0], 20.1);
    assert!(second_page.revealing);
    for time in [20.0, 20.1, 20.8, 21.7, 22.99] {
        for page in &after.pages {
            for widget in &page.widgets {
                let _ = reveal.sample(page.id, widget, time);
            }
        }
    }
    reveal.prune(&after, 23.0);
    assert!(reveal.entries.is_empty());
    assert_eq!(after, committed);
}

#[test]
fn consecutive_transactions_retarget_from_the_displayed_state_and_keep_other_work() {
    let before = document(0);
    let middle = document(2);
    let mut reveal = DesignReveal::default();
    reveal.record(&before, &middle, 0.0);
    let displayed = reveal.sample(1, &middle.pages[0].widgets[0], 0.2);
    let expected = displayed.widget.into_owned();
    let opacity = displayed.opacity;
    let mut after = middle.clone();
    after.pages[0].widgets[0].x += 300.0;
    after.pages[0].widgets[1].width += 20.0;
    reveal.record(&middle, &after, 0.2);
    let retargeted = reveal.sample(1, &after.pages[0].widgets[0], 0.2);
    assert_eq!(&*retargeted.widget, &expected);
    close(retargeted.opacity, opacity);
    assert_eq!(reveal.active_count(0.2), 2);
    let mut latest = after.clone();
    latest.pages[0]
        .widgets
        .push(Widget::new(5, WidgetKind::Image, 10.0, 10.0));
    let old_start = reveal.entries[&(1, 2)].start;
    reveal.record(&after, &latest, 0.3);
    assert_eq!(reveal.entries[&(1, 2)].start, old_start);
    assert_eq!(reveal.active_count(0.3), 3);
    latest.pages[0].widgets.swap(0, 1);
    reveal.prune(&latest, 0.31);
    let reordered = reveal.sample(1, &latest.pages[0].widgets[0], 0.31);
    assert!(!reordered.revealing);
}

#[test]
fn rotated_images_keep_center_rotation_and_unmodified_source_geometry() {
    let before = document(0);
    let mut after = document(1);
    let image = &mut after.pages[0].widgets[0];
    image.kind = WidgetKind::Image;
    image.rotation = 73.0;
    image.image_data = Some("data:image/png;base64,unchanged".into());
    let mut reveal = DesignReveal::default();
    reveal.record(&before, &after, 0.0);
    let image = &after.pages[0].widgets[0];
    let sample = reveal.sample(1, image, DRAW_SECONDS / 2.0);
    close(
        sample.widget.x + sample.widget.width / 2.0,
        image.x + image.width / 2.0,
    );
    close(
        sample.widget.y + sample.widget.height / 2.0,
        image.y + image.height / 2.0 + 3.5,
    );
    assert_eq!(sample.widget.rotation, image.rotation);
    assert_eq!(sample.widget.image_data, image.image_data);
    assert!(matches!(
        reveal.sample(1, image, f64::NAN).widget,
        Cow::Borrowed(_)
    ));
    reveal.record(&before, &after, f64::INFINITY);
    assert!(!reveal.is_active(0.0));
}

#[test]
fn sampled_paint_is_clipped_and_does_not_mutate_the_committed_project() {
    let before = document(0);
    let after = document(2);
    let committed = after.clone();
    let mut reveal = DesignReveal::default();
    reveal.record(&before, &after, 0.0);
    let context = egui::Context::default();
    let clip = Rect::from_min_size(Pos2::ZERO, vec2(300.0, 200.0));
    for time in [0.0, 0.2, 1.0] {
        let output = context.run(
            egui::RawInput {
                time: Some(time),
                ..Default::default()
            },
            |ctx| {
                let painter = ctx
                    .layer_painter(egui::LayerId::background())
                    .with_clip_rect(clip);
                for widget in &after.pages[0].widgets {
                    let sample = reveal.sample(1, widget, time);
                    if sample.opacity > 0.0 {
                        let mut painter = painter.clone();
                        painter.multiply_opacity(sample.opacity);
                        super::super::render::draw_widget(
                            &painter,
                            &sample.widget,
                            Pos2::ZERO,
                            1.0,
                            None,
                        );
                    }
                }
            },
        );
        assert!(
            output
                .shapes
                .iter()
                .all(|shape| clip.contains_rect(shape.clip_rect))
        );
        if time == 0.0 {
            assert!(output.shapes.is_empty());
        } else {
            assert!(!output.shapes.is_empty());
        }
    }
    assert_eq!(after, committed);
}
