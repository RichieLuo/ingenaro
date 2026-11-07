use super::*;

impl NativeDocument {
    pub(super) fn patch_project(
        &self,
        project: &Project,
        parts: &mut BTreeMap<u64, Graph>,
    ) -> Result<()> {
        ensure!(
            project.page_tree == self.baseline.page_tree,
            "原生保存暂不支持修改页面文件夹结构；请保留目录结构后保存"
        );
        ensure!(
            project.pages.iter().map(|p| p.id).collect::<Vec<_>>()
                == self.baseline.pages.iter().map(|p| p.id).collect::<Vec<_>>(),
            "原生保存暂不支持新增、删除或重排页面；请保留页面结构后保存"
        );
        ensure!(
            project.name == self.baseline.name,
            "原生项目名称来自文件名，请使用另存为更改文件名"
        );
        ensure!(
            project.pages.iter().all(|page| {
                page.canvas != crate::model::CanvasMode::ResponsiveWidth
                    && page.responsive_min_width.is_none()
                    && page.widgets.iter().all(|widget| {
                        widget.horizontal_layout == crate::model::HorizontalLayout::LEFT
                    })
            }),
            "响应式布局设置仅支持 .ingenaro 工程"
        );
        let owners = owners(parts);
        let previous = self
            .baseline
            .pages
            .iter()
            .flat_map(|p| p.widgets.iter().map(move |w| (w.id, (p.id, w))))
            .collect::<BTreeMap<_, _>>();
        let current = project
            .pages
            .iter()
            .flat_map(|p| p.widgets.iter().map(move |w| (w.id, (p.id, w))))
            .collect::<BTreeMap<_, _>>();
        let mut edited_bindings = self.bindings.clone();
        for id in previous.keys().filter(|id| !self.bindings.contains_key(id)) {
            ensure!(
                current.contains_key(id)
                    || self
                        .preview_parents
                        .get(id)
                        .is_some_and(|parent| !current.contains_key(parent)),
                "只读母版预览和元件库名称标签暂不支持单独删除"
            );
        }
        for page in &project.pages {
            let old = self
                .baseline
                .pages
                .iter()
                .find(|p| p.id == page.id)
                .context("原生页面绑定丢失")?;
            if self.library {
                ensure!(
                    page.name == old.name
                        && page.width == old.width
                        && page.height == old.height
                        && page.canvas == old.canvas
                        && page.background == old.background,
                    "元件库展示页是浏览布局，暂不支持修改展示页名称、尺寸或背景；组件本身仍可编辑保存"
                );
            } else {
                ensure!(
                    page.width == old.width
                        && page.height == old.height
                        && page.canvas == old.canvas,
                    "原生保存暂不支持修改页面尺寸或 Auto 模式"
                );
                let package = self.pages.get(&page.id).context("原生页面绑定丢失")?;
                if page.name != old.name {
                    rename_page(parts, package, &page.name)?;
                }
                if page.background != old.background {
                    set_page_background(parts, package, &page.background)?;
                }
            }
        }
        for (id, binding) in &self.bindings {
            let Some((old_page, old)) = previous.get(id) else {
                continue;
            };
            if let Some((page, widget)) = current.get(id) {
                ensure!(
                    page == old_page,
                    "原生保存暂不支持跨页面移动现有元件，请在目标页面复制创建"
                );
                if *widget != *old {
                    self.patch_widget(parts, &owners, binding, old, widget)?;
                }
            } else {
                let source = get_node(parts, &owners, &binding.object)?;
                ensure!(
                    !source.kind.ends_with(":DynamicPanel") && !source.kind.ends_with(":Repeater"),
                    "动态面板与 Repeater 在编辑器中已展开，暂不支持原生删除其外层容器"
                );
                ensure!(
                    !parts
                        .get(owners.get(&binding.object).unwrap())
                        .unwrap()
                        .objects
                        .values()
                        .any(|n| array(&n.properties["cells"])
                            .iter()
                            .any(|v| reference_id(v) == Some(raw_id(&binding.object)))),
                    "原生保存暂不支持删除表格单元格，请修改其文本或样式"
                );
                remove_visible_reference(parts, &binding.object);
            }
        }
        for (id, (page, widget)) in &current {
            if let Some((_, old)) = previous.get(id) {
                if !self.bindings.contains_key(id) {
                    let expected = self.preview_parents.get(id).and_then(|parent| {
                        Some(transform_preview(
                            old,
                            previous.get(parent)?.1,
                            current.get(parent)?.1,
                        ))
                    });
                    ensure!(
                        expected
                            .as_ref()
                            .map_or(*widget == *old, |expected| matching_preview(
                                widget, expected
                            )),
                        "只读母版预览和元件库名称标签暂不支持编辑；请编辑母版实例本身或普通元件"
                    );
                }
            } else {
                ensure!(
                    !self.library,
                    "元件库原生保存暂不支持新增组件；已有组件可编辑或删除元件"
                );
                let package = self.pages.get(page).context("新增元件的页面绑定丢失")?;
                let object = add_widget(parts, package, widget, &self.pages)?;
                edited_bindings.insert(*id, Binding { object, scale: 1.0 });
            }
        }
        for page in &project.pages {
            let old = self
                .baseline
                .pages
                .iter()
                .find(|p| p.id == page.id)
                .unwrap();
            let old_ids = old
                .widgets
                .iter()
                .filter(|w| current.contains_key(&w.id))
                .map(|w| w.id)
                .collect::<Vec<_>>();
            let new_ids = page
                .widgets
                .iter()
                .filter(|w| previous.contains_key(&w.id))
                .map(|w| w.id)
                .collect::<Vec<_>>();
            let additions_in_middle = page.widgets.iter().enumerate().any(|(i, w)| {
                !previous.contains_key(&w.id)
                    && page.widgets[i + 1..]
                        .iter()
                        .any(|next| previous.contains_key(&next.id))
            });
            let groups_changed = page.widgets.iter().any(|widget| {
                previous
                    .get(&widget.id)
                    .map_or(!widget.groups.is_empty(), |(_, old)| {
                        old.groups != widget.groups
                    })
            });
            if groups_changed {
                ensure!(!self.library, "元件库展示页暂不支持新增或拆分原生组合");
                patch_native_groups(
                    parts,
                    self.pages.get(&page.id).unwrap(),
                    page,
                    old,
                    &edited_bindings,
                    &self.preview_parents,
                    &self.group_bindings,
                )?;
            }
            if old_ids != new_ids || additions_in_middle || groups_changed {
                ensure!(!self.library, "元件库展示页暂不支持重排元件层级");
                let package = self.pages.get(&page.id).unwrap();
                patch_native_order(
                    parts,
                    package,
                    page,
                    old,
                    &edited_bindings,
                    &self.preview_parents,
                )?;
            }
            for (id, name) in &page.group_names {
                let Some(binding) = self.group_bindings.get(id) else {
                    continue; // New groups are named when writing their source objects.
                };
                if old.group_names.get(id) == Some(name) {
                    continue;
                }
                ensure!(
                    old.widgets.iter().any(|widget| {
                        widget.groups.contains(id) && self.bindings.contains_key(&widget.id)
                    }),
                    "只读母版预览中的分组暂不支持重命名"
                );
                let offset = owners.get(&binding.object).context("原生分组部件丢失")?;
                patch_native_group_name(
                    parts.get_mut(offset).unwrap(),
                    raw_id(&binding.object),
                    name,
                )?;
            }
        }
        super::native_events::patch(
            parts,
            &self.baseline,
            project,
            &edited_bindings,
            &self.pages,
        )?;
        Ok(())
    }

