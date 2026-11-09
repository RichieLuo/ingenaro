//! Bounded image drafts and previews. No attachment is written into the prototype file.
use super::*;
mod clipboard;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct ImageDraft {
    loading: Option<mpsc::Receiver<Result<Vec<ai::ImageAttachment>, String>>>,
    loading_generation: Option<String>,
    quiet_clipboard: bool,
    pub(super) thumbnails: HashMap<String, egui::TextureHandle>,
    rendering: HashMap<String, ImageRender>,
    pub preview: Option<String>,
    paste_down: bool,
}

enum ImageRender {
    Loading(mpsc::Receiver<Result<egui::ColorImage, String>>),
    Failed(String),
}

pub(super) fn reference_image_picker() -> rfd::FileDialog {
    rfd::FileDialog::new().add_filter("参考图片", &["png", "jpg", "jpeg", "webp", "gif"])
}

impl ImageDraft {
    fn texture(
        &mut self,
        ctx: &egui::Context,
        attachment: &ai::ImageAttachment,
    ) -> Result<Option<egui::TextureHandle>, String> {
        let id = attachment.id();
        if let Some(texture) = self.thumbnails.get(id) {
            return Ok(Some(texture.clone()));
        }
        let rendering = self.rendering.entry(id.into()).or_insert_with(|| {
            let attachment = attachment.clone();
            let edge = ctx.input(|input| input.max_texture_side).min(1280) as u32;
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let result = image::load_from_memory(attachment.png_bytes())
                    .map(|image| {
                        let image = image
                            .thumbnail(edge.min(image.width()), edge.min(image.height()))
                            .into_rgba8();
                        egui::ColorImage::from_rgba_unmultiplied(
                            [image.width() as usize, image.height() as usize],
                            image.as_raw(),
                        )
                    })
                    .map_err(|_| "无法生成图片预览".to_owned());
                let _ = tx.send(result);
            });
            ImageRender::Loading(rx)
        });
        let result = match rendering {
            ImageRender::Failed(error) => return Err(error.clone()),
            ImageRender::Loading(receiver) => match receiver.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(32));
                    return Ok(None);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err("图片预览线程意外结束，请重新添加图片".into())
                }
            },
        };
        match result {
            Ok(image) => {
                let texture = ctx.load_texture(
                    format!("ai-image-{id}"),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                self.rendering.remove(id);
                self.thumbnails.insert(id.into(), texture.clone());
                Ok(Some(texture))
            }
            Err(error) => {
                *rendering = ImageRender::Failed(error.clone());
                Err(error)
            }
        }
    }
}

