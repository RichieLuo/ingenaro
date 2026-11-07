use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Default, serde::Deserialize, serde::Serialize)]
pub(super) struct ComponentClipboard {
    pub(super) widgets: Vec<Widget>,
    pub(super) group_names: BTreeMap<u64, String>,
}

impl ComponentClipboard {
    pub(super) fn from_selection(page: &Page, widgets: Vec<Widget>) -> Self {
        let groups = widgets
            .iter()
            .flat_map(|w| &w.groups)
            .collect::<BTreeSet<_>>();
        let group_names = page
            .group_names
            .iter()
            .filter(|(id, _)| groups.contains(id))
            .map(|(id, name)| (*id, name.clone()))
            .collect();
        Self {
            widgets,
            group_names,
        }
    }
}

impl App {
    pub(super) fn add_widget(&mut self, kind: WidgetKind, x: f32, y: f32) {
        let id = self.project.allocate_id();
        let mut widget = Widget::new(id, kind, x.max(0.0), y.max(0.0));
        if kind == WidgetKind::Image {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("图片", &["png", "jpg", "jpeg", "gif", "webp"])
                .pick_file()
            else {
                return;
            };
            match std::fs::read(&path)
                .map_err(anyhow::Error::from)
                .and_then(|bytes| {
                    anyhow::ensure!(bytes.len() <= 12 * 1024 * 1024, "图片不能超过 12 MB");
                    let (width, height) = image::ImageReader::new(std::io::Cursor::new(&bytes))
                        .with_guessed_format()?
                        .into_dimensions()?;
                    anyhow::ensure!(
                        width <= 8192
                            && height <= 8192
                            && u64::from(width) * u64::from(height) <= 32_000_000,
                        "图片尺寸不能超过 8192 px 或 3200 万像素"
                    );
                    let img = image::load_from_memory(&bytes)?;
                    let mut png = std::io::Cursor::new(Vec::new());
                    img.write_to(&mut png, image::ImageFormat::Png)?;
                    anyhow::ensure!(
                        png.get_ref().len() <= 12 * 1024 * 1024,
                        "转换后的图片超过 12 MB，请缩小后再导入"
                    );
                    widget.height =
                        (widget.width * img.height() as f32 / img.width() as f32).max(1.0);
                    widget.image_data = Some(format!(
                        "data:image/png;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(png.into_inner())
                    ));
                    Ok(())
                }) {
                Ok(()) => {}
                Err(e) => {
                    self.status = format!("图片导入失败：{e:#}");
                    return;
                }
            }
        }
        self.project.pages[self.page].widgets.push(widget);
        self.selected = BTreeSet::from([id]);
        self.status = format!("已添加{}", kind.label());
    }

    pub(super) fn add_library_item(&mut self, item: LibraryItem, x: f32, y: f32) {
        if item.id == "image" {
            self.add_widget(WidgetKind::Image, x, y);
            return;
        }
        let widgets = item.create(&mut self.project, x.max(0.0), y.max(0.0));
        self.selected = widgets.iter().map(|widget| widget.id).collect();
        self.project.pages[self.page].widgets.extend(widgets);
        self.status = format!("已添加{}", item.label);
    }

    pub(super) fn delete_selection(&mut self) {
        let ids: BTreeSet<u64> = self.project.pages[self.page]
            .widgets
            .iter()
            .filter(|w| {
                self.selected.contains(&w.id)
                    && !w.locked
                    && self
                        .native
                        .as_ref()
                        .is_none_or(|d| d.is_editable_widget(w.id))
            })
            .map(|w| w.id)
            .collect();
        let page = &mut self.project.pages[self.page];
        page.widgets.retain(|w| !ids.contains(&w.id));
        page.prune_group_names();
        for page in &mut self.project.pages {
            for widget in &mut page.widgets {
                if widget
                    .interaction
                    .target_widget
                    .is_some_and(|id| ids.contains(&id))
                {
                    widget.interaction.target_widget = None;
                    widget.interaction.action = Action::None;
                }
            }
        }
        self.project.prune_interaction_references();
        self.selected.clear();
    }

    pub(super) fn copy(&mut self) {
        let page = &self.project.pages[self.page];
        let widgets = page
            .widgets
            .iter()
            .filter(|w| self.selected.contains(&w.id))
            .cloned()
            .collect();
        self.clipboard = ComponentClipboard::from_selection(page, widgets);
        self.status = format!("已复制 {} 个组件", self.clipboard.widgets.len());
    }

    pub(super) fn select_hit(&mut self, id: u64, additive: bool, inside: bool) {
        let id = self
            .native
            .as_ref()
            .map_or(id, |native| native.selection_owner(id));
        let hit = rpshop::grouping::expanded_selection(
            &self.project.pages[self.page],
            &BTreeSet::from([id]),
            inside,
        );
        self.select_layers(hit, additive);
    }

    pub(super) fn select_layers(&mut self, hit: BTreeSet<u64>, additive: bool) {
        if !additive {
            self.selected = hit;
        } else if hit.is_subset(&self.selected) {
            self.selected.retain(|id| !hit.contains(id));
        } else {
            self.selected.extend(hit);
        }
    }

    pub(super) fn group_selection(&mut self, ungroup: bool) {
        let action = if ungroup { "解组" } else { "组合" };
        self.status = match self.edit_groups(ungroup, None) {
            Ok(()) => format!("已{action}"),
            Err(error) => format!("{action}失败：{error:#}"),
        };
    }