    pub(super) fn patch_widget(
        &self,
        parts: &mut BTreeMap<u64, Graph>,
        owners: &BTreeMap<String, u64>,
        binding: &Binding,
        old: &Widget,
        widget: &Widget,
    ) -> Result<()> {
        ensure!(
            widget.text_layout == old.text_layout,
            "原生文字布局元数据暂不支持修改，请保留原有对齐和内边距"
        );
        let offset = *owners.get(&binding.object).context("元件原生部件丢失")?;
        let original = get_node(parts, owners, &binding.object)?.clone();
        let text_id = if widget.kind.is_plain_text_control() {
            raw_id(&binding.object).to_owned()
        } else {
            contained_text_id(parts.get(&offset).unwrap(), &original)
                .unwrap_or_else(|| raw_id(&binding.object).to_owned())
        };
        let style_id = original
            .properties
            .get("MyStyle")
            .and_then(reference_id)
            .context("元件原生样式丢失")?
            .to_owned();
        let style_offset = offset;
        let style_node = parts
            .get_mut(&style_offset)
            .unwrap()
            .objects
            .get_mut(&style_id)
            .context("原生样式对象丢失")?;
        let original_properties = props(&style_node.properties["PropList"]);
        if widget.x != old.x || widget.y != old.y {
            let previous = original_properties.get(&101).unwrap_or(&Value::Null);
            let x = number(&previous["X"]).unwrap_or(0.0) + (widget.x - old.x) / binding.scale;
            let y = number(&previous["Y"]).unwrap_or(0.0) + (widget.y - old.y) / binding.scale;
            set_property(&mut style_node.properties, 101, json!({"X":x,"Y":y}));
        }
        if widget.width != old.width || widget.height != old.height {
            let previous = original_properties.get(&105).unwrap_or(&Value::Null);
            let width = number(&previous["Width"]).unwrap_or(old.width / binding.scale)
                * widget.width
                / old.width;
            let height = number(&previous["Height"]).unwrap_or(old.height / binding.scale)
                * widget.height
                / old.height;
            set_property(
                &mut style_node.properties,
                105,
                json!({"Width":width,"Height":height}),
            );
        }
        let bitmap_vector =
            original.kind.ends_with(":VectorShape") && widget.kind == WidgetKind::Image;
        let points_changed = widget.vector_points != old.vector_points;
        let type_changed = widget.kind != old.kind;
        if (points_changed && !type_changed)
            || (bitmap_vector && widget.image_data != old.image_data)
        {
            validate_native_vector_edit(&original, old, widget)?;
        }
        if points_changed && type_changed {
            ensure!(
                widget.vector_points.is_none(),
                "请先保存元件类型变化，再镜像其新原生路径"
            );
        }
        for (changed, id, color) in [
            (
                widget.fill != old.fill,
                if widget.kind == WidgetKind::Line {
                    8
                } else {
                    7
                },
                &widget.fill,
            ),
            (widget.stroke != old.stroke, 8, &widget.stroke),
            (widget.text_color != old.text_color, 6, &widget.text_color),
        ] {
            if changed {
                set_property(
                    &mut style_node.properties,
                    id,
                    json!({"fill-type":"solid","fill-color":color_argb(color)?}),
                );
            }
        }
        if widget.font_size != old.font_size {
            set_property(
                &mut style_node.properties,
                2,
                json!(widget.font_size / binding.scale * 0.75),
            );
        }
        if widget.radius != old.radius {
            set_property(
                &mut style_node.properties,
                71,
                json!(widget.radius / binding.scale),
            );
        }
        if widget.opacity != old.opacity {
            set_property(&mut style_node.properties, 70, json!(widget.opacity));
        }
        if widget.rotation != old.rotation {
            set_property(
                &mut style_node.properties,
                ROTATION_PROPERTY,
                json!(widget.rotation),
            );
        }
        if widget.image_data != old.image_data
            && !bitmap_vector
            && let Some(image) = &widget.image_data
        {
            let data = image.split_once(',').context("图片不是 data URI")?.1;
            set_property(
                &mut style_node.properties,
                27,
                json!({"Stream":{"$binary":data}}),
            );
        }
        let node = parts
            .get_mut(&offset)
            .unwrap()
            .objects
            .get_mut(raw_id(&binding.object))
            .unwrap();
        if widget.hidden != old.hidden {
            node.properties["VisibleFromSettings"] = json!(!widget.hidden);
        }
        if widget.locked != old.locked {
            node.properties["IsLocked"] = json!(widget.locked);
        }
        if widget.name != old.name {
            node.properties["name-block"]["text"] = json!(widget.name);
        }
        if widget.kind == WidgetKind::Radio && widget.radio_group != old.radio_group {
            node.properties["radio-group"] =
                json!(widget.radio_group.as_deref().unwrap_or(&widget.name));
        }
        if widget.kind != old.kind {
            ensure!(!bitmap_vector, "自定义矢量元件暂不支持更改原生类型");
            set_kind(node, widget.kind)?;
        }
        if points_changed && !type_changed {
            node.properties["ControlPoints"] = widget.vector_points.clone().unwrap();
        }
        if widget.kind.is_option_list() && widget.text != old.text {
            let graph = parts.get_mut(&offset).unwrap();
            let mut node = graph.node(raw_id(&binding.object)).unwrap().clone();
            ensure!(
                list_options_text(&node.properties, graph).is_some(),
                "此列表选项结构尚未识别，不能修改原生选项"
            );
            set_list_options(graph, &mut node, &widget.text);
            graph
                .objects
                .insert(raw_id(&binding.object).to_owned(), node);
        }
        let text_node = parts
            .get_mut(&offset)
            .unwrap()
            .objects
            .get_mut(&text_id)
            .context("原生文本对象丢失")?;
        if widget.text != old.text && !widget.kind.is_option_list() {
            if text_node.kind.ends_with(":RichTextPanel")
                && text_node.properties.get("rtf-block").is_none()
            {
                text_node.properties["rtf-block"] = json!({"*type":"rich-text-block", "attributed-string":{
                    "Version":{"Major":8,"Minor":0,"Build":0,"Revision":0},"Paragraphs":[]}});
            }
            set_text(&mut text_node.properties, &widget.text);
        }
        if widget.font_size != old.font_size || widget.text_color != old.text_color {
            update_inlines(&mut text_node.properties, widget, binding.scale)?;
        }
        if widget.interaction != old.interaction {
            set_interaction(
                parts,
                offset,
                raw_id(&binding.object),
                &widget.interaction,
                &self.pages,
            )?;
        }
        Ok(())
    }
}
