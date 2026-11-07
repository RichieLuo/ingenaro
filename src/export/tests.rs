use super::*;

#[test]
fn form_controls_escape_values_and_scope_radio_groups_per_page() {
    let mut project = Project::blank();
    for kind in [
        WidgetKind::Radio,
        WidgetKind::Radio,
        WidgetKind::TextArea,
        WidgetKind::Droplist,
        WidgetKind::ListBox,
    ] {
        let mut widget = Widget::new(project.allocate_id(), kind, 10.0, 20.0);
        widget.name = "group\"<&".into();
        widget.text = "<first>\r\n\"second\" & value\n</textarea><script>bad()</script>".into();
        project.pages[0].widgets.push(widget);
    }
    let mut second = project.pages[0].clone();
    second.id = project.allocate_id();
    for widget in &mut second.widgets {
        widget.id = project.allocate_id();
    }
    project.pages.push(second);
    let output = html(&project).unwrap();
    let group = "group&quot;&lt;&amp;";
    for page in &project.pages {
        assert_eq!(
            output
                .matches(&format!("name=\"page-{}:{group}\"", page.id))
                .count(),
            2
        );
    }
    assert_eq!(output.matches("<textarea aria-label=").count(), 2);
    assert_eq!(output.matches(" size=\"5\"").count(), 2);
    assert!(output.contains("<option value=\"&lt;first&gt;\">&lt;first&gt;</option>"));
    assert!(output.contains("<option value=\"&quot;second&quot; &amp; value\">"));
    assert!(!output.contains("<script>bad()</script>"));
    let image = svg(&project.pages[0]).unwrap();
    assert_eq!(image.matches("<circle ").count(), 2);
    assert!(image.contains("clip-path=\"url(#form-"));
    assert!(!image.contains("<script>bad()</script>"));
}

#[test]
fn vector_presets_export_paths_with_text_and_rotation() {
    let mut project = Project::blank();
    let mut widget = Widget::new(project.allocate_id(), WidgetKind::Rectangle, 30.0, 40.0);
    widget.rotation = 25.0;
    widget.text = "decision <yes>".into();
    widget.vector_points = Some(
        json!([[{"Path":{"X":0.5,"Y":0.0},"HigherSegmentExists":true},
        {"Path":{"X":1.0,"Y":0.5},"HigherSegmentExists":true},
        {"Path":{"X":0.5,"Y":1.0},"HigherSegmentExists":true},
        {"Path":{"X":0.0,"Y":0.5},"HigherSegmentExists":true}]]),
    );
    project.pages[0].widgets.push(widget);
    let output = html(&project).unwrap();
    assert!(output.contains("class=\"vector-shape\""));
    assert!(output.contains("transform:rotate(25deg)"));
    assert!(output.contains("decision &lt;yes&gt;"));
    assert!(output.contains("background:transparent;border:1px solid transparent"));
    let image = svg(&project.pages[0]).unwrap();
    assert!(image.contains("<path transform=\"translate(30 40)\""));
    assert!(image.contains("fill-rule=\"evenodd\""));
    assert!(image.contains("decision &lt;yes&gt;"));
}

#[test]
fn native_radio_groups_keep_source_names_and_ungrouped_inputs_independent() {
    let mut widget = Widget::new(17, WidgetKind::Radio, 0.0, 0.0);
    widget.name = "visible component name".into();
    widget.radio_group = Some("source \"group\"".into());
    let mut output = String::new();
    write_html_widget(&mut output, &widget, "page-1", None).unwrap();
    assert!(output.contains("name=\"page-1:source &quot;group&quot;\""));
    assert!(output.contains("aria-label=\"visible component name\""));
    widget.radio_group = Some(String::new());
    output.clear();
    write_html_widget(&mut output, &widget, "page-1", None).unwrap();
    assert!(output.contains("name=\"page-1:widget-17\""));
}

