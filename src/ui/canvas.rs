use super::*;

impl App {
    pub(super) fn load_textures(&mut self, ctx: &egui::Context) {
        let mut pages = BTreeSet::from([self.page]);
        if self.ai_page_overview_visible() {
            for id in self.ai_activity_page_ids() {
                if let Some(index) = self.project.pages.iter().position(|page| page.id == id) {
                    pages.insert(index);
                }
            }
        }
        let needed: BTreeSet<u64> = pages
            .iter()
            .filter_map(|i| self.project.pages.get(*i))
            .flat_map(|p| &p.widgets)
            .map(|w| w.id)
            .collect();
        self.textures.retain(|id, _| needed.contains(id));
        for w in pages
            .iter()
            .filter_map(|i| self.project.pages.get(*i))
            .flat_map(|p| &p.widgets)
        {
            if w.kind != WidgetKind::Image || self.textures.contains_key(&w.id) {
                continue;
            }
            if let Some(data) = &w.image_data
                && let Some((_, payload)) = data.split_once(',')
                && let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(payload)
                && let Ok(img) = image::load_from_memory(&bytes)
            {
                let img = img.into_rgba8();
                let tex = ctx.load_texture(
                    format!("widget-{}", w.id),
                    egui::ColorImage::from_rgba_unmultiplied(
                        [img.width() as usize, img.height() as usize],
                        &img,
                    ),
                    egui::TextureOptions::LINEAR,
                );
                self.textures.insert(w.id, tex);
            }
        }
    }

