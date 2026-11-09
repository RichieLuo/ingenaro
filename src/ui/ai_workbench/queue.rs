//! User-authored follow-ups: FIFO, explicit pause/cancel, context and attachments captured per submission.
use super::*;
use std::collections::VecDeque;
#[cfg(test)]
mod cli_tests;
#[cfg(test)]
mod tests;

const QUEUE_HEADER_ROW_HEIGHT: f32 = 22.0;
const QUEUE_ITEM_SPACING: f32 = 3.0;
const QUEUE_ROW_HEIGHT: f32 = 30.0;

fn queue_header_height(ui: &egui::Ui) -> f32 {
    QUEUE_HEADER_ROW_HEIGHT + QUEUE_ITEM_SPACING + ui.spacing().item_spacing.y
}

#[derive(Clone)]
pub(super) struct Submission {
    pub id: u64,
    pub prompt: String,
    pub context: serde_json::Value,
    pub scope: String,
    pub engine: ai::EngineConfig,
    pub generation: String,
    pub kind: design::ArtifactKind,
    pub context_mode: usize,
    pub read_only: bool,
    pub images: Vec<ai::ImageAttachment>,
    pub reference_files: Vec<automation::ReferenceFile>,
}
impl Submission {
    fn context_bytes(&self) -> usize {
        serde_json::to_vec(&self.context).map_or(usize::MAX / 32, |bytes| bytes.len())
            + self.prompt.len()
            + self
                .reference_files
                .iter()
                .map(automation::ReferenceFile::byte_len)
                .sum::<usize>()
    }
}
#[derive(Default)]
pub(super) struct MessageQueue {
    entries: VecDeque<Submission>,
    next_id: u64,
    pub paused: bool,
}
impl MessageQueue {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn entries(&self) -> impl Iterator<Item = &Submission> {
        self.entries.iter()
    }
    pub fn push(&mut self, mut submission: Submission) -> Result<u64, String> {
        if self.entries.len() >= 16 {
            return Err("队列已达 16 条，请等待或移除已有需求".into());
        }
        let queued = self
            .entries
            .iter()
            .map(Submission::context_bytes)
            .sum::<usize>();
        if submission.context_bytes().saturating_add(queued) > 16 * 1024 * 1024 {
            return Err("队列引用累计超过 16 MiB，请先处理已有需求".into());
        }
        let total = self
            .entries
            .iter()
            .flat_map(|item| &item.images)
            .map(ai::ImageAttachment::byte_len)
            .sum::<usize>();
        if total
            + submission
                .images
                .iter()
                .map(ai::ImageAttachment::byte_len)
                .sum::<usize>()
            > 24 * 1024 * 1024
        {
            return Err("队列图片累计超过 24 MiB，请先发送已有内容".into());
        }
        self.next_id = self.next_id.saturating_add(1);
        submission.id = self.next_id;
        let id = submission.id;
        self.entries.push_back(submission);
        Ok(id)
    }
    pub fn pop(&mut self) -> Option<Submission> {
        if self.paused {
            None
        } else {
            self.entries.pop_front()
        }
    }
    pub fn remove(&mut self, id: u64) {
        self.entries.retain(|entry| entry.id != id);
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.paused = false;
    }
}

impl App {
    pub(super) fn ai_submission_engine(&self) -> Result<ai::EngineConfig, String> {
        let mut engine = self
            .ai
            .settings
            .engines
            .iter()
            .find(|engine| engine.id == self.ai.selected_engine)
            .cloned()
            .ok_or("请先选择本地智能体")?;
        if let Some(model) = &self.ai.model_override {
            engine.model = model.clone();
        }
        engine.reasoning_effort = self.ai.effort_override.resolve(engine.reasoning_effort);
        if let Some(state) = self.ai.catalogs.get(&engine.id) {
            let model = state
                .catalog
                .models
                .iter()
                .find(|model| model.id == engine.model)
                .or_else(|| {
                    if engine.model.is_empty() {
                        state
                            .catalog
                            .models
                            .iter()
                            .find(|model| model.is_default || model.id == "default")
                    } else {
                        None
                    }
                });
            if let Some(model) = model
                && engine.reasoning_effort.is_some_and(|effort| {
                    !model
                        .reasoning_efforts
                        .iter()
                        .any(|value| value == effort.as_str())
                })
            {
                engine.reasoning_effort = None;
            }
        }
        Ok(engine)
    }