#[test]
fn overlapping_library_contours_fill_independently_in_both_exports() {
    for (id, count) in [
        ("flow-database", 2),
        ("flow-file", 2),
        ("flow-stack-file", 3),
    ] {
        let mut project = Project::blank();
        project.pages[0].background = "#17293A".into();
        let item = crate::widget_library::ITEMS
            .iter()
            .find(|item| item.id == id)
            .unwrap();
        let mut widget = item.create(&mut project, 30.0, 40.0).remove(0);
        widget.fill = "#8A61D1".into();
        widget.opacity = 0.5;
        let paths = crate::shape_geometry::svg_paths(&widget).unwrap();
        assert_eq!(paths.len(), count, "{id}");
        project.pages[0].widgets.push(widget);
        let html = html(&project).unwrap();
        let svg = svg(&project.pages[0]).unwrap();
        for (output, prefix) in [
            (&html, "<path d=\""),
            (&svg, "<path transform=\"translate(30 40)\" d=\""),
        ] {
            assert_eq!(output.matches(prefix).count(), count, "{id}");
            assert_eq!(output.matches("fill=\"#8A61D1\"").count(), count, "{id}");
            for path in &paths {
                assert!(
                    output.contains(&format!("{prefix}{}\" fill=\"#8A61D1\"", escape_html(path))),
                    "{id}"
                );
            }
        }
    }
}

#[test]
fn authored_events_export_page_load_action_order_and_string_ids() {
    use crate::model::{AuthoredEvent, EventTrigger, Interaction};
    let mut project = Project::blank();
    let mut event = AuthoredEvent::new(project.pages[0].id, None, EventTrigger::PageLoad);
    let mut label = Widget::new(project.allocate_id(), WidgetKind::Text, 10.0, 10.0);
    label.text = "before".into();
    let target = label.id;
    project.pages[0].widgets.push(label);
    event.actions = vec![
        Interaction {
            action: Action::ToggleVisibility,
            target_widget: Some(target),
            ..Default::default()
        },
        Interaction {
            action: Action::SetText,
            target_widget: Some(target),
            value: "</script><script>unsafe</script>".into(),
            ..Default::default()
        },
    ];
    project.interaction_events.push(event);
    let output = html(&project).unwrap();
    let state = player_data(&output);
    assert_eq!(state["authored_events"][0]["trigger"], "PageLoad");
    assert_eq!(
        state["authored_events"][0]["actions"][0]["action"],
        "ToggleVisibility"
    );
    assert_eq!(
        state["authored_events"][0]["actions"][1]["widget"],
        target.to_string()
    );
    assert!(!output.contains("<script>unsafe</script>"));
    assert_eq!(
        state["authored_events"][0]["actions"][1]["value"],
        "</script><script>unsafe</script>"
    );
}