    pub(super) fn edit_groups(&mut self, ungroup: bool, name: Option<&str>) -> anyhow::Result<()> {
        let before = self.project.clone();
        let mut draft = before.clone();
        if let Some(native) = &self.native {
            anyhow::ensure!(
                !self.selected.iter().any(|id| {
                    native.selection_owner(*id) == *id && !native.is_editable_widget(*id)
                }),
                "当前选择包含只读内容，无法组合或解组"
            );
            for widget in &mut draft.pages[self.page].widgets {
                if native.selection_owner(widget.id) != widget.id {
                    widget.locked = false;
                }
            }
        }
        let selected = if ungroup {
            rpshop::grouping::ungroup(&mut draft, self.page, &self.selected)?
        } else {
            rpshop::grouping::group_named(&mut draft, self.page, &self.selected, name)?.1
        };
        if let Some(native) = &self.native {
            let locks = before.pages[self.page]
                .widgets
                .iter()
                .map(|widget| (widget.id, widget.locked))
                .collect::<HashMap<_, _>>();
            for widget in &mut draft.pages[self.page].widgets {
                widget.locked = locks[&widget.id];
            }
            native.refresh_previews(&mut draft, &before);
            native.preview_scene(&draft)?;
        }
        self.project = draft;
        self.selected = selected;
        self.remember(before);
        Ok(())
    }

    pub(super) fn flip_selection(&mut self, horizontal: bool) {
        let before = self.project.clone();
        let mut draft = before.clone();
        let mut count = 0;
        let mut result = Ok(());
        for widget in &mut draft.pages[self.page].widgets {
            if !self.selected.contains(&widget.id) || widget.locked {
                continue;
            }
            if self
                .native
                .as_ref()
                .is_some_and(|native| !native.is_editable_widget(widget.id))
            {
                continue;
            }
            if self.native.is_none()
                && widget.vector_points.is_none()
                && widget.kind == WidgetKind::Line
            {
                widget.vector_points = Some(
                    serde_json::json!([[{"Path":{"X":0,"Y":0},"HigherSegmentExists":true},{"Path":{"X":1,"Y":1},"HigherSegmentExists":false}]]),
                );
            }
            let is_bitmap = widget.kind == WidgetKind::Image
                && self
                    .native
                    .as_ref()
                    .is_none_or(|native| native.is_bitmap_widget(widget.id));
            if is_bitmap {
                if let Some(data) = &widget.image_data {
                    match rpshop::bitmap::flip_data_uri(data, horizontal) {
                        Ok(data) => {
                            widget.image_data = Some(data);
                            count += 1;
                        }
                        Err(error) => {
                            result = Err(error);
                            break;
                        }
                    }
                }
            } else if widget.vector_points.is_some() {
                if let Err(error) = rpshop::vector_edit::flip_widget(widget, horizontal) {
                    result = Err(error);
                    break;
                }
                count += 1;
            }
        }
        match result {
            Ok(()) if count > 0 => {
                self.project = draft;
                self.textures.clear();
                self.status = format!(
                    "已{}翻转 {count} 个图像或形状",
                    if horizontal { "水平" } else { "垂直" }
                );
                self.remember(before);
            }
            Ok(()) => self.status = "当前选择没有可翻转的图像或矢量路径".into(),
            Err(error) => self.status = format!("翻转失败：{error:#}"),
        }
    }

    pub(super) fn paste(&mut self) {
        let ComponentClipboard {
            mut widgets,
            group_names,
        } = self.clipboard.clone();
        let mut remap = HashMap::new();
        let mut next_id = self.project.next_id;
        let mut dropped_layout = false;
        for w in &mut widgets {
            let id = next_id;
            let Some(next) = next_id.checked_add(1).filter(|next| *next < u64::MAX) else {
                self.status = "组件标识已耗尽，无法粘贴".into();
                return;
            };
            next_id = next;
            remap.insert(w.id, id);
            w.id = id;
            w.x += 24.0;
            w.y += 24.0;
            if self.native.is_some() && w.horizontal_layout != Default::default() {
                w.horizontal_layout = Default::default();
                dropped_layout = true;
            }
            if w.name.len() + " 副本".len() <= 1024 {
                w.name.push_str(" 副本");
            }
            w.locked = false;
        }
        let groups = match rpshop::grouping::remap_group_ids(&mut widgets, &mut next_id) {
            Ok(groups) => groups,
            Err(error) => {
                self.status = format!("粘贴失败：{error:#}");
                return;
            }
        };
        for w in &mut widgets {
            if let Some(id) = w.interaction.target_widget.and_then(|v| remap.get(&v)) {
                w.interaction.target_widget = Some(*id);
            } else if w.interaction.target_widget.is_some_and(|id| {
                !self.project.pages[self.page]
                    .widgets
                    .iter()
                    .any(|x| x.id == id)
            }) {
                w.interaction = Default::default();
            }
        }
        self.selected = widgets.iter().map(|w| w.id).collect();
        self.project.next_id = next_id;
        let page = &mut self.project.pages[self.page];
        page.widgets.extend(widgets);
        page.group_names.extend(
            group_names
                .into_iter()
                .filter_map(|(old, name)| groups.get(&old).map(|id| (*id, name))),
        );
        let page_id = page.id;
        self.project
            .duplicate_interaction_events(page_id, &remap.into_iter().collect());
        if dropped_layout {
            self.status = "已按固定尺寸粘贴，响应式布局仅支持 RPShop 工程".into();
        }
    }
}