    pub(super) fn viewport_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() || self.drag.is_some() {
            return;
        }
        let ctrl = egui::Modifiers::CTRL;
        for (key, command) in [
            (egui::Key::Num0, ViewCommand::Zoom(1.0)),
            (egui::Key::Num1, ViewCommand::CenterPage),
            (egui::Key::Num2, ViewCommand::FitSelection),
            (egui::Key::Num3, ViewCommand::CenterSelection),
            (egui::Key::Num9, ViewCommand::Origin),
        ] {
            if Self::consume_editor_key(ctx, ctrl, key) {
                self.view.pending = Some(command);
                self.fit = false;
            }
        }
        if Self::consume_editor_key(ctx, ctrl, egui::Key::Plus)
            || Self::consume_editor_key(ctx, ctrl, egui::Key::Equals)
            || Self::consume_editor_key(ctx, ctrl | egui::Modifiers::SHIFT, egui::Key::Plus)
            || Self::consume_editor_key(ctx, ctrl | egui::Modifiers::SHIFT, egui::Key::Equals)
        {
            self.view.pending = Some(ViewCommand::Step(0.1));
            self.fit = false;
        }
        if Self::consume_editor_key(ctx, ctrl, egui::Key::Minus) {
            self.view.pending = Some(ViewCommand::Step(-0.1));
            self.fit = false;
        }
    }

    pub(super) fn navigate_canvas(
        &mut self,
        ui: &egui::Ui,
        ctx: &egui::Context,
        viewport: Rect,
    ) -> bool {
        if !ui.is_enabled() {
            self.view.panning = None;
            return true;
        }
        let content = viewport::content_rect(viewport);
        let anchor = content.center() - viewport.min;
        let page = &self.project.pages[self.page];
        let output_size = page.size_for_output();
        let page_bounds = Rect::from_min_size(Pos2::ZERO, vec2(output_size[0], output_size[1]));
        if self.fit {
            if page.canvas.is_auto() {
                self.zoom = 1.0;
                self.view.pan = vec2(viewport::RULER_SIZE, viewport::RULER_SIZE);
            } else {
                self.view.fit(&mut self.zoom, viewport, page_bounds);
                self.zoom = self.zoom.min(1.0);
                self.view.center(self.zoom, viewport, page_bounds);
            }
            self.fit = false;
        }
        if let Some(command) = self.view.pending.take() {
            let selected = selection_geometry(page, &self.selected).map(|w| {
                let bounds = manipulation::rotated_bounds(&w);
                Rect::from_min_max(
                    Pos2::new(bounds[0], bounds[1]),
                    Pos2::new(bounds[2], bounds[3]),
                )
            });
            match command {
                ViewCommand::Step(step) => {
                    let target = self.zoom + step;
                    self.view.zoom_at(&mut self.zoom, target, anchor);
                }
                ViewCommand::Zoom(target) => self.view.zoom_at(&mut self.zoom, target, anchor),
                ViewCommand::CenterPage => self.view.center(self.zoom, viewport, page_bounds),
                ViewCommand::FitSelection => {
                    if let Some(bounds) = selected {
                        self.view.fit(&mut self.zoom, viewport, bounds);
                    }
                }
                ViewCommand::CenterSelection => {
                    if let Some(bounds) = selected {
                        self.view.center(self.zoom, viewport, bounds);
                    }
                }
                ViewCommand::Origin => {
                    self.view.pan = if page.canvas.is_auto() {
                        vec2(viewport::RULER_SIZE, viewport::RULER_SIZE)
                    } else {
                        vec2(40.0, 40.0)
                    }
                }
            }
        }
        let hovered = ui.rect_contains_pointer(content);
        let space = !ctx.wants_keyboard_input() && ctx.input(|i| i.key_down(egui::Key::Space));
        let was_panning = self.view.panning.is_some();
        if hovered && self.drag.is_none() {
            let pointer = ctx.input(|i| i.pointer.interact_pos());
            let button = if ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Middle)) {
                Some(egui::PointerButton::Middle)
            } else if space && ctx.input(|i| i.pointer.primary_pressed()) {
                Some(egui::PointerButton::Primary)
            } else {
                None
            };
            if let (Some(button), Some(pointer)) = (button, pointer) {
                self.view.panning = Some(PanDrag {
                    pointer,
                    pan: self.view.pan,
                    button,
                });
            }
        }
        if let Some(drag) = &self.view.panning {
            if let Some(pointer) = ctx.input(|i| i.pointer.interact_pos()) {
                self.view.pan = drag.pan + (pointer - drag.pointer);
            }
            if !ctx.input(|i| i.pointer.button_down(drag.button)) {
                self.view.panning = None;
            }
        }
        let navigating = was_panning
            || self.view.panning.is_some()
            || (space && hovered)
            || (hovered && ctx.input(|i| i.pointer.button_down(egui::PointerButton::Middle)));
        if hovered && self.drag.is_none() && self.view.panning.is_none() {
            let pointer = ctx
                .input(|i| i.pointer.hover_pos())
                .unwrap_or(viewport.center());
            for event in ctx.input(|i| i.events.clone()) {
                match event {
                    egui::Event::MouseWheel {
                        unit,
                        delta,
                        modifiers,
                    } => self.view.wheel(
                        &mut self.zoom,
                        unit,
                        delta,
                        modifiers,
                        pointer - viewport.min,
                        content,
                    ),
                    egui::Event::Zoom(factor) if factor.is_finite() && factor > 0.0 => {
                        let target = self.zoom * factor;
                        self.view
                            .zoom_at(&mut self.zoom, target, pointer - viewport.min);
                    }
                    _ => {}
                }
            }
        }
        if navigating {
            ctx.set_cursor_icon(if self.view.panning.is_some() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
        }
        navigating
    }

    pub(super) fn canvas(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::from_rgb(243, 244, 248)))
            .show(ctx, |ui| {
                if self.confirm.is_some() || self.editing.is_some() {
                    ui.disable();
                }
                let auto_canvas = self.project.pages[self.page].canvas.is_auto();
                if !auto_canvas {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "{}  /  {}",
                                self.project.name, self.project.pages[self.page].name
                            ))
                            .size(11.0)
                            .color(MUTED),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!(
                                    "{:.0} × {:.0}",
                                    self.project.pages[self.page].width,
                                    self.project.pages[self.page].height
                                ))
                                .size(11.0)
                                .color(MUTED),
                            );
                        });
                    });
                    ui.add_space(8.0);
                }
                let size = ui.available_size().max(vec2(1.0, 1.0));
                let (id, workspace) = ui.allocate_space(size);
                let content = viewport::content_rect(workspace);
                // Give ruler gestures an inert owner instead of letting egui's
                // nearest-widget hit testing grab the adjacent sidebar resizer.
                ui.interact(workspace, id.with("ruler-strips"), Sense::click_and_drag());
                let response = ui.interact(content, id, Sense::click_and_drag());
                let navigating = self.navigate_canvas(ui, ctx, workspace);
                // Finish before painting and hit-testing: committed geometry must
                // never remain an invisible/moving pointer target during editing.
                if self.drag.is_some()
                    || self.editing.is_some()
                    || (!navigating
                        && ui.is_enabled()
                        && ui.rect_contains_pointer(content)
                        && ctx.input(|i| {
                            i.pointer.primary_down()
                                || i.pointer.secondary_down()
                                || i.pointer.any_pressed()
                                || i.pointer.any_released()
                        }))
                {
                    self.ai_reveal.clear();
                }
                let reveal_time = ctx.input(|i| i.time);
                self.ai_reveal.prune(&self.project, reveal_time);
                if self.ai_reveal.is_active(reveal_time) {
                    ctx.request_repaint_after(Duration::from_millis(16));
                }
                let page = &self.project.pages[self.page];
                let origin = workspace.min + self.view.pan;
                let page_rect =
                    Rect::from_min_size(origin, vec2(page.width, page.height) * self.zoom);
                let rect = if auto_canvas { content } else { page_rect };
                let workspace_painter = ui
                    .painter()
                    .with_clip_rect(workspace.intersect(ui.clip_rect()));
                let painter = workspace_painter.with_clip_rect(content.intersect(ui.clip_rect()));
                let clipped = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
                let released = ctx.input(|i| i.pointer.primary_released());
                let drop_at = if self.pending_widget.is_some()
                    && ui.is_enabled()
                    && ui.rect_contains_pointer(clipped.clip_rect())
                {
                    ctx.input(|i| {
                        if released {
                            i.pointer.interact_pos()
                        } else {
                            i.pointer.hover_pos()
                        }
                    })
                    .map(|pos| ((pos - origin) / self.zoom).max(egui::Vec2::ZERO))
                } else {
                    None
                };
                // Paint the committed component on the release frame, at the preview's position.
                if released
                    && let Some(drag) = self.pending_widget.take()
                    && let Some(pos) = drop_at
                {
                    self.add_library_item(drag.item, pos.x, pos.y);
                }
                let page = &self.project.pages[self.page];
                if !auto_canvas {
                    painter.rect_filled(
                        rect.translate(vec2(0.0, 5.0)).expand(4.0),
                        5.0,
                        Color32::from_black_alpha(7),
                    );
                }
                painter.rect_filled(rect, 0.0, color(&page.background));
                if self.grid && self.zoom >= 0.35 {
                    let grid = Stroke::new(0.5, Color32::from_black_alpha(9));
                    let x_range = if auto_canvas {
                        (
                            ((rect.left() - origin.x) / self.zoom / 20.0).floor() as i32,
                            ((rect.right() - origin.x) / self.zoom / 20.0).ceil() as i32,
                        )
                    } else {
                        (0, (page.width / 20.0).ceil() as i32)
                    };
                    let y_range = if auto_canvas {
                        (
                            ((rect.top() - origin.y) / self.zoom / 20.0).floor() as i32,
                            ((rect.bottom() - origin.y) / self.zoom / 20.0).ceil() as i32,
                        )
                    } else {
                        (0, (page.height / 20.0).ceil() as i32)
                    };
                    let grid_painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
                    for x in x_range.0..=x_range.1 {
                        let x = x as f32 * 20.0;
                        let px = origin.x + x * self.zoom;
                        grid_painter.line_segment(
                            [Pos2::new(px, rect.top()), Pos2::new(px, rect.bottom())],
                            grid,
                        );
                    }
                    for y in y_range.0..=y_range.1 {
                        let y = y as f32 * 20.0;
                        let py = origin.y + y * self.zoom;
                        grid_painter.line_segment(
                            [Pos2::new(rect.left(), py), Pos2::new(rect.right(), py)],
                            grid,
                        );
                    }
                }
                let mut reveal_focus = None;
                let mut selection_revealing = false;
                for w in &page.widgets {
                    if !w.hidden {
                        let sample = self.ai_reveal.sample(page.id, w, reveal_time);
                        selection_revealing |= self.selected.contains(&w.id) && sample.revealing;
                        if sample.opacity > 0.0 {
                            let mut widget_painter = clipped.clone();
                            widget_painter.multiply_opacity(sample.opacity);
                            draw_widget(
                                &widget_painter,
                                &sample.widget,
                                origin,
                                self.zoom,
                                self.textures.get(&w.id),
                            );
                        }
                        if let Some(progress) = sample.progress {
                            let bounds = manipulation::rotated_bounds(&sample.widget);
                            let bounds = Rect::from_min_max(
                                origin + vec2(bounds[0], bounds[1]) * self.zoom,
                                origin + vec2(bounds[2], bounds[3]) * self.zoom,
                            );
                            if bounds.intersects(clipped.clip_rect()) {
                                reveal_focus = Some((bounds, progress));
                            }
                        }
                    }
                }
                self.ai_reveal
                    .paint_status(&clipped, page.id, reveal_time, reveal_focus);
                for w in page
                    .widgets
                    .iter()
                    .filter(|w| !selection_revealing && self.selected.contains(&w.id) && !w.hidden)
                {
                    draw_selection_frame(&painter, w, origin, self.zoom, false);
                }
                let editable: BTreeSet<_> = page
                    .widgets
                    .iter()
                    .filter(|w| {
                        self.selected.contains(&w.id)
                            && !w.locked
                            && !w.hidden
                            && self
                                .native
                                .as_ref()
                                .is_none_or(|n| n.is_editable_widget(w.id))
                    })
                    .map(|w| w.id)
                    .collect();
                let bounds = (!selection_revealing)
                    .then(|| selection_geometry(page, &editable))
                    .flatten();
                if let Some(bounds) = &bounds {
                    draw_selection_frame(&painter, bounds, origin, self.zoom, true);
                }
                let zoom = self.zoom;
                let handle_at = |pos: Pos2| {
                    bounds.as_ref().and_then(|w| {
                        manipulation::handle_positions(w)
                            .into_iter()
                            .rev()
                            .find_map(|(handle, p)| {
                                (document_pos(p, origin, zoom).distance(pos) <= 8.0)
                                    .then_some(handle)
                            })
                    })
                };
                if let Some(drag) = &self.pending_widget {
                    if let Some(pos) = drop_at {
                        drag.paint(&clipped, origin + pos * zoom, zoom);
                        ctx.set_cursor_icon(egui::CursorIcon::Copy);
                    } else if ui.rect_contains_pointer(content) {
                        ctx.set_cursor_icon(egui::CursorIcon::NoDrop);
                    }
                } else if let Some(pos) = response.hover_pos().filter(|_| !navigating) {
                    if self.tool.is_some() {
                        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                    } else if let Some(handle) = handle_at(pos) {
                        ctx.set_cursor_icon(match handle {
                            Handle::Rotate => egui::CursorIcon::Grab,
                            Handle::North | Handle::South => egui::CursorIcon::ResizeVertical,
                            Handle::East | Handle::West => egui::CursorIcon::ResizeHorizontal,
                            Handle::NorthWest | Handle::SouthEast => egui::CursorIcon::ResizeNwSe,
                            Handle::NorthEast | Handle::SouthWest => egui::CursorIcon::ResizeNeSw,
                        });
                    }
                }
                if !navigating
                    && response.clicked()
                    && let Some(pos) = response
                        .interact_pointer_pos()
                        .filter(|pos| content.contains(*pos))
                {
                    if let Some(kind) = self.tool.take() {
                        if rect.contains(pos) {
                            let p = (pos - origin) / self.zoom;
                            self.add_widget(kind, p.x, p.y);
                        }
                    } else {
                        let p = (pos - origin) / self.zoom;
                        let hit = self.project.pages[self.page]
                            .widgets
                            .iter()
                            .rev()
                            .find(|w| {
                                !w.hidden && manipulation::hit_test(w, [p.x, p.y], 4.0 / self.zoom)
                            })
                            .map(|w| w.id);
                        if let Some(id) = hit {
                            let modifiers = ctx.input(|i| i.modifiers);
                            self.select_hit(
                                id,
                                modifiers.shift || modifiers.ctrl,
                                modifiers.ctrl && modifiers.alt,
                            );
                        } else if !ctx.input(|i| i.modifiers.shift || i.modifiers.ctrl) {
                            self.selected.clear();
                        }
                    }
                }
                if !navigating
                    && response.double_clicked()
                    && response
                        .interact_pointer_pos()
                        .is_some_and(|p| content.contains(p))
                {
                    self.editing = self
                        .selected
                        .iter()
                        .copied()
                        .find(|id| editable.contains(id));
                }
                if !navigating
                    && response.secondary_clicked()
                    && let Some(pos) = response
                        .interact_pointer_pos()
                        .filter(|pos| content.contains(*pos))
                {
                    let p = (pos - origin) / self.zoom;
                    let hit = self.project.pages[self.page]
                        .widgets
                        .iter()
                        .rev()
                        .find(|w| {
                            !w.hidden && manipulation::hit_test(w, [p.x, p.y], 4.0 / self.zoom)
                        })
                        .map(|w| w.id);
                    if let Some(id) = hit
                        && !self.selected.contains(&id)
                    {
                        self.select_hit(
                            id,
                            false,
                            ctx.input(|i| i.modifiers.ctrl && i.modifiers.alt),
                        );
                    }
                }
                if !navigating
                    && response.drag_started_by(egui::PointerButton::Primary)
                    && let Some(pos) = ctx
                        .input(|i| i.pointer.press_origin())
                        .filter(|pos| content.contains(*pos))
                {
                    if self.tool.is_some() && rect.contains(pos) {
                        let kind = self.tool.take().unwrap();
                        let before = self.project.clone();
                        let p = (pos - origin) / self.zoom;
                        let id = self.project.allocate_id();
                        self.project.pages[self.page]
                            .widgets
                            .push(Widget::new(id, kind, p.x, p.y));
                        self.selected = BTreeSet::from([id]);
                        self.drag = Some(Drag {
                            before,
                            pointer: pos,
                            mode: DragMode::Draw {
                                id,
                                kind,
                                start: [p.x, p.y],
                            },
                        });
                    } else {
                        let page = &self.project.pages[self.page];
                        let handle = handle_at(pos);
                        let p = (pos - origin) / self.zoom;
                        let mut mode = DragMode::Move;
                        if let (Some(handle), Some(bounds)) = (handle, bounds.clone()) {
                            mode = if handle == Handle::Rotate {
                                let pivot = [
                                    bounds.x + bounds.width / 2.0,
                                    bounds.y + bounds.height / 2.0,
                                ];
                                DragMode::Rotate {
                                    pivot,
                                    angle: (p.y - pivot[1]).atan2(p.x - pivot[0]).to_degrees(),
                                    start: [p.x, p.y],
                                }
                            } else {
                                DragMode::Resize {
                                    handle,
                                    bounds: Box::new(bounds),
                                }
                            };
                        } else {
                            let hit = page
                                .widgets
                                .iter()
                                .rev()
                                .find(|w| {
                                    let owner = self
                                        .native
                                        .as_ref()
                                        .map_or(w.id, |n| n.selection_owner(w.id));
                                    let unlocked = if owner == w.id {
                                        !w.locked
                                    } else {
                                        page.widgets.iter().any(|w| w.id == owner && !w.locked)
                                    };
                                    !w.hidden
                                        && unlocked
                                        && self.native.as_ref().is_none_or(|n| {
                                            n.is_editable_widget(n.selection_owner(w.id))
                                        })
                                        && manipulation::hit_test(w, [p.x, p.y], 4.0 / self.zoom)
                                })
                                .map(|w| {
                                    self.native
                                        .as_ref()
                                        .map_or(w.id, |n| n.selection_owner(w.id))
                                });
                            if let Some(id) = hit {
                                if !self.selected.contains(&id) {
                                    if !ctx.input(|i| i.modifiers.shift || i.modifiers.ctrl) {
                                        self.selected.clear();
                                    }
                                    let inside = ctx.input(|i| i.modifiers.ctrl && i.modifiers.alt);
                                    self.selected.extend(rpshop::grouping::expanded_selection(
                                        &self.project.pages[self.page],
                                        &BTreeSet::from([id]),
                                        inside,
                                    ));
                                }
                            } else {
                                let initial =
                                    if ctx.input(|i| i.modifiers.shift || i.modifiers.ctrl) {
                                        self.selected.clone()
                                    } else {
                                        BTreeSet::new()
                                    };
                                self.selected = initial.clone();
                                mode = DragMode::Marquee { initial };
                            }
                        }
                        if !self.selected.is_empty() || matches!(mode, DragMode::Marquee { .. }) {
                            self.drag = Some(Drag {
                                before: self.project.clone(),
                                pointer: pos,
                                mode,
                            });
                        }
                    }
                }
                if !navigating
                    && response.dragged_by(egui::PointerButton::Primary)
                    && let Some(drag) = &self.drag
                    && let Some(pos) = response
                        .interact_pointer_pos()
                        .filter(|pos| content.contains(*pos))
                {
                    let delta = (pos - drag.pointer) / self.zoom;
                    let modifiers = ctx.input(|i| i.modifiers);
                    if let DragMode::Marquee { initial } = &drag.mode {
                        let from = (drag.pointer - origin) / self.zoom;
                        let to = (pos - origin) / self.zoom;
                        self.selected = initial.clone();
                        self.selected.extend(rpshop::selection::box_select(
                            &self.project.pages[self.page].widgets,
                            [from.x, from.y],
                            [to.x, to.y],
                            rpshop::selection::BoxSelectionMode::Contained,
                        ));
                        self.selected = rpshop::grouping::expanded_selection(
                            &self.project.pages[self.page],
                            &self.selected,
                            modifiers.ctrl && modifiers.alt,
                        );
                        painter.rect_filled(
                            Rect::from_two_pos(drag.pointer, pos),
                            0.0,
                            PURPLE.gamma_multiply(0.08),
                        );
                        painter.rect_stroke(
                            Rect::from_two_pos(drag.pointer, pos),
                            0.0,
                            Stroke::new(1.0, PURPLE),
                            StrokeKind::Inside,
                        );
                    } else {
                        let editable: BTreeSet<_> = self.project.pages[self.page]
                            .widgets
                            .iter()
                            .filter(|w| {
                                self.selected.contains(&w.id)
                                    && !w.locked
                                    && self
                                        .native
                                        .as_ref()
                                        .is_none_or(|n| n.is_editable_widget(w.id))
                            })
                            .map(|w| w.id)
                            .collect();
                        apply_drag(
                            &mut self.project.pages[self.page],
                            &drag.before.pages[self.page],
                            &editable,
                            &drag.mode,
                            [delta.x, delta.y],
                            modifiers.shift,
                            modifiers.alt,
                            self.snap,
                        );
                        if let Some(native) = &self.native {
                            native.refresh_previews(&mut self.project, &drag.before);
                        }
                    }
                }
                if !navigating
                    && response.drag_stopped_by(egui::PointerButton::Primary)
                    && let Some(drag) = self.drag.take()
                {
                    self.remember(drag.before);
                }
                response.context_menu(|ui| {
                    if ui.button("粘贴").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                        ui.close();
                    }
                    if ui.button("复制").clicked() {
                        self.copy_to_system(ctx);
                        ui.close();
                    }
                    if ui
                        .add_enabled(!self.selected.is_empty(), egui::Button::new("剪切  Ctrl+X"))
                        .clicked()
                    {
                        self.cut_components(ctx);
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            !self.selected.is_empty(),
                            egui::Button::new("创建副本  Ctrl+D"),
                        )
                        .clicked()
                    {
                        self.copy();
                        self.paste();
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(self.selected.len() > 1, egui::Button::new("组合  Ctrl+G"))
                        .clicked()
                    {
                        self.group_selection(false);
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            !self.selected.is_empty(),
                            egui::Button::new("解组  Ctrl+Shift+G"),
                        )
                        .clicked()
                    {
                        self.group_selection(true);
                        ui.close();
                    }
                    if ui
                        .add_enabled(!self.selected.is_empty(), egui::Button::new("置于顶层"))
                        .clicked()
                    {
                        self.align(8);
                        ui.close();
                    }
                    if ui
                        .add_enabled(!self.selected.is_empty(), egui::Button::new("置于底层"))
                        .clicked()
                    {
                        self.align(11);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("删除").clicked() {
                        self.delete_selection();
                        ui.close();
                    }
                });
                viewport::paint_rulers(&workspace_painter, workspace, origin, self.zoom);
            });
    }
}

#[cfg(test)]
mod reveal_tests {
    use super::*;

    #[test]
    fn pointer_press_finishes_reveal_before_selection_without_creating_history() {
        let before = Project::blank();
        let mut after = before.clone();
        after.pages[0]
            .widgets
            .push(Widget::new(2, WidgetKind::Rectangle, 0.0, 0.0));
        after.next_id = 3;
        let mut app = tests::fixture(after.clone());
        app.zoom = 1.0;
        app.ai_reveal.record(&before, &after, 0.0);
        let context = egui::Context::default();
        let position = Pos2::new(100.0, 100.0);
        for (time, pressed) in [(0.0, None), (0.1, Some(true)), (0.12, Some(false))] {
            let mut events = vec![egui::Event::PointerMoved(position)];
            if let Some(pressed) = pressed {
                events.push(egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let _ = context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 600.0))),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| app.canvas(ctx),
            );
            assert_eq!(app.ai_reveal.is_active(time), pressed.is_none());
            assert_eq!(app.project, after);
            assert!(app.undo.is_empty());
        }
        assert_eq!(app.selected, BTreeSet::from([2]));
    }
}