fn player_data(output: &str) -> serde_json::Value {
    serde_json::from_str(
        output
            .split("id=\"prototype-data\">")
            .nth(1)
            .unwrap()
            .split("</script>")
            .next()
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn player_directory_uses_depth_first_order_and_string_ids_for_nested_pages() {
    use crate::model::PageTreeNode;
    let mut project = Project::blank();
    let huge = 9_007_199_254_740_993;
    let mut second = project.pages[0].clone();
    second.id = huge;
    second.name = "Nested <page>".into();
    project.pages.push(second);
    project.next_id = huge + 1;
    project.page_tree = vec![PageTreeNode::Folder {
        id: "folder".into(),
        name: "Folder <script>".into(),
        children: vec![PageTreeNode::Page {
            page_id: huge,
            children: vec![PageTreeNode::Page {
                page_id: 1,
                children: vec![],
            }],
        }],
    }];
    let output = html(&project).unwrap();
    let data = player_data(&output);
    assert_eq!(data["initialPage"], huge.to_string());
    assert_eq!(data["pages"][0]["id"], huge.to_string());
    assert_eq!(data["pages"][1]["id"], "1");
    assert_eq!(data["page_tree"][0]["type"], "folder");
    assert!(
        data["page_tree"][0].get("id").is_none(),
        "a folder must never become a page target"
    );
    assert_eq!(data["page_tree"][0]["children"][0]["id"], huge.to_string());
    assert_eq!(
        data["page_tree"][0]["children"][0]["children"][0]["id"],
        "1"
    );
    assert!(!output.contains("Folder <script>"));
    assert!(output.contains("value=\"100\" selected"));
    assert!(output.contains("id=\"page-picker\" aria-expanded=\"false\""));
}

#[test]
fn player_is_one_offline_document_with_hash_navigation_and_no_old_preview_frame() {
    let output = html(&Project::default()).unwrap();
    assert!(output.contains("id=\"history-back\""));
    assert!(output.contains("id=\"page-search\""));
    assert!(output.contains("id=\"previous-page\""));
    assert!(output.contains("id=\"restore-player\""));
    assert!(!output.contains("class=\"preview-bar\""));
    assert!(!output.contains("data-open-page="));
    assert!(!output.contains("src=\"player.js\""));
    assert!(!output.contains("href=\"player.css\""));
    assert!(output.contains("history.back()") && output.contains("history.forward()"));
    assert!(output.contains("hashchange") && output.contains("popstate"));
    assert!(output.contains("match[1]") && output.contains("pages.has(match[1])"));
    assert!(!output.contains("parseInt(match[1]"));
}

#[test]
fn native_page_keys_are_explicit_escaped_and_unambiguous() {
    use std::collections::BTreeMap;
    let project = Project::default();
    let first = project.pages[0].id;
    let second = project.pages[1].id;
    let keys = BTreeMap::from([
        (first, "native:first/\"<page>".into()),
        (second, "native:second".into()),
    ]);
    let output = html_with_page_keys(&project, &keys).unwrap();
    let state = player_data(&output);
    assert_eq!(state["pages"][0]["key"], "native:first/\"<page>");
    assert_eq!(state["pages"][1]["key"], "native:second");
    assert!(!output.contains("native:first/\"<page>"));
    let duplicate = BTreeMap::from([(first, "same".into()), (second, "same".into())]);
    assert!(html_with_page_keys(&project, &duplicate).is_err());
    assert_eq!(
        player_data(&html(&project).unwrap())["pages"][0]["key"],
        format!("id-{first}")
    );
}

#[test]
fn auto_page_exports_content_bounds_and_source_text_layout() {
    let mut project = Project::blank();
    let id = project.allocate_id();
    let mut widget = Widget::new(id, WidgetKind::Rectangle, 1600.0, 20.0);
    widget.text = "source title".into();
    widget.height = 22.0;
    widget.text_layout = Some(crate::model::TextLayout {
        horizontal: 0,
        vertical: 0,
        padding: [0.0; 4],
        wrap: false,
        bold: true,
    });
    project.pages[0].canvas = crate::model::CanvasMode::Auto;
    project.pages[0].widgets.push(widget);
    let markup = html(&project).unwrap();
    assert!(markup.contains("width:1780px;height:64px"));
    assert!(markup.contains("justify-content:flex-start;align-items:flex-start;text-align:left"));
    assert!(markup.contains("font-weight:700;line-height:1.2;white-space:pre;overflow:visible"));
    let vector = svg(&project.pages[0]).unwrap();
    assert!(vector.contains("width=\"1780\" height=\"64\""));
    assert!(vector.contains(
        "font-weight=\"700\" text-anchor=\"start\" dominant-baseline=\"text-before-edge\""
    ));
    assert!(vector.contains("<tspan x=\"1600\" y=\"20\">source title</tspan>"));
}

#[test]
fn rotated_lines_keep_diagonal_geometry_in_both_exports() {
    let mut project = Project::blank();
    let id = project.allocate_id();
    let mut line = Widget::new(id, WidgetKind::Line, 10.0, 20.0);
    line.width = 120.0;
    line.height = 60.0;
    line.rotation = 37.5;
    project.pages[0].widgets.push(line);
    let html = html(&project).unwrap();
    assert!(html.contains("transform:rotate(37.5deg);transform-origin:center center"));
    assert!(html.contains("left:10px;top:20px;width:120px;height:60px;background:transparent"));
    assert!(html.contains("viewBox=\"0 0 120 60\""));
    assert!(html.contains("x1=\"0\" y1=\"0\" x2=\"120\" y2=\"60\""));
    let svg = svg(&project.pages[0]).unwrap();
    assert!(svg.contains("transform=\"rotate(37.5 70 50)\""));
    assert!(svg.contains("x1=\"10\" y1=\"20\" x2=\"130\" y2=\"80\""));
    assert!(svg.contains("stroke-width=\"4\""));
}

#[test]
fn rotated_text_uses_the_same_center_pivot_in_both_exports() {
    let mut project = Project::blank();
    let id = project.allocate_id();
    let mut text = Widget::new(id, WidgetKind::Text, 12.0, 18.0);
    text.width = 80.0;
    text.height = 24.0;
    text.rotation = -90.0;
    text.text = "Rotation".into();
    project.pages[0].widgets.push(text);
    assert!(html(&project).unwrap().contains("transform:rotate(-90deg)"));
    let output = svg(&project.pages[0]).unwrap();
    assert!(output.contains("transform=\"rotate(-90 52 30)\""));
    assert!(output.contains("Rotation</tspan>"));
}

#[test]
fn html_escapes_document_text_and_script_breakouts() {
    let mut project = Project::blank();
    project.name = "<img src=x onerror=alert(1)>".into();
    let target_id = project.allocate_id();
    let mut target = Widget::new(target_id, WidgetKind::Text, 0.0, 0.0);
    target.name = "\" onclick=\"alert(1)".into();
    target.text = "<script>alert('oops')</script> & \"quoted\"".into();
    let button_id = project.allocate_id();
    let mut button = Widget::new(button_id, WidgetKind::Button, 0.0, 100.0);
    button.interaction.action = Action::SetText;
    button.interaction.target_widget = Some(target_id);
    button.interaction.value = "</script><script>alert('breakout')</script>\u{2028}".into();
    project.pages[0].widgets.extend([target, button]);
    let output = html(&project).unwrap();
    assert!(!output.contains("<img src=x"));
    assert!(!output.contains("<script>alert("));
    assert!(output.contains("&lt;script&gt;"));
    assert!(output.contains("\\u003c/script\\u003e"));
    let json_text = output
        .split("id=\"prototype-data\">")
        .nth(1)
        .unwrap()
        .split("</script>")
        .next()
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_text).unwrap();
    assert_eq!(
        parsed["interactions"][0]["value"],
        project.pages[0].widgets[1].interaction.value
    );
}

#[test]
fn preview_exports_all_actions_and_keeps_large_ids_exact() {
    let mut project = Project::default();
    let huge = 9_007_199_254_740_993;
    project.pages[0].id = huge;
    for page in &mut project.pages {
        for widget in &mut page.widgets {
            if widget.interaction.target_page == Some(1) {
                widget.interaction.target_page = Some(huge);
            }
        }
    }
    project.next_id = huge + 1;
    let output = html(&project).unwrap();
    for action in ["Navigate", "ToggleVisibility", "SetText"] {
        assert!(output.contains(action));
    }
    assert!(output.contains(&format!("\"initialPage\":\"{huge}\"")));
    assert!(output.contains("target.hidden=!target.hidden"));
    assert!(output.contains("label.textContent=item.value"));
    assert!(!output.contains("innerHTML"));
    assert!(!output.contains("https://"));
}

#[test]
fn svg_escapes_text_skips_hidden_and_rejects_bad_dimensions() {
    let mut project = Project::blank();
    let visible_id = project.allocate_id();
    let hidden_id = project.allocate_id();
    let mut visible = Widget::new(visible_id, WidgetKind::Text, 10.0, 20.0);
    visible.text = "<unsafe> & \"text\"".into();
    let mut hidden = Widget::new(hidden_id, WidgetKind::Rectangle, 10.0, 20.0);
    hidden.hidden = true;
    project.pages[0].widgets.extend([visible, hidden]);
    let output = svg(&project.pages[0]).unwrap();
    assert!(output.contains("&lt;unsafe&gt; &amp; &quot;text&quot;"));
    assert!(!output.contains(&format!("id=\"w-{hidden_id}\"")));
    project.pages[0].width = f32::NAN;
    assert!(svg(&project.pages[0]).is_err());
}

#[test]
fn raster_image_is_embedded_in_both_exports() {
    use base64::Engine;
    let mut project = Project::blank();
    let id = project.allocate_id();
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let data = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(encoded.into_inner())
    );
    let mut widget = Widget::new(id, WidgetKind::Image, 0.0, 0.0);
    widget.image_data = Some(data.clone());
    project.pages[0].widgets.push(widget);
    assert!(html(&project).unwrap().contains(&data));
    assert!(svg(&project.pages[0]).unwrap().contains(&data));
}