impl App {
    pub(super) fn ai_pick_images(&mut self) {
        if self.ai.image_draft.loading.is_some() {
            return;
        }
        let existing = self.ai.images.len();
        if let Err(error) = image_capacity(existing, 1) {
            self.ai.state = error;
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.ai.image_draft.loading = Some(rx);
        self.ai.image_draft.loading_generation = Some(self.ai.generation.clone());
        self.ai.image_draft.quiet_clipboard = false;
        std::thread::spawn(move || {
            let result = (|| {
                let Some(paths) = reference_image_picker().pick_files() else {
                    return Ok(vec![]);
                };
                image_capacity(existing, paths.len())?;
                paths
                    .into_iter()
                    .map(|path| {
                        ai::ImageAttachment::from_file(path).map_err(|error| error.to_string())
                    })
                    .collect::<Result<Vec<_>, String>>()
            })();
            let _ = tx.send(result);
        });
    }
    pub(super) fn ai_load_images(&mut self, files: Vec<egui::DroppedFile>) {
        if self.ai.image_draft.loading.is_some() {
            self.ai.state = "图片仍在读取，请稍后再添加".into();
            return;
        }
        if let Err(error) = image_capacity(self.ai.images.len(), files.len()) {
            self.ai.state = error;
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.ai.image_draft.loading = Some(rx);
        self.ai.image_draft.loading_generation = Some(self.ai.generation.clone());
        self.ai.image_draft.quiet_clipboard = false;
        std::thread::spawn(move || {
            let result = files
                .into_iter()
                .map(|file| {
                    if let Some(bytes) = file.bytes {
                        ai::ImageAttachment::from_bytes(file.name, &bytes)
                            .map_err(|error| error.to_string())
                    } else if let Some(path) = file.path {
                        ai::ImageAttachment::from_file(path).map_err(|error| error.to_string())
                    } else {
                        Err("无法读取拖入图片".into())
                    }
                })
                .collect::<Result<Vec<_>, String>>();
            let _ = tx.send(result);
        });
    }
    pub(super) fn ai_paste_image(&mut self) {
        if self.ai.image_draft.loading.is_some() {
            return;
        }
        if let Err(error) = image_capacity(self.ai.images.len(), 1) {
            self.ai.state = error;
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.ai.image_draft.loading = Some(rx);
        self.ai.image_draft.loading_generation = Some(self.ai.generation.clone());
        self.ai.image_draft.quiet_clipboard = false;
        std::thread::spawn(move || {
            let result = clipboard::read();
            let _ = tx.send(result);
        });
    }
    pub(super) fn ai_append_images(
        &mut self,
        images: Vec<ai::ImageAttachment>,
    ) -> Result<(), String> {
        let mut next = self.ai.images.clone();
        next.extend(images);
        ai::validate_image_attachments(&next).map_err(|error| error.to_string())?;
        self.ai.images = next;
        Ok(())
    }
    pub(super) fn ai_poll_images(&mut self) {
        let result =
            self.ai
                .image_draft
                .loading
                .as_ref()
                .and_then(|receiver| match receiver.try_recv() {
                    Ok(result) => Some(result),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some(Err("图片读取线程意外结束，请重试".into()))
                    }
                });
        if let Some(result) = result {
            self.ai.image_draft.loading = None;
            if self.ai.image_draft.loading_generation.take().as_deref() != Some(&self.ai.generation)
            {
                return;
            }
            match result {
                Ok(images) => {
                    if !images.is_empty() {
                        let count = images.len();
                        match self.ai_append_images(images) {
                            Ok(()) => {
                                self.ai.state =
                                    format!("已添加 {count} 张图片，发送时会交给所选智能体")
                            }
                            Err(error) => self.ai.state = error,
                        }
                    }
                }
                Err(error) => {
                    if !self.ai.image_draft.quiet_clipboard || error != clipboard::EMPTY_MESSAGE {
                        self.ai.state = error;
                    }
                }
            }
        }
        self.ai
            .image_draft
            .thumbnails
            .retain(|id, _| self.ai.images.iter().any(|image| image.id() == id));
        self.ai
            .image_draft
            .rendering
            .retain(|id, _| self.ai.images.iter().any(|image| image.id() == id));
    }
    pub(super) fn ai_image_loading(&self) -> bool {
        self.ai.image_draft.loading.is_some()
    }
    pub(super) fn ai_images_height(&self) -> f32 {
        if self.ai.images.is_empty() && !self.ai_image_loading() {
            0.0
        } else {
            82.0
        }
    }
    pub(super) fn ai_images_strip(&mut self, ui: &mut egui::Ui) {
        if self.ai.images.is_empty() {
            if self.ai_image_loading() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("正在读取图片…");
                });
            }
            return;
        }
        let mut remove = None;
        let mut preview = None;
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("ai-image-draft-strip")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                        ui.horizontal(|ui| {
                            for attachment in &self.ai.images {
                                let texture = self.ai.image_draft.texture(ui.ctx(), attachment);
                                if let Err(error) = &texture {
                                    self.ai.state = error.clone();
                                }
                                let (rect, response) =
                                    ui.allocate_exact_size(vec2(84.0, 66.0), Sense::click());
                                ui.painter().rect_filled(
                                    rect,
                                    7.0,
                                    Color32::from_rgb(241, 244, 248),
                                );
                                let image_rect = Rect::from_center_size(
                                    rect.center() - vec2(0.0, 7.0),
                                    fit_size(
                                        vec2(attachment.width() as f32, attachment.height() as f32),
                                        vec2(76.0, 45.0),
                                    ),
                                );
                                if let Ok(Some(texture)) = texture {
                                    ui.painter().image(
                                        texture.id(),
                                        image_rect,
                                        Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
                                        Color32::WHITE,
                                    );
                                }
                                ui.painter().text(
                                    rect.center_bottom() - vec2(0.0, 8.0),
                                    Align2::CENTER_CENTER,
                                    short_name(attachment.name()),
                                    FontId::proportional(9.0),
                                    MUTED,
                                );
                                if response
                                    .on_hover_text(format!(
                                        "{} · {} × {}\n点击预览，右上角移除",
                                        attachment.name(),
                                        attachment.width(),
                                        attachment.height()
                                    ))
                                    .clicked()
                                {
                                    preview = Some(attachment.id().to_owned());
                                }
                                let close = Rect::from_center_size(
                                    rect.right_top() + vec2(-8.0, 8.0),
                                    vec2(18.0, 18.0),
                                );
                                let response = ui.interact(
                                    close,
                                    ui.make_persistent_id(("remove-ai-image", attachment.id())),
                                    Sense::click(),
                                );
                                ui.painter()
                                    .circle_filled(close.center(), 8.0, Color32::WHITE);
                                for (a, b) in [
                                    (vec2(-3.0, -3.0), vec2(3.0, 3.0)),
                                    (vec2(-3.0, 3.0), vec2(3.0, -3.0)),
                                ] {
                                    ui.painter().line_segment(
                                        [close.center() + a, close.center() + b],
                                        Stroke::new(1.0, MUTED),
                                    );
                                }
                                #[cfg(test)]
                                ui.ctx().data_mut(|data| {
                                    data.insert_temp(
                                        egui::Id::new(("ai-image-remove-test", attachment.id())),
                                        close,
                                    );
                                    data.insert_temp(
                                        egui::Id::new(("ai-image-open-test", attachment.id())),
                                        rect,
                                    );
                                });
                                if response.on_hover_text("移除图片").clicked() {
                                    remove = Some(attachment.id().to_owned());
                                }
                            }
                        });
                    });
            });
        if let Some(id) = &remove {
            self.ai.images.retain(|image| image.id() != id);
        }
        if let Some(id) = preview
            && remove.as_ref() != Some(&id)
        {
            self.ai.image_draft.preview = Some(id);
        }
    }
    pub(in crate::ui) fn ai_image_preview(&mut self, ctx: &egui::Context) {
        let Some(id) = self.ai.image_draft.preview.clone() else {
            return;
        };
        let Some(attachment) = self
            .ai
            .images
            .iter()
            .find(|image| image.id() == id)
            .cloned()
        else {
            self.ai.image_draft.preview = None;
            return;
        };
        self.ai.embedded.hide();
        let mut close = false;
        dialog_style::window(ctx, "参考图片")
            .default_width(640.0)
            .show(ctx, |ui| {
                close |= dialog_style::header(
                    ui,
                    dialog_style::Symbol::Preview,
                    "参考图片",
                    attachment.name(),
                );
                match self.ai.image_draft.texture(ctx, &attachment) {
                    Ok(Some(texture)) => {
                        ui.image((
                            texture.id(),
                            fit_size(
                                vec2(attachment.width() as f32, attachment.height() as f32),
                                vec2(590.0, (ctx.content_rect().height() - 210.0).max(100.0)),
                            ),
                        ));
                    }
                    Ok(None) => {
                        ui.spinner();
                        ui.label("正在生成图片预览…");
                    }
                    Err(error) => {
                        ui.label(error);
                    }
                }
                dialog_style::note(ui, "仅在发送需求时交给所选本地 Agent；不写入原型工程。");
            });
        if close {
            self.ai.image_draft.preview = None;
        }
    }
    pub(in crate::ui) fn ai_handle_image_input(&mut self, ctx: &egui::Context) {
        self.ai_poll_images();
        if !self.ai.visible
            || self.plugins.open
            || self.ai.settings_open
            || self.confirm.is_some()
            || self.ai_modal_active()
            || self.editing.is_some()
        {
            return;
        }
        let files = ctx.input(|input| input.raw.dropped_files.clone());
        if !files.is_empty() && files.iter().all(is_image_drop) {
            ctx.input_mut(|input| input.raw.dropped_files.clear());
            self.ai_load_images(files);
        }
        let editor_focused = self
            .ai
            .prompt_editor_id
            .is_some_and(|id| ctx.memory(|memory| memory.has_focus(id)));
        let paste = clipboard::pressed(ctx, &mut self.ai.image_draft.paste_down);
        if editor_focused && paste {
            self.ai_paste_image();
            self.ai.image_draft.quiet_clipboard = true;
        }
        #[cfg(target_os = "windows")]
        if editor_focused {
            ctx.request_repaint_after(Duration::from_millis(32));
        }
    }
}
fn fit_size(image: egui::Vec2, bounds: egui::Vec2) -> egui::Vec2 {
    image * (bounds.x / image.x).min(bounds.y / image.y).min(1.0)
}
fn short_name(name: &str) -> String {
    if name.chars().count() > 10 {
        format!("{}…", name.chars().take(9).collect::<String>())
    } else {
        name.into()
    }
}
fn image_capacity(existing: usize, additional: usize) -> Result<(), String> {
    if additional > ai::MAX_IMAGE_ATTACHMENTS.saturating_sub(existing) {
        Err(format!(
            "每条需求最多添加 {} 张图片",
            ai::MAX_IMAGE_ATTACHMENTS
        ))
    } else {
        Ok(())
    }
}

fn is_image_drop(file: &egui::DroppedFile) -> bool {
    let path = file
        .path
        .as_deref()
        .unwrap_or_else(|| std::path::Path::new(&file.name));
    file.mime.starts_with("image/")
        || path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "png" | "jpg" | "jpeg" | "webp" | "gif"
                )
            })
}
