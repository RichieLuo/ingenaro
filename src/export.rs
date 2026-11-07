use crate::model::{Action, HorizontalLayout, Page, Project, Widget, WidgetKind, validate_page};
use anyhow::Result;
use serde_json::json;
use std::fmt::Write;

/// Export an entirely self-contained prototype. All document content is treated as data.
pub fn html(project: &Project) -> Result<String> {
    html_with_page_keys(project, &std::collections::BTreeMap::new())
}

/// Browser snapshots can retain a native page's identity across model ID reassignment.
pub fn html_with_page_keys(
    project: &Project,
    page_keys: &std::collections::BTreeMap<u64, String>,
) -> Result<String> {
    html_with_native(project, page_keys, None)
}

/// The executable scene is a read-only view of the native graph, independent
/// of the editor's intentionally smaller editable model.
pub fn html_with_native(
    project: &Project,
    page_keys: &std::collections::BTreeMap<u64, String>,
    native: Option<&crate::rp_format::NativePreview>,
) -> Result<String> {
    project.validate()?;
    let ordered_pages = player_page_order(project);
    let mut keys = std::collections::BTreeSet::new();
    for page in &ordered_pages {
        let key = page_keys
            .get(&page.id)
            .cloned()
            .unwrap_or_else(|| format!("id-{}", page.id));
        anyhow::ensure!(
            !key.is_empty() && key.len() <= 1024 && keys.insert(key),
            "预览页面标识为空、重复或过长"
        );
    }
    let mut output = String::with_capacity(32_768);
    write!(
        output,
        "<!doctype html>\n<html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"referrer\" content=\"no-referrer\"><link rel=\"icon\" href=\"data:,\"><title>{}</title><style>",
        escape_html(&project.name)
    )?;
    output.push_str(include_str!("player.css"));
    output.push_str(r#"</style></head><body>
<header class="player-toolbar" id="player-toolbar" aria-label="原型浏览工具栏">
<button class="icon-button" id="history-back" title="后退" aria-label="后退" disabled>←</button><button class="icon-button" id="history-forward" title="前进" aria-label="前进" disabled>→</button>
<button class="page-picker" id="page-picker" aria-expanded="false" aria-controls="player-nav" title="显示或收起页面目录"><span aria-hidden="true">☰</span><span class="page-title" id="page-title"></span><span class="page-count" id="page-count"></span><span aria-hidden="true">▾</span></button>
<span class="toolbar-spacer"></span><button class="icon-button" id="reload" title="重新载入最新预览" aria-label="重新载入">↻</button><span class="toolbar-rule"></span>
<label for="zoom">缩放</label><select id="zoom" aria-label="缩放"><option value="25">25%</option><option value="50">50%</option><option value="75">75%</option><option value="100" selected>100%</option><option value="150">150%</option><option value="200">200%</option><option value="250">250%</option><option value="300">300%</option><option value="400">400%</option><option value="fit-width">适应宽度</option><option value="fit-page">完整适应</option></select>
<button id="hotspots" aria-pressed="false" title="显示所有可交互组件">热点</button><span class="toolbar-rule"></span><button class="icon-button" id="hide-player" title="隐藏浏览工具栏" aria-label="隐藏浏览工具栏">×</button></header>
<aside class="player-nav" id="player-nav" aria-label="页面目录" hidden><div class="nav-heading"><span class="project-name" id="project-name"></span><button class="nav-tool" id="previous-page" title="上一页（逗号）" aria-label="上一页">‹</button><button class="nav-tool" id="next-page" title="下一页（句号）" aria-label="下一页">›</button><button class="nav-tool" id="expand-all" title="展开全部" aria-label="展开全部">⊞</button><button class="nav-tool" id="collapse-all" title="收起全部" aria-label="收起全部">⊟</button></div><input class="nav-search" id="page-search" type="search" placeholder="搜索页面" aria-label="搜索页面"><nav class="page-tree" id="page-tree" aria-label="项目页面"></nav><div class="nav-splitter" id="nav-splitter" tabindex="0" role="separator" aria-label="调整页面目录宽度" aria-orientation="vertical" aria-valuemin="180" aria-valuemax="420" aria-valuenow="220"></div></aside>
<button class="restore-player" id="restore-player" title="显示浏览工具栏" aria-label="显示浏览工具栏">›</button><main class="prototype-viewport" id="viewport"><div class="stage-holder" id="stage-holder">"#);
    for (index, page) in ordered_pages.iter().enumerate() {
        let [width, height] = page.size_for_output();
        write!(
            output,
            "<section class=\"rp-page\" id=\"page-{}\" aria-label=\"{}\" style=\"width:{}px;height:{}px;background:{}\"{}>",
            page.id,
            escape_html(&page.name),
            width,
            height,
            page.background,
            if index == 0 { "" } else { " hidden" }
        )?;
        let native_page = page_keys.get(&page.id).is_some_and(|key| {
            native.is_some_and(|preview| preview.scene.pages.iter().any(|p| &p.key == key))
        });
        if !native_page {
            for widget in &page.widgets {
                write_html_widget(
                    &mut output,
                    widget,
                    &format!("page-{}", page.id),
                    page.responsive_min_width().map(|_| page.width),
                )?;
            }
        }
        output.push_str("</section>");
    }
    output.push_str("</div></main><script type=\"application/json\" id=\"prototype-data\">");
    let mut state = json!({
        "name": project.name,
        // All source identifiers stay decimal strings, including directory references.
        "initialPage": ordered_pages[0].id.to_string(),
        "page_tree": player_page_tree(project),
        "pages": ordered_pages.iter().map(|page| {
            let size=page.size_for_output();
            json!({
                "id":page.id.to_string(),
                "key":page_keys.get(&page.id).cloned().unwrap_or_else(||format!("id-{}",page.id)),
                "name":page.name,"width":size[0],"height":size[1],
                "auto_width":page.canvas.auto_width(),"auto_height":page.canvas.auto_height(),
                "responsive_min_width":page.responsive_min_width()
            })
        }).collect::<Vec<_>>(),
        "interactions": project.pages.iter().flat_map(|page| &page.widgets).filter(|widget| widget.interaction.action != Action::None).map(|widget| json!({
            "id":widget.id.to_string(), "action":widget.interaction.action,
            "page":widget.interaction.target_page.map(|id| id.to_string()),
            "widget":widget.interaction.target_widget.map(|id| id.to_string()),
            "value":widget.interaction.value,
        })).collect::<Vec<_>>()
    });
    state["authored_events"] = json!(project.interaction_events.iter().map(|event| json!({
        "id":event.id,"name":event.name,"page":event.page.to_string(),
        "owner":event.owner.map(|id|id.to_string()),"trigger":event.trigger,
        "actions":event.actions.iter().map(|action|json!({"action":action.action,
            "page":action.target_page.map(|id|id.to_string()),"widget":action.target_widget.map(|id|id.to_string()),"value":action.value})).collect::<Vec<_>>()
    })).collect::<Vec<_>>());
    if let Some(native) = native {
        let mut native = serde_json::to_value(native)?;
        prepare_native_visuals(&mut native)?;
        state["native"] = native;
    }
    output.push_str(&escape_script_json(&serde_json::to_string(&state)?));
    output.push_str("</script><script>");
    if native.is_some() {
        output.push_str(include_str!("prototype.js"));
    }
    output.push_str(include_str!("player.js"));
    output.push_str("</script></body></html>");
    Ok(output)
}

fn prepare_native_visuals(value: &mut serde_json::Value) -> Result<()> {
    match value {
        serde_json::Value::Object(map) => {
            for (field, html_field) in [
                ("widget", "visual_html"),
                ("selected_style", "selected_html"),
                ("hover_style", "hover_html"),
                ("mouse_down_style", "mouse_down_html"),
            ] {
                if let Some(widget) = map.get(field).filter(|v| !v.is_null()) {
                    let mut widget: Widget = serde_json::from_value(widget.clone())?;
                    widget.x = 0.0;
                    widget.y = 0.0;
                    widget.rotation = 0.0;
                    widget.opacity = 1.0;
                    widget.hidden = false;
                    widget.interaction = Default::default();
                    let mut html = String::new();
                    write_html_widget(&mut html, &widget, "native", None)?;
                    // Runtime instances are scoped per frame/master. Numeric
                    // editor IDs must never become duplicate document IDs.
                    html = html.replacen(&format!(" id=\"w-{}\"", widget.id), "", 1);
                    map.insert(html_field.into(), html.into());
                    if let Some(serde_json::Value::Object(widget)) = map.get_mut(field) {
                        widget.remove("image_data");
                        widget.remove("vector_points");
                    }
                }
            }
            for value in map.values_mut() {
                prepare_native_visuals(value)?;
            }
        }
        serde_json::Value::Array(array) => {
            for value in array {
                prepare_native_visuals(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn player_page_order(project: &Project) -> Vec<&Page> {
    fn visit<'a>(
        nodes: &[crate::model::PageTreeNode],
        project: &'a Project,
        output: &mut Vec<&'a Page>,
    ) {
        for node in nodes {
            if let crate::model::PageTreeNode::Page { page_id, .. } = node {
                output.push(
                    project
                        .pages
                        .iter()
                        .find(|page| page.id == *page_id)
                        .expect("validated directory page"),
                );
            }
            visit(node.children(), project, output);
        }
    }
    if project.page_tree.is_empty() {
        project.pages.iter().collect()
    } else {
        let mut pages = Vec::new();
        visit(&project.page_tree, project, &mut pages);
        pages
    }
}

fn player_page_tree(project: &Project) -> Vec<serde_json::Value> {
    fn nodes(tree: &[crate::model::PageTreeNode], project: &Project) -> Vec<serde_json::Value> {
        tree.iter().map(|node| match node {
            crate::model::PageTreeNode::Folder { id, name, children } => json!({"type":"folder","key":format!("folder:{id}"),"name":name,"children":nodes(children,project)}),
            crate::model::PageTreeNode::Page { page_id, children } => {
                let page=project.pages.iter().find(|page| page.id==*page_id).expect("validated directory page");
                json!({"type":"page","key":format!("page:{page_id}"),"id":page_id.to_string(),"name":page.name,"children":nodes(children,project)})
            }
        }).collect()
    }
    if project.page_tree.is_empty() {
        project.pages.iter().map(|page| json!({"type":"page","key":format!("page:{}",page.id),"id":page.id.to_string(),"name":page.name,"children":[]})).collect()
    } else {
        nodes(&project.page_tree, project)
    }
}

fn write_html_widget(
    output: &mut String,
    widget: &Widget,
    scope: &str,
    responsive_width: Option<f32>,
) -> Result<()> {
    let class = kind_class(widget.kind);
    let radio_group = match widget.radio_group.as_deref() {
        Some("") => format!("widget-{}", widget.id),
        Some(group) => group.to_owned(),
        None => widget.name.clone(),
    };
    let vector = (widget.kind == WidgetKind::Rectangle)
        .then(|| crate::shape_geometry::svg_paths(widget))
        .flatten();
    let radius = if vector.is_some() {
        "0px".to_owned()
    } else if widget.kind == WidgetKind::Ellipse {
        "50%".to_owned()
    } else {
        format!("{}px", widget.radius)
    };
    let interactive = widget.interaction.action != Action::None;
    let fill = if widget.kind == WidgetKind::Line || vector.is_some() {
        "transparent"
    } else {
        &widget.fill
    };
    let layout_css = widget.text_layout.as_ref().map_or_else(String::new, |layout| {
        let horizontal = ["flex-start", "center", "flex-end"][usize::from(layout.horizontal)];
        let vertical = ["flex-start", "center", "flex-end"][usize::from(layout.vertical)];
        let text_align = ["left", "center", "right"][usize::from(layout.horizontal)];
        format!(";justify-content:{horizontal};align-items:{vertical};text-align:{text_align};padding:{}px {}px {}px {}px;font-family:Arial,Microsoft YaHei,sans-serif;font-weight:{};line-height:1.2;white-space:{};overflow:{}",
            layout.padding[1], layout.padding[2], layout.padding[3], layout.padding[0],
            if layout.bold {700} else {400}, if layout.wrap {"pre-wrap"} else {"pre"}, if layout.wrap {"hidden"} else {"visible"})
    });
    let (left, width) = if let Some(design_width) = responsive_width
        && widget.horizontal_layout != HorizontalLayout::LEFT
    {
        let start = f64::from(widget.horizontal_layout.left);
        let span = f64::from(widget.horizontal_layout.right) - start;
        let design_width = f64::from(design_width);
        // Each edge follows a share of the delta from the design width.
        let x_offset = f64::from(widget.x) - design_width * start / 100.0;
        let width_offset = f64::from(widget.width) - design_width * span / 100.0;
        (
            format!("calc({start}% + {x_offset}px)"),
            format!("max(1px, calc({span}% + {width_offset}px))"),
        )
    } else {
        (format!("{}px", widget.x), format!("{}px", widget.width))
    };
    write!(
        output,
        "<div id=\"w-{}\" class=\"widget {}{}\" data-interactive=\"{}\" aria-label=\"{}\" style=\"left:{left};top:{}px;width:{width};height:{}px;background:{};border:1px solid {};border-radius:{};color:{};font-size:{}px;opacity:{};transform:rotate({}deg);transform-origin:center center{}\"{}{}>",
        widget.id,
        class,
        if widget.kind == WidgetKind::Image && widget.image_data.is_none() {
            " empty"
        } else {
            ""
        },
        interactive,
        escape_html(&widget.name),
        widget.y,
        widget.height,
        fill,
        if vector.is_some() {
            "transparent"
        } else {
            &widget.stroke
        },
        radius,
        widget.text_color,
        widget.font_size,
        widget.opacity,
        widget.rotation,
        layout_css,
        if widget.hidden { " hidden" } else { "" },
        if interactive {
            " role=\"button\" tabindex=\"0\""
        } else {
            ""
        }
    )?;
    if let Some(paths) = vector {
        write!(
            output,
            "<svg class=\"vector-shape\" xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" preserveAspectRatio=\"none\" aria-hidden=\"true\">",
            widget.width, widget.height
        )?;
        for path in paths {
            write!(
                output,
                "<path d=\"{}\" fill=\"{}\" fill-rule=\"evenodd\" stroke=\"{}\"/>",
                escape_html(&path),
                widget.fill,
                widget.stroke
            )?;
        }
        output.push_str("</svg>");
    }
    match widget.kind {
        WidgetKind::Input => write!(
            output,
            "<input type=\"text\" value=\"{}\" aria-label=\"{}\">",
            escape_html(&widget.text),
            escape_html(&widget.name)
        )?,
        WidgetKind::Checkbox => write!(
            output,
            "<label><input type=\"checkbox\"><span class=\"text-value\">{}</span></label>",
            escape_html(&widget.text)
        )?,
        WidgetKind::Radio => write!(
            output,
            "<label><input type=\"radio\" name=\"{}:{}\" data-radio-group=\"{}\"><span class=\"text-value\">{}</span></label>",
            escape_html(scope),
            escape_html(&radio_group),
            escape_html(&radio_group),
            escape_html(&widget.text)
        )?,
        WidgetKind::TextArea => write!(
            output,
            "<textarea aria-label=\"{}\">\n{}</textarea>",
            escape_html(&widget.name),
            escape_html(&widget.text)
        )?,
        WidgetKind::Droplist | WidgetKind::ListBox => {
            write!(
                output,
                "<select aria-label=\"{}\"{}>",
                escape_html(&widget.name),
                if widget.kind == WidgetKind::ListBox {
                    " size=\"5\""
                } else {
                    ""
                }
            )?;
            for option in widget.text.split('\n') {
                let option = escape_html(option.trim_end_matches('\r'));
                write!(output, "<option value=\"{option}\">{option}</option>")?;
            }
            output.push_str("</select>");
        }
        WidgetKind::Image => {
            if let Some(data) = &widget.image_data {
                write!(
                    output,
                    "<img src=\"{}\" alt=\"{}\" draggable=\"false\">",
                    escape_html(data),
                    escape_html(&widget.text)
                )?;
            } else {
                write!(
                    output,
                    "<span class=\"text-value\">{}</span>",
                    escape_html(&widget.text)
                )?;
            }
        }
        WidgetKind::Line => {
            let [a, b] = crate::vector_edit::line_endpoints(widget);
            write!(
                output,
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100%\" height=\"100%\" viewBox=\"0 0 {} {}\" preserveAspectRatio=\"none\" aria-hidden=\"true\" style=\"overflow:visible;pointer-events:none\"><line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/></svg>",
                widget.width,
                widget.height,
                a[0] - widget.x,
                a[1] - widget.y,
                b[0] - widget.x,
                b[1] - widget.y,
                widget.fill,
                widget.height.clamp(1.0, 4.0)
            )?;
        }
        _ => write!(
            output,
            "<span class=\"text-value\">{}</span>",
            escape_html(&widget.text)
        )?,
    }
    output.push_str("</div>");
    Ok(())
}

fn kind_class(kind: WidgetKind) -> &'static str {
    match kind {
        WidgetKind::Rectangle => "k-rectangle",
        WidgetKind::Ellipse => "k-ellipse",
        WidgetKind::Text => "k-text",
        WidgetKind::Button => "k-button",
        WidgetKind::Input => "k-input",
        WidgetKind::Checkbox => "k-checkbox",
        WidgetKind::Radio => "k-radio",
        WidgetKind::TextArea => "k-textarea",
        WidgetKind::Droplist => "k-droplist",
        WidgetKind::ListBox => "k-listbox",
        WidgetKind::Line => "k-line",
        WidgetKind::Image => "k-image",
        WidgetKind::Panel => "k-panel",
    }
}

/// Export one page as a portable SVG with embedded raster images and editable text.
pub fn svg(page: &Page) -> Result<String> {
    validate_page(page)?;
    let [width, height] = page.size_for_output();
    let mut output = String::with_capacity(8_192);
    write!(
        output,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" role=\"img\"><title>{}</title><defs><clipPath id=\"page-clip\"><rect width=\"{}\" height=\"{}\"/></clipPath></defs><rect width=\"100%\" height=\"100%\" fill=\"{}\"/><g clip-path=\"url(#page-clip)\">",
        width,
        height,
        width,
        height,
        escape_html(&page.name),
        width,
        height,
        page.background
    )?;
    for widget in &page.widgets {
        if widget.hidden {
            continue;
        }
        write!(
            output,
            "<g id=\"w-{}\" opacity=\"{}\" transform=\"rotate({} {} {})\"><title>{}</title>",
            widget.id,
            widget.opacity,
            widget.rotation,
            widget.x + widget.width / 2.0,
            widget.y + widget.height / 2.0,
            escape_html(&widget.name)
        )?;
        let vector = (widget.kind == WidgetKind::Rectangle)
            .then(|| crate::shape_geometry::svg_paths(widget))
            .flatten();
        match widget.kind {
            WidgetKind::Rectangle if vector.is_some() => {
                for path in vector.as_ref().unwrap() {
                    write!(
                        output,
                        "<path transform=\"translate({} {})\" d=\"{}\" fill=\"{}\" fill-rule=\"evenodd\" stroke=\"{}\"/>",
                        widget.x,
                        widget.y,
                        escape_html(path),
                        widget.fill,
                        widget.stroke
                    )?;
                }
            }
            WidgetKind::Ellipse => write!(
                output,
                "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"{}\" stroke=\"{}\"/>",
                widget.x + widget.width / 2.0,
                widget.y + widget.height / 2.0,
                widget.width / 2.0,
                widget.height / 2.0,
                widget.fill,
                widget.stroke
            )?,
            WidgetKind::Line => {
                let [a, b] = crate::vector_edit::line_endpoints(widget);
                write!(
                    output,
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/>",
                    a[0],
                    a[1],
                    b[0],
                    b[1],
                    widget.fill,
                    widget.height.clamp(1.0, 4.0)
                )?;
            }
            WidgetKind::Text | WidgetKind::Checkbox | WidgetKind::Radio => {}
            _ => {
                write!(
                    output,
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{}\" stroke=\"{}\"/>",
                    widget.x,
                    widget.y,
                    widget.width,
                    widget.height,
                    widget.radius.min(widget.width.min(widget.height) / 2.0),
                    widget.fill,
                    widget.stroke
                )?;
            }
        }
        if widget.kind == WidgetKind::Image {
            if let Some(data) = &widget.image_data {
                write!(
                    output,
                    "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"xMidYMid meet\" xlink:href=\"{}\"/>",
                    widget.x,
                    widget.y,
                    widget.width,
                    widget.height,
                    escape_html(data)
                )?;
            } else {
                svg_text(&mut output, widget, false)?;
            }
        } else if matches!(widget.kind, WidgetKind::Checkbox | WidgetKind::Radio) {
            let size = 17.0_f32.min(widget.height);
            if widget.kind == WidgetKind::Radio {
                write!(
                    output,
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#FFFFFF\" stroke=\"#B7C0CE\"/>",
                    widget.x + size / 2.0,
                    widget.y + widget.height / 2.0,
                    size / 2.0
                )?;
            } else {
                write!(
                    output,
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"3\" fill=\"#FFFFFF\" stroke=\"#B7C0CE\"/>",
                    widget.x,
                    widget.y + (widget.height - size) / 2.0,
                    size,
                    size
                )?;
            }
            svg_text(&mut output, widget, true)?;
        } else if matches!(
            widget.kind,
            WidgetKind::TextArea | WidgetKind::Droplist | WidgetKind::ListBox
        ) {
            let mut label = widget.clone();
            label.text_layout = Some(crate::model::TextLayout {
                horizontal: 0,
                vertical: if widget.kind == WidgetKind::Droplist {
                    1
                } else {
                    0
                },
                padding: [8.0, 8.0, 24.0, 8.0],
                wrap: true,
                bold: widget
                    .text_layout
                    .as_ref()
                    .is_some_and(|layout| layout.bold),
            });
            if widget.kind == WidgetKind::Droplist {
                label.text = widget.text.lines().next().unwrap_or_default().to_owned();
                let x = widget.x + widget.width - 13.0;
                let y = widget.y + widget.height / 2.0;
                write!(
                    output,
                    "<path d=\"M {} {} l 4 4 l 4 -4\" fill=\"none\" stroke=\"{}\"/>",
                    x - 4.0,
                    y - 2.0,
                    widget.text_color
                )?;
            }
            write!(
                output,
                "<defs><clipPath id=\"form-{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath></defs><g clip-path=\"url(#form-{})\">",
                widget.id,
                widget.x + 1.0,
                widget.y + 1.0,
                (widget.width - 2.0).max(0.0),
                (widget.height - 2.0).max(0.0),
                widget.id
            )?;
            svg_text(&mut output, &label, false)?;
            output.push_str("</g>");
        } else if widget.kind != WidgetKind::Line {
            svg_text(&mut output, widget, false)?;
        }
        output.push_str("</g>");
    }
    output.push_str("</g></svg>");
    Ok(output)
}

fn svg_text(output: &mut String, widget: &Widget, checkbox: bool) -> Result<()> {
    if widget.text.is_empty() {
        return Ok(());
    }
    if let Some(layout) = &widget.text_layout {
        let [left, top, right, bottom] = layout.padding;
        let inner_width = (widget.width - left - right).max(1.0);
        let inner_height = (widget.height - top - bottom).max(1.0);
        let x = widget.x
            + left
            + match layout.horizontal {
                1 => inner_width * 0.5,
                2 => inner_width,
                _ => 0.0,
            };
        let lines = widget.text.lines().collect::<Vec<_>>();
        let line_height = widget.font_size * 1.2;
        let extra = lines.len().saturating_sub(1) as f32 * line_height;
        let (y, baseline) = match layout.vertical {
            1 => (widget.y + top + inner_height * 0.5 - extra * 0.5, "central"),
            2 => (widget.y + widget.height - bottom - extra, "text-after-edge"),
            _ => (widget.y + top, "text-before-edge"),
        };
        let anchor = ["start", "middle", "end"][usize::from(layout.horizontal)];
        write!(
            output,
            "<text fill=\"{}\" font-family=\"Arial,Microsoft YaHei,sans-serif\" font-size=\"{}\" font-weight=\"{}\" text-anchor=\"{}\" dominant-baseline=\"{}\">",
            widget.text_color,
            widget.font_size,
            if layout.bold { 700 } else { 400 },
            anchor,
            baseline
        )?;
        for (index, line) in lines.iter().enumerate() {
            write!(
                output,
                "<tspan x=\"{}\" y=\"{}\">{}</tspan>",
                x,
                y + index as f32 * line_height,
                escape_html(line)
            )?;
        }
        output.push_str("</text>");
        return Ok(());
    }
    let left_aligned = matches!(
        widget.kind,
        WidgetKind::Text | WidgetKind::Input | WidgetKind::Checkbox | WidgetKind::Radio
    );
    let padding = if checkbox {
        26.0
    } else if widget.kind == WidgetKind::Input {
        12.0
    } else {
        0.0
    };
    let x = if left_aligned {
        widget.x + padding
    } else {
        widget.x + widget.width / 2.0
    };
    let anchor = if left_aligned { "start" } else { "middle" };
    let lines: Vec<&str> = widget.text.lines().collect();
    let line_height = widget.font_size * 1.35;
    let first_y =
        widget.y + widget.height / 2.0 - (lines.len().saturating_sub(1) as f32 * line_height) / 2.0;
    write!(
        output,
        "<text fill=\"{}\" font-family=\"Inter,Segoe UI,Microsoft YaHei,Arial,sans-serif\" font-size=\"{}\" text-anchor=\"{}\" dominant-baseline=\"central\">",
        widget.text_color, widget.font_size, anchor
    )?;
    for (index, line) in lines.iter().enumerate() {
        write!(
            output,
            "<tspan x=\"{}\" y=\"{}\">{}</tspan>",
            x,
            first_y + index as f32 * line_height,
            escape_html(line)
        )?;
    }
    output.push_str("</text>");
    Ok(())
}

fn escape_html(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            // XML 1.0 cannot represent these controls, even as numeric references.
            '\0'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}' => {
                output.push('\u{fffd}')
            }
            _ => output.push(character),
        }
    }
    output
}

fn escape_script_json(value: &str) -> String {
    value
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

#[cfg(test)]
mod tests;