    pub(super) fn ai_capture_submission(&mut self) -> Result<Submission, String> {
        self.ai_observe_selection();
        let engine = self.ai_submission_engine()?;
        if self.ai_image_loading() {
            return Err("图片仍在读取，完成后再发送，避免遗漏附件".into());
        }
        if self.ai.reference_files.loading() {
            return Err("参考文件仍在读取，完成后再发送".into());
        }
        ai::validate_image_attachments(&self.ai.images).map_err(|error| error.to_string())?;
        if self.ai.prompt.len() > 128 * 1024 {
            return Err("单条需求超过 128 KiB，请缩短后发送".into());
        }
        let prompt = if self.ai.prompt.trim().is_empty() && !self.ai.images.is_empty() {
            "请根据附图设计或修改当前原型。".into()
        } else {
            self.ai.prompt.clone()
        };
        if prompt.trim().is_empty() {
            return Err("请先输入需求或添加图片。使用参考文件时请说明设计需求".into());
        }
        let context = self.ai_reference_context();
        let context_bytes = serde_json::to_vec(&context)
            .map_err(|_| "无法编码当前引用")?
            .len();
        if context_bytes > 1536 * 1024 {
            return Err("引用内容较大，请改为当前页面或选择范围后发送".into());
        }
        Ok(Submission {
            id: 0,
            prompt,
            context,
            scope: self.ai_project_key(),
            engine,
            generation: self.ai.generation.clone(),
            kind: self.ai.kind,
            context_mode: if self.ai.references.selected.is_some() {
                2
            } else {
                self.ai.context
            },
            read_only: self.ai.discuss_only,
            images: self.ai.images.clone(),
            reference_files: self.ai.reference_files.files.clone(),
        })
    }
    pub(super) fn ai_submit(&mut self) {
        let submission = match self.ai_capture_submission() {
            Ok(submission) => submission,
            Err(error) => {
                self.ai.state = error;
                if self
                    .ai
                    .settings
                    .engines
                    .iter()
                    .all(|engine| engine.id != self.ai.selected_engine)
                {
                    self.ai.settings_open = true;
                }
                return;
            }
        };
        if self.ai.authorization.mode() == ai::PermissionMode::Full
            && !self.ai_has_full_permission()
        {
            self.ai.authorization.confirm_before_send();
            return;
        }
        if self.ai.queue.len() == 0 && self.ai.running.is_none() && self.ai.steering.is_none() {
            self.ai.queue.paused = false;
        }
        match self.ai.queue.push(submission) {
            Ok(_) => {
                self.ai.prompt.clear();
                self.ai.images.clear();
                self.ai.reference_files.files.clear();
                self.ai.state = format!("已加入队列 · {} 条待执行", self.ai.queue.len());
                self.ai_poll_queue();
            }
            Err(error) => self.ai.state = error,
        }
    }
    pub(super) fn ai_poll_queue(&mut self) {
        if self.ai.running.is_some()
            || self.ai.steering.is_some()
            || self.ai.queue.paused
            || self.confirm.is_some()
            || self.ai_modal_active()
            || self.ai.settings_open
            || self.drag.is_some()
            || self.editing.is_some()
        {
            return;
        }
        let Some(submission) = self.ai.queue.pop() else {
            return;
        };
        if submission.scope != self.ai_project_key()
            || submission.generation != self.ai.generation
            || submission.engine.id != self.ai.selected_engine
        {
            self.ai.state = "队列上下文已变化，已暂停，未发送到错误的工程或引擎".into();
            self.ai.queue.entries.push_front(submission);
            self.ai.queue.paused = true;
            return;
        }
        if !self.ai_begin(submission.clone()) {
            // Preserve failed-to-start input for an explicit retry rather than losing it.
            self.ai.queue.entries.push_front(submission);
            self.ai.queue.paused = true;
        }
    }
    pub(super) fn ai_queue_height(&self, ui: &egui::Ui, available: f32) -> f32 {
        let header = queue_header_height(ui);
        if self.ai.queue.len() == 0 || available <= header {
            0.0
        } else {
            (header + self.ai.queue.len().min(3) as f32 * QUEUE_ROW_HEIGHT).min(available)
        }
    }

