use super::*;

impl Converter<'_> {
    pub(super) fn allocate(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
    pub(super) fn warn(&mut self, message: &'static str) {
        *self.warnings.entry(message).or_default() += 1;
    }

    pub(super) fn page(&mut self, package: &str, name: &str) -> Result<Page> {
        let root = self
            .roots
            .get(package)
            .and_then(|id| self.graph.node(id))
            .with_context(|| format!("页面「{name}」缺少原生对象包"))?;
        let mut page = Page {
            id: self.allocate(),
            name: truncate(name, 1024),
            width: 64.0,
            height: 64.0,
            canvas: CanvasMode::Auto,
            background: "#FFFFFF".into(),
            widgets: Vec::new(),
            group_names: BTreeMap::new(),
        };
        let properties = self.styles.for_page(self.graph, root);
        let size = properties.get(&105).unwrap_or(&Value::Null);
        let width = number(&size["Width"]).filter(|width| *width > 0.0);
        let height = number(&size["Height"]).filter(|height| *height > 0.0);
        page.canvas = match (width, height) {
            (Some(_), Some(_)) => CanvasMode::Fixed,
            (Some(_), None) => CanvasMode::FixedWidth,
            (None, Some(_)) => CanvasMode::FixedHeight,
            (None, None) => CanvasMode::Auto,
        };
        if let Some(width) = width {
            page.width = finite(width, 64.0, 20_000.0);
        }
        if let Some(height) = height {
            page.height = finite(height, 64.0, 20_000.0);
        }
        if let Some(background) = properties.get(&7).and_then(fill_color) {
            page.background = background;
        }
        if root.properties["AdaptiveStyles"]
            .as_array()
            .is_some_and(|s| !s.is_empty())
        {
            self.warn("自适应视图已采用默认视图布局");
        }
        let diagram = root
            .properties
            .get("diagram")
            .and_then(reference_id)
            .with_context(|| format!("页面「{name}」没有画布"))?;
        self.diagram(
            diagram,
            (0.0, 0.0),
            false,
            &mut page.widgets,
            &mut BTreeSet::new(),
            0,
        )?;
        for id in page
            .widgets
            .iter()
            .flat_map(|w| &w.groups)
            .collect::<BTreeSet<_>>()
        {
            let binding = &self.group_bindings[id];
            if let Some(name) = self
                .graph
                .node(&binding.object)
                .and_then(|node| node.properties.pointer("/name-block/text"))
                .and_then(Value::as_str)
                .map(|name| truncate(name, 1024))
                .filter(|name| !name.trim().is_empty())
            {
                page.group_names.insert(*id, name);
            }
        }
        for widget in &page.widgets {
            let bounds = crate::manipulation::rotated_bounds(widget);
            if page.canvas.auto_width() {
                page.width = page.width.max(bounds[2]).min(20_000.0);
            }
            if page.canvas.auto_height() {
                page.height = page.height.max(bounds[3]).min(20_000.0);
            }
        }
        Ok(page)
    }

    pub(super) fn library_pages(
        &mut self,
        order: &[(String, String)],
        project: &mut Project,
        page_ids: &mut BTreeMap<String, u64>,
    ) -> Result<()> {
        ensure!(order.len() <= 24 * 128, "元件库超过 3072 个组件的导入上限");
        for (gallery_index, components) in order.chunks(24).enumerate() {
            let mut gallery = Page {
                id: self.allocate(),
                name: format!("组件库 {}", gallery_index + 1),
                width: 1600.0,
                height: 2160.0,
                canvas: CanvasMode::Fixed,
                background: "#F5F7FA".into(),
                widgets: Vec::new(),
                group_names: BTreeMap::new(),
            };
            for (index, (package, name)) in components.iter().enumerate() {
                let page = self.page(package, name)?;
                page_ids.insert(package.clone(), gallery.id);
                gallery.group_names.extend(page.group_names);
                let cell_x = (index % 4) as f32 * 400.0;
                let cell_y = (index / 4) as f32 * 360.0;
                let mut label = Widget::new(
                    self.allocate(),
                    WidgetKind::Text,
                    cell_x + 20.0,
                    cell_y + 12.0,
                );
                label.text = name.clone();
                label.name = truncate(name, 1024);
                label.width = 360.0;
                label.height = 44.0;
                label.font_size = 14.0;
                label.locked = true;
                gallery.widgets.push(label);
                if !page.widgets.is_empty() {
                    let min_x = page
                        .widgets
                        .iter()
                        .map(|w| w.x)
                        .fold(f32::INFINITY, f32::min);
                    let min_y = page
                        .widgets
                        .iter()
                        .map(|w| w.y)
                        .fold(f32::INFINITY, f32::min);
                    let max_x = page
                        .widgets
                        .iter()
                        .map(|w| w.x + w.width)
                        .fold(f32::NEG_INFINITY, f32::max);
                    let max_y = page
                        .widgets
                        .iter()
                        .map(|w| w.y + w.height)
                        .fold(f32::NEG_INFINITY, f32::max);
                    let scale = (360.0 / (max_x - min_x).max(1.0))
                        .min(280.0 / (max_y - min_y).max(1.0))
                        .min(1.0);
                    for mut widget in page.widgets {
                        if let Some(binding) = self.bindings.get_mut(&widget.id) {
                            binding.scale = scale;
                        }
                        widget.x = cell_x + 20.0 + (widget.x - min_x) * scale;
                        widget.y = cell_y + 64.0 + (widget.y - min_y) * scale;
                        widget.width = (widget.width * scale).max(1.0);
                        widget.height = (widget.height * scale).max(1.0);
                        widget.font_size = (widget.font_size * scale).max(4.0);
                        if let Some(layout) = &mut widget.text_layout {
                            layout.padding = layout.padding.map(|value| value * scale);
                        }
                        widget.radius *= scale;
                        widget.name = truncate(&format!("{name} / {}", widget.name), 1024);
                        gallery.widgets.push(widget);
                    }
                }
            }
            project.pages.push(gallery);
        }
        Ok(())
    }

    pub(super) fn diagram(
        &mut self,
        id: &str,
        offset: (f32, f32),
        hidden: bool,
        output: &mut Vec<Widget>,
        ancestors: &mut BTreeSet<String>,
        depth: usize,
    ) -> Result<()> {
        ensure!(depth <= 64, "原生元件嵌套超过 64 层");
        ensure!(ancestors.insert(id.to_owned()), "原生画布包含循环引用");
        let node = self
            .graph
            .node(id)
            .with_context(|| format!("原生画布引用 {id} 不存在"))?;
        let references =
            direct_diagram_references(self.graph, array(&node.properties["diagram-objects"]))?;
        for reference in &references {
            if let Some(id) = reference_id(reference) {
                self.widget(id, offset, hidden, output, ancestors, depth + 1)?;
            }
        }
        ancestors.remove(id);
        Ok(())
    }

    pub(super) fn widget(
        &mut self,
        id: &str,
        offset: (f32, f32),
        parent_hidden: bool,
        output: &mut Vec<Widget>,
        ancestors: &mut BTreeSet<String>,
        depth: usize,
    ) -> Result<()> {
        ensure!(depth <= 64, "原生元件嵌套超过 64 层");
        ensure!(output.len() < 5000, "原生单页超过 5000 个可导入元件");
        ensure!(ancestors.insert(id.to_owned()), "原生元件包含循环引用");
        let node = self
            .graph
            .node(id)
            .with_context(|| format!("原生元件引用 {id} 不存在"))?;
        let p = &node.properties;
        let hidden = parent_hidden || p["VisibleFromSettings"].as_bool() == Some(false);
        let style = self.styles.for_node(self.graph, node);
        let location = style
            .get(&101)
            .or_else(|| p.get("ContainedLocation"))
            .unwrap_or(&Value::Null);
        let x = number(&location["X"]).unwrap_or(0.0) + offset.0;
        let y = number(&location["Y"]).unwrap_or(0.0) + offset.1;
        if node.kind.ends_with(":Layer") {
            let group = self.allocate();
            self.group_bindings.insert(
                group,
                native::GroupBinding {
                    object: id.to_owned(),
                },
            );
            let start = output.len();
            for reference in array(&p["DiagramObjects"]) {
                if let Some(child) = reference_id(reference) {
                    // Native groups keep child coordinates in the parent diagram.
                    self.widget(child, offset, hidden, output, ancestors, depth + 1)?;
                }
            }
            for widget in &mut output[start..] {
                widget.groups.insert(0, group);
            }
            ancestors.remove(id);
            return Ok(());
        }
        if node.kind.ends_with(":Table") || node.kind.ends_with(":MenuObject") {
            self.warn("表格或菜单已展开为独立单元格");
            for reference in array(&p["diagram-objects"]) {
                if let Some(child) = reference_id(reference) {
                    self.widget(child, (x, y), hidden, output, ancestors, depth + 1)?;
                }
            }
            ancestors.remove(id);
            return Ok(());
        }
        let widget = self.visual_widget(id, node, &style, (x, y), hidden)?;
        let instance_id = widget.id;
        let instance_rotation = widget.rotation;
        let instance_center = (
            widget.x + widget.width * 0.5,
            widget.y + widget.height * 0.5,
        );
        output.push(widget);
        if node.kind.ends_with(":ReferenceDiagramObject") {
            let master = p
                .get("ReferredMaster")
                .and_then(reference_id)
                .and_then(|package| self.roots.get(package))
                .and_then(|root| self.graph.node(root));
            if let Some(diagram) = master
                .and_then(|root| root.properties.get("diagram"))
                .and_then(reference_id)
            {
                let start = output.len();
                self.diagram(diagram, (x, y), hidden, output, ancestors, depth + 1)?;
                for child in &mut output[start..] {
                    self.bindings.remove(&child.id);
                    self.preview_parents.insert(child.id, instance_id);
                    child.locked = true;
                    if instance_rotation.abs() > 0.001 {
                        let radians = instance_rotation.to_radians();
                        let dx = child.x + child.width * 0.5 - instance_center.0;
                        let dy = child.y + child.height * 0.5 - instance_center.1;
                        child.x = instance_center.0 + dx * radians.cos()
                            - dy * radians.sin()
                            - child.width * 0.5;
                        child.y = instance_center.1 + dx * radians.sin() + dy * radians.cos()
                            - child.height * 0.5;
                        child.rotation += instance_rotation;
                    }
                }
                self.warn("母版内容已显示为只读预览；可编辑实例本身，母版内元件暂不支持单独编辑");
            } else {
                self.warn("无法读取的母版实例已保留为占位容器");
            }
        } else if node.kind.ends_with(":DynamicPanel") {
            self.warn("编辑画布显示动态面板首状态；原生预览保留所有状态与裁剪");
            if let Some(package) = array(&p["panel-diagrams"])
                .first()
                .and_then(|v| reference_id(v))
            {
                let root_id = self
                    .roots
                    .get(package)
                    .map(String::as_str)
                    .unwrap_or(package);
                let state = self
                    .graph
                    .node(root_id)
                    .with_context(|| "动态面板首状态对象不存在")?;
                let diagram = state
                    .properties
                    .get("diagram")
                    .and_then(reference_id)
                    .unwrap_or(root_id);
                self.diagram(diagram, (x, y), hidden, output, ancestors, depth + 1)?;
            }
        } else if node.kind.ends_with(":Repeater") {
            self.warn("Repeater 已展开为一份模板，数据、筛选与重复逻辑未保留");
            if let Some(diagram) = p.get("BaseDiagram").and_then(reference_id) {
                self.diagram(diagram, (x, y), hidden, output, ancestors, depth + 1)?;
            }
        }
        ancestors.remove(id);
        Ok(())
    }

    pub(super) fn visual_widget(
        &mut self,
        id: &str,
        node: &Node,
        style: &Properties,
        position: (f32, f32),
        hidden: bool,
    ) -> Result<Widget> {
        let p = &node.properties;
        let (x, y) = position;
        let shape = p["Shape"].as_i64().unwrap_or(0);
        let kind = match node.kind.rsplit(':').next().unwrap_or("") {
            "VectorShape" => match shape {
                15 => WidgetKind::Ellipse,
                29 | 30 => WidgetKind::Line,
                _ => WidgetKind::Rectangle,
            },
            "ImageBox" | "Screenshot" => WidgetKind::Image,
            "TextBox" => WidgetKind::Input,
            "TextArea" => WidgetKind::TextArea,
            "ComboBox" => WidgetKind::Droplist,
            "ListBox" => WidgetKind::ListBox,
            "Checkbox" => WidgetKind::Checkbox,
            "RadioButton" => WidgetKind::Radio,
            "HorizontalLine" | "VerticalLine" | "Connector" => WidgetKind::Line,
            "DynamicPanel" | "Repeater" | "InlineFrame" | "ImageMapRegion" => WidgetKind::Panel,
            "Hyperlink" | "TreeNodeObject" | "RichTextPanel" => WidgetKind::Text,
            "TableCell" => WidgetKind::Rectangle,
            _ => {
                self.warn("部分元件类型已转换为基础容器");
                WidgetKind::Panel
            }
        };
        let mut widget = Widget::new(
            self.allocate(),
            kind,
            finite(x, -100_000.0, 100_000.0),
            finite(y, -100_000.0, 100_000.0),
        );
        if node.kind.ends_with(":VectorShape")
            && let Some(points) = p.get("ControlPoints")
            && crate::vector_edit::validate_points(points).is_ok()
        {
            widget.vector_points = Some(points.clone());
        }
        let size = style
            .get(&105)
            .or_else(|| p.get("ContainedSize"))
            .unwrap_or(&Value::Null);
        widget.width = finite(
            number(&size["Width"]).unwrap_or(widget.width),
            1.0,
            20_000.0,
        );
        widget.height = finite(
            number(&size["Height"]).unwrap_or(widget.height),
            1.0,
            20_000.0,
        );
        let text_properties = text_properties(p, self.graph);
        widget.text = truncate(&widget_text(text_properties), 100_000);
        if kind.is_option_list() {
            widget.text = if let Some(options) = list_options_text(p, self.graph) {
                truncate(&options, 100_000)
            } else {
                self.warn("部分列表选项结构尚未识别，原生选项已保留");
                String::new()
            };
        }
        widget.name = truncate(
            p.pointer("/name-block/text")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| {
                    if widget.text.trim().is_empty() {
                        kind.label()
                    } else {
                        &widget.text
                    }
                }),
            1024,
        );
        widget.fill = style.get(&7).and_then(fill_color).unwrap_or_else(|| {
            if matches!(
                kind,
                WidgetKind::Text
                    | WidgetKind::Line
                    | WidgetKind::Checkbox
                    | WidgetKind::Radio
                    | WidgetKind::Panel
            ) {
                "#FFFFFF00".into()
            } else {
                "#FFFFFF".into()
            }
        });
        widget.stroke = style
            .get(&8)
            .and_then(fill_color)
            .unwrap_or_else(|| "#999999".into());
        let border_width = style.get(&15).and_then(number).unwrap_or(1.0);
        if border_width <= 0.0 {
            widget.stroke = "#00000000".into();
        }
        widget.text_color = style
            .get(&6)
            .and_then(fill_color)
            .unwrap_or_else(|| "#333333".into());
        widget.font_size = finite(
            style.get(&2).and_then(number).unwrap_or(9.75) * 4.0 / 3.0,
            4.0,
            512.0,
        );
        widget.radius = finite(
            style.get(&71).and_then(number).unwrap_or(0.0),
            0.0,
            20_000.0,
        );
        widget.opacity = finite(style.get(&70).and_then(number).unwrap_or(1.0), 0.0, 1.0);
        widget.rotation = finite(
            style
                .get(&ROTATION_PROPERTY)
                .and_then(number)
                .unwrap_or(0.0),
            -36_000.0,
            36_000.0,
        );
        widget.hidden = hidden;
        widget.locked = p["IsLocked"].as_bool().unwrap_or(false);
        if let Some(inline) = first_inline(text_properties) {
            if let Some(size) = inline.get("Size").and_then(number) {
                widget.font_size = finite(size * 4.0 / 3.0, 4.0, 512.0);
            }
            if let Some(color) = inline.get("Color").and_then(argb_color) {
                widget.text_color = color;
            }
        }
        widget.text_layout = Some(TextLayout {
            horizontal: style.get(&9).and_then(Value::as_u64).unwrap_or(1).min(2) as u8,
            vertical: style.get(&10).and_then(Value::as_u64).unwrap_or(1).min(2) as u8,
            padding: [11, 12, 13, 14].map(|key| {
                finite(
                    style.get(&key).and_then(number).unwrap_or(0.0),
                    0.0,
                    20_000.0,
                )
            }),
            wrap: !text_properties["AutoFitWidth"].as_bool().unwrap_or(false)
                && !p["AutoFitWidth"].as_bool().unwrap_or(false),
            bold: first_inline(text_properties)
                .and_then(|inline| inline.get("Weight"))
                .and_then(number)
                .is_some_and(|weight| weight >= 600.0),
        });
        if kind == WidgetKind::Radio {
            widget.radio_group = Some(truncate(p["radio-group"].as_str().unwrap_or(""), 1024));
        }
        if node.kind.ends_with(":InlineFrame") {
            self.warn("编辑画布以内联框架占位；原生预览可加载本工程页面");
        }
        if kind == WidgetKind::Image {
            let image = style.get(&27).or_else(|| style.get(&35));
            if let Some(image) = image {
                widget.image_data = embedded_image(image, self.graph);
            }
            if widget.image_data.is_none() {
                self.warn("图片源不可用或格式不受支持，已保留图片占位");
            }
        }
        if node.kind.ends_with(":VectorShape") && !matches!(shape, 0 | 15 | 29 | 30) {
            if let Some(data) = vector::rasterize(
                p,
                widget.width,
                widget.height,
                &widget.fill,
                &widget.stroke,
                border_width,
            ) {
                widget.kind = WidgetKind::Image;
                widget.image_data = Some(data);
                widget.fill = "#FFFFFF00".into();
                widget.stroke = "#FFFFFF00".into();
                self.warn("自定义矢量路径已转换为可移动缩放的图片");
            } else {
                self.warn("无法转换的矢量路径已用基础形状占位");
            }
        }
        if kind == WidgetKind::Line {
            widget.fill = style
                .get(&8)
                .and_then(fill_color)
                .unwrap_or_else(|| "#999999".into());
            widget.stroke = "#FFFFFF00".into();
        }
        if let Some(package) = self.navigation(p) {
            self.pending_links.push((widget.id, package));
        }
        self.bindings.insert(
            widget.id,
            native::Binding {
                object: id.to_owned(),
                scale: 1.0,
            },
        );
        Ok(widget)
    }

    pub(super) fn navigation(&mut self, properties: &Value) -> Option<String> {
        let map = properties
            .get("InteractionMap")
            .and_then(|v| self.graph.resolve(v))?;
        let table = map.properties["node-table"].as_object()?;
        let mut authored_nodes = BTreeSet::new();
        for (key, value) in table {
            if reference_id(value).is_some_and(native_events::is_authored_case) {
                authored_nodes.insert(key.as_str());
                if let Some(node) = self.graph.node(key) {
                    authored_nodes.extend(
                        array(&node.properties["children"])
                            .iter()
                            .filter_map(reference_id),
                    );
                }
            }
        }
        let entries = table
            .iter()
            .filter(|(key, _)| !authored_nodes.contains(key.as_str()))
            .map(|(_, value)| value)
            .filter_map(|v| self.graph.resolve(v))
            .collect::<Vec<_>>();
        let events = entries
            .iter()
            .filter(|n| n.kind == "Axure:InteractionEvent")
            .collect::<Vec<_>>();
        let cases = entries
            .iter()
            .filter(|n| n.kind == "Axure:InteractionCase")
            .collect::<Vec<_>>();
        if cases.is_empty() {
            return None;
        }
        let actions = entries
            .iter()
            .filter(|n| n.kind != "Axure:InteractionEvent" && n.kind != "Axure:InteractionCase")
            .collect::<Vec<_>>();
        let supported = events.len() == 1
            && events[0].properties["event-type"].as_i64() == Some(16)
            && events[0].properties["Disabled"].as_bool() != Some(true)
            && cases.len() == 1
            && cases[0].properties["Disabled"].as_bool() != Some(true)
            && cases[0]
                .properties
                .get("Condition")
                .and_then(reference_id)
                .is_none_or(|id| id == EMPTY_ID)
            && actions.len() == 1
            && actions[0].kind == "Axure:InteractionLinkWindowAction"
            && actions[0].properties["Disabled"].as_bool() != Some(true)
            && actions[0].properties["link-type"].as_i64() == Some(1);
        if supported
            && let Some(target) = self.graph.resolve(&actions[0].properties["target"])
            && target.properties["target-type"].as_i64() == Some(1)
        {
            return target
                .properties
                .get("target-page-id")
                .and_then(reference_id)
                .map(str::to_owned);
        }
        if !entries.is_empty() {
            self.warn("编辑画布仅提供简单跳转配置；原有多动作在原生预览按支持范围执行");
        }
        None
    }
}
