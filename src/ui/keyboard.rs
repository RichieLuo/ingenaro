use super::*;

impl App {
    pub(super) fn keyboard(&mut self, ctx: &egui::Context) {
        if let Some(save_as) = Self::consume_save_shortcut(ctx) {
            self.save_project(save_as);
        }
        if Self::consume_editor_key(ctx, egui::Modifiers::CTRL, egui::Key::O) {
            self.request_replace("open");
        }
        if Self::consume_editor_key(ctx, egui::Modifiers::CTRL, egui::Key::N) {
            self.request_replace("new");
        }
        if Self::consume_editor_key(ctx, egui::Modifiers::NONE, egui::Key::F5) {
            self.start_preview(ctx);
        }
        if Self::consume_editor_key(ctx, egui::Modifiers::NONE, egui::Key::Escape) {
            self.tool = None;
        }
        if ctx.wants_keyboard_input() {
            return;
        }
        for (key, kind) in [
            (egui::Key::R, WidgetKind::Rectangle),
            (egui::Key::O, WidgetKind::Ellipse),
            (egui::Key::T, WidgetKind::Text),
            (egui::Key::L, WidgetKind::Line),
        ] {
            if Self::consume_editor_key(ctx, egui::Modifiers::NONE, key) {
                self.tool = Some(kind);
                self.status = format!("{}工具：在画布中绘制", kind.label());
            }
        }
        for (key, kind) in [
            (egui::Key::B, WidgetKind::Rectangle),
            (egui::Key::E, WidgetKind::Ellipse),
            (egui::Key::T, WidgetKind::Text),
        ] {
            if Self::consume_editor_key(ctx, egui::Modifiers::CTRL | egui::Modifiers::SHIFT, key) {
                self.tool = Some(kind);
                self.status = format!("{}工具：在画布中绘制", kind.label());
            }
        }
        if Self::consume_editor_key(ctx, egui::Modifiers::NONE, egui::Key::V) {
            self.tool = None;
        }
        for (key, mode) in [
            (egui::Key::L, 0),
            (egui::Key::C, 1),
            (egui::Key::R, 2),
            (egui::Key::T, 3),
            (egui::Key::M, 4),
            (egui::Key::B, 5),
        ] {
            if Self::consume_editor_key(ctx, egui::Modifiers::CTRL | egui::Modifiers::ALT, key) {
                self.align(mode);
                return;
            }
        }
        for (key, mode) in [(egui::Key::H, 6), (egui::Key::U, 7)] {
            if Self::consume_editor_key(ctx, egui::Modifiers::CTRL | egui::Modifiers::SHIFT, key) {
                self.align(mode);
                return;
            }
        }
        if Self::consume_editor_key(
            ctx,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            egui::Key::Z,
        ) {
            self.redo();
        } else if Self::consume_editor_key(ctx, egui::Modifiers::CTRL, egui::Key::Z) {
            self.undo();
        }
        if Self::consume_editor_key(ctx, egui::Modifiers::CTRL, egui::Key::Y) {
            self.redo();
        }
        let before = self.project.clone();
        if Self::consume_editor_key(
            ctx,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            egui::Key::K,
        ) {
            let lock = self.project.pages[self.page].widgets.iter().any(|widget| {
                self.selected.contains(&widget.id)
                    && !widget.locked
                    && self
                        .native
                        .as_ref()
                        .is_none_or(|native| native.is_editable_widget(widget.id))
            });
            for widget in &mut self.project.pages[self.page].widgets {
                if self.selected.contains(&widget.id)
                    && self
                        .native
                        .as_ref()
                        .is_none_or(|native| native.is_editable_widget(widget.id))
                {
                    widget.locked = lock;
                }
            }
            self.status = if lock {
                "已锁定所选组件"
            } else {
                "已解锁所选组件"
            }
            .into();
            self.remember(before);
            return;
        }
        if Self::consume_editor_key(
            ctx,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            egui::Key::G,
        ) {
            self.group_selection(true);
            return;
        } else if Self::consume_editor_key(ctx, egui::Modifiers::CTRL, egui::Key::G) {
            self.group_selection(false);
            return;
        }
        if Self::consume_editor_key(ctx, egui::Modifiers::CTRL, egui::Key::A) {
            self.selected = self.project.pages[self.page]
                .widgets
                .iter()
                .filter(|w| !w.locked && !w.hidden)
                .map(|w| w.id)
                .collect();
        }
        // Selectable labels handle Copy during painting, after editor shortcuts.
        if self.ai.visible
            && ctx
                .plugin::<egui::text_selection::LabelSelectionState>()
                .lock()
                .has_selection()
            && ctx.input(|input| {
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Copy))
            })
        {
            return;
        }
        // Native desktop backends emit clipboard events instead of Key(C/X/V).
        // Leave these events untouched while a text editor owns keyboard focus.
        let (copy, cut, paste) = ctx.input_mut(|input| {
            let mut copy = false;
            let mut cut = false;
            let mut paste = None;
            input.events.retain(|event| match event {
                egui::Event::Copy => {
                    copy = true;
                    false
                }
                egui::Event::Cut => {
                    cut = true;
                    false
                }
                egui::Event::Paste(contents) => {
                    paste = Some(contents.clone());
                    false
                }
                _ => true,
            });
            (copy, cut, paste)
        });
        if Self::consume_editor_key(ctx, egui::Modifiers::CTRL, egui::Key::C) || copy {
            self.copy_to_system(ctx);
        }
        if Self::consume_editor_key(ctx, egui::Modifiers::CTRL, egui::Key::X) || cut {
            self.cut_components(ctx);
        }
        if let Some(contents) = paste {
            self.paste_components(&contents);
        } else if Self::consume_editor_key(ctx, egui::Modifiers::CTRL, egui::Key::V) {
            self.paste();
        }
        if Self::consume_editor_key(ctx, egui::Modifiers::CTRL, egui::Key::D) {
            self.copy();
            self.paste();
        }
        if Self::consume_editor_key(ctx, egui::Modifiers::NONE, egui::Key::Delete)
            || Self::consume_editor_key(ctx, egui::Modifiers::NONE, egui::Key::Backspace)
        {
            self.delete_selection();
        }
        for (key, dx, dy) in [
            (egui::Key::ArrowLeft, -1.0, 0.0),
            (egui::Key::ArrowRight, 1.0, 0.0),
            (egui::Key::ArrowUp, 0.0, -1.0),
            (egui::Key::ArrowDown, 0.0, 1.0),
        ] {
            let resize_fast =
                Self::consume_editor_key(ctx, egui::Modifiers::CTRL | egui::Modifiers::SHIFT, key);
            if resize_fast || Self::consume_editor_key(ctx, egui::Modifiers::CTRL, key) {
                let step = if resize_fast { 10.0 } else { 1.0 };
                for widget in &mut self.project.pages[self.page].widgets {
                    if self.selected.contains(&widget.id)
                        && !widget.locked
                        && self
                            .native
                            .as_ref()
                            .is_none_or(|native| native.is_editable_widget(widget.id))
                    {
                        widget.width = (widget.width + dx * step).clamp(1.0, 20_000.0);
                        widget.height = (widget.height + dy * step).clamp(1.0, 20_000.0);
                    }
                }
            }
            let fast = Self::consume_editor_key(ctx, egui::Modifiers::SHIFT, key);
            if fast || Self::consume_editor_key(ctx, egui::Modifiers::NONE, key) {
                let step = if fast { 10.0 } else { 1.0 };
                for w in &mut self.project.pages[self.page].widgets {
                    if self.selected.contains(&w.id)
                        && !w.locked
                        && self
                            .native
                            .as_ref()
                            .is_none_or(|native| native.is_editable_widget(w.id))
                    {
                        w.x += dx * step;
                        w.y += dy * step;
                    }
                }
            }
        }
        self.remember(before);
    }

    pub(super) fn consume_editor_key(
        ctx: &egui::Context,
        modifiers: egui::Modifiers,
        key: egui::Key,
    ) -> bool {
        ctx.input_mut(|input| {
            let mut consumed = false;
            // egui-winit recognizes any Ctrl+C as Copy, including Ctrl+Alt+C.
            // Recover the editor's horizontal-center shortcut before treating
            // the remaining native Copy event as an ordinary clipboard action.
            let native_center = key == egui::Key::C
                && modifiers == (egui::Modifiers::CTRL | egui::Modifiers::ALT)
                && input.modifiers.matches_exact(modifiers);
            input.events.retain(|event| {
                let matches = matches!(event, egui::Event::Key {
                    key: pressed_key,
                    pressed: true,
                    modifiers: pressed_modifiers,
                    ..
                } if *pressed_key == key && pressed_modifiers.matches_exact(modifiers))
                    || (native_center && matches!(event, egui::Event::Copy));
                consumed |= matches;
                !matches
            });
            consumed
        })
    }

    pub(super) fn consume_save_shortcut(ctx: &egui::Context) -> Option<bool> {
        if Self::consume_editor_key(
            ctx,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            egui::Key::S,
        ) {
            Some(true)
        } else if Self::consume_editor_key(ctx, egui::Modifiers::CTRL, egui::Key::S) {
            Some(false)
        } else {
            None
        }
    }

    pub(super) fn write_component_clipboard(&self, ctx: &egui::Context) {
        if self.clipboard.widgets.is_empty() {
            return;
        }
        let (version, contents) = if self.clipboard.group_names.is_empty() {
            (1, serde_json::to_string(&self.clipboard.widgets))
        } else {
            (2, serde_json::to_string(&self.clipboard))
        };
        if let Ok(contents) = contents {
            ctx.copy_text(format!("RPShop.components.v{version}\n{contents}"));
        }
    }

    pub(super) fn copy_to_system(&mut self, ctx: &egui::Context) {
        self.copy();
        self.write_component_clipboard(ctx);
    }

    pub(super) fn cut_components(&mut self, ctx: &egui::Context) {
        let page = &self.project.pages[self.page];
        let widgets = page
            .widgets
            .iter()
            .filter(|widget| {
                self.selected.contains(&widget.id)
                    && !widget.locked
                    && self
                        .native
                        .as_ref()
                        .is_none_or(|native| native.is_editable_widget(widget.id))
            })
            .cloned()
            .collect();
        self.clipboard = editing::ComponentClipboard::from_selection(page, widgets);
        if self.clipboard.widgets.is_empty() {
            self.status = "当前选择没有可剪切的组件".into();
            return;
        }
        self.write_component_clipboard(ctx);
        let count = self.clipboard.widgets.len();
        self.delete_selection();
        self.status = format!("已剪切 {count} 个组件");
    }

    pub(super) fn paste_components(&mut self, contents: &str) {
        let (contents, legacy) =
            if let Some(contents) = contents.strip_prefix("RPShop.components.v1\n") {
                (contents, true)
            } else if let Some(contents) = contents.strip_prefix("RPShop.components.v2\n") {
                (contents, false)
            } else {
                self.status = "剪贴板中没有可粘贴的组件".into();
                return;
            };
        if contents.len() > 48 * 1024 * 1024 {
            self.status = "剪贴板组件超过 48 MB 限制".into();
            return;
        }
        let parsed = if legacy {
            serde_json::from_str::<Vec<Widget>>(contents).map(|widgets| {
                editing::ComponentClipboard {
                    widgets,
                    ..Default::default()
                }
            })
        } else {
            serde_json::from_str(contents)
        };
        let clipboard = match parsed {
            Ok(clipboard) if !clipboard.widgets.is_empty() && clipboard.widgets.len() <= 5_000 => {
                clipboard
            }
            _ => {
                self.status = "剪贴板组件数据无效".into();
                return;
            }
        };
        let extra_ids = clipboard
            .widgets
            .iter()
            .map(|widget| widget.groups.len() as u64 + 1)
            .sum::<u64>();
        if self.project.next_id.checked_add(extra_ids + 1).is_none() {
            self.status = "组件标识已耗尽，无法粘贴".into();
            return;
        }
        let before = self.project.clone();
        let selected = self.selected.clone();
        let expected_count = before.pages[self.page].widgets.len() + clipboard.widgets.len();
        let old_clipboard = std::mem::replace(&mut self.clipboard, clipboard);
        self.paste();
        if self.project.pages[self.page].widgets.len() != expected_count {
            self.project = before;
            self.selected = selected;
            self.clipboard = old_clipboard;
            return;
        }
        // A copied navigation target may belong to another document.
        let pages: BTreeSet<u64> = self.project.pages.iter().map(|page| page.id).collect();
        for widget in &mut self.project.pages[self.page].widgets {
            if self.selected.contains(&widget.id)
                && widget
                    .interaction
                    .target_page
                    .is_some_and(|id| !pages.contains(&id))
            {
                widget.interaction = Default::default();
            }
        }
        if let Err(error) = self.project.validate() {
            self.project = before;
            self.selected = selected;
            self.clipboard = old_clipboard;
            self.status = format!("粘贴失败：{error:#}");
        } else {
            self.status = format!("已粘贴 {} 个组件", self.selected.len());
        }
    }
}