    pub(super) fn ai_edit_queued(&mut self, id: u64) {
        // Never overwrite an unsent draft or discard its attachments.
        if !self.ai.prompt.trim().is_empty()
            || !self.ai.images.is_empty()
            || self.ai_image_loading()
            || !self.ai.reference_files.files.is_empty()
            || self.ai.reference_files.loading()
        {
            self.ai.state = "请先发送或清空当前草稿，再编辑待发送消息".into();
            return;
        }
        let Some(entry) = self
            .ai
            .queue
            .entries()
            .find(|entry| entry.id == id)
            .cloned()
        else {
            return;
        };
        if entry.scope != self.ai_project_key()
            || entry.generation != self.ai.generation
            || entry.engine.id != self.ai.selected_engine
        {
            self.ai.state = "这条消息不属于当前工程或会话，不能恢复到输入框".into();
            return;
        }
        self.ai.queue.remove(id);
        self.ai.prompt = entry.prompt;
        self.ai.images = entry.images;
        self.ai.reference_files.files = entry.reference_files;
        // Editing deliberately recaptures references on the next send, rather than
        // silently keeping an invisible old selection snapshot.
        self.ai.state = "已恢复到输入框 · 再次发送时使用当前引用、模型和权限".into();
    }

    pub(super) fn ai_queue_panel(&mut self, ui: &mut egui::Ui, height: f32) {
        if height == 0.0 {
            return;
        }
        let list_height = height - queue_header_height(ui);
        let mut remove = None;
        let mut edit = None;
        let mut steer = None;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, QUEUE_ITEM_SPACING);
            ui.spacing_mut().interact_size.y = QUEUE_HEADER_ROW_HEIGHT;
            ui.spacing_mut().button_padding = vec2(4.0, 1.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} · {} 条",
                        if self.ai.queue.paused {
                            "发送已暂停"
                        } else {
                            "等待发送"
                        },
                        self.ai.queue.len()
                    ))
                    .size(10.0)
                    .color(MUTED),
                );
                if self.ai.queue.paused && ui.small_button("继续发送").clicked() {
                    self.ai.queue.paused = false;
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("ai-pending-queue")
                .max_height(list_height)
                .min_scrolled_height(0.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (index, item) in self.ai.queue.entries().enumerate() {
                        ui.push_id(item.id, |ui| {
                            egui::Frame::new()
                                .fill(Color32::from_rgb(248, 248, 251))
                                .corner_radius(5)
                                .inner_margin(egui::Margin::symmetric(7, 3))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(format!("{}", index + 1))
                                                .size(10.0)
                                                .color(MUTED),
                                        );
                                        let width = (ui.available_width() - 128.0).max(20.0);
                                        ui.add_sized(
                                            [width, 20.0],
                                            egui::Label::new(
                                                RichText::new(&item.prompt).size(11.0).color(INK),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(
                                            format!(
                                                "{}
图片 {} 张 · 发送时的引用已固定",
                                                item.prompt,
                                                item.images.len()
                                            ),
                                        );
                                        let response = ui.add_enabled(
                                            self.ai.running.is_some()
                                                && item.engine.kind.supports_steering()
                                                && self.ai.steering.is_none()
                                                && !self.ai.page_agents.has_active()
                                                && item.reference_files.is_empty(),
                                            egui::Button::new(RichText::new("引导对话").size(10.0)).frame(false),
                                        ).on_hover_text("立即将这条消息发送给当前任务；引擎确认后从队列移除");
                                        #[cfg(test)]
                                        ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(("ai-queue-steer", item.id)), response.rect));
                                        if response.clicked() { steer = Some(item.id); }
                                        let response = ui
                                            .add_enabled(
                                                self.ai.steering.is_none(),
                                                egui::Button::new(RichText::new("✎").color(MUTED))
                                                    .frame(false),
                                            )
                                            .on_hover_text("恢复到输入框编辑；再次发送时更新引用");
                                        #[cfg(test)]
                                        ui.ctx().data_mut(|data| {
                                            data.insert_temp(
                                                egui::Id::new(("ai-queue-edit", item.id)),
                                                response.rect,
                                            )
                                        });
                                        if response.clicked() {
                                            edit = Some(item.id);
                                        }
                                        let response = ui
                                            .add_enabled(
                                                self.ai.steering.is_none(),
                                                egui::Button::new(RichText::new("×").color(MUTED))
                                                    .frame(false),
                                            )
                                            .on_hover_text("撤回这条待发送消息");
                                        #[cfg(test)]
                                        ui.ctx().data_mut(|data| {
                                            data.insert_temp(
                                                egui::Id::new(("ai-queue-remove", item.id)),
                                                response.rect,
                                            )
                                        });
                                        if response.clicked() {
                                            remove = Some(item.id);
                                        }
                                    });
                                });
                        });
                    }
                });
        });
        if let Some(id) = steer {
            self.ai_steer_queued(id);
        }
        if let Some(id) = remove {
            self.ai.queue.remove(id);
        }
        if let Some(id) = edit {
            self.ai_edit_queued(id);
            if let Some(editor) = self.ai.prompt_editor_id {
                ui.memory_mut(|memory| memory.request_focus(editor));
            }
        }
    }
}
