use super::*;
use automation::{MAX_REFERENCE_BYTES, MAX_REFERENCE_FILES, ReferenceFile};

#[derive(Default)]
pub(super) struct Draft {
    pub files: Vec<ReferenceFile>,
    loading: Option<mpsc::Receiver<Result<Vec<ReferenceFile>, String>>>,
}

impl Draft {
    pub fn loading(&self) -> bool {
        self.loading.is_some()
    }
}

impl App {
    pub(super) fn ai_pick_reference_files(&mut self) {
        let draft = &mut self.ai.reference_files;
        if draft.loading() {
            return;
        }
        let remaining = MAX_REFERENCE_FILES - draft.files.len();
        if remaining == 0 {
            self.ai.state = format!("最多可添加 {MAX_REFERENCE_FILES} 个参考文件");
            return;
        }
        let (tx, rx) = mpsc::channel();
        draft.loading = Some(rx);
        std::thread::spawn(move || {
            let result = (|| {
                let paths = rfd::FileDialog::new()
                    .set_title("选择本轮交给 AI 读取的参考文件")
                    .pick_files()
                    .unwrap_or_default();
                if paths.len() > remaining {
                    return Err(format!("还可添加 {remaining} 个参考文件"));
                }
                paths.iter().map(|path| ReferenceFile::open(path)).collect()
            })();
            let _ = tx.send(result);
        });
    }

    pub(super) fn ai_poll_reference_files(&mut self) {
        let draft = &mut self.ai.reference_files;
        let Some(receiver) = &draft.loading else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("参考文件读取中断，请重试".into()),
        };
        draft.loading = None;
        match result {
            Ok(files) if files.is_empty() => {}
            Ok(files) => {
                let bytes = draft
                    .files
                    .iter()
                    .chain(&files)
                    .map(ReferenceFile::byte_len)
                    .sum::<usize>();
                if bytes > MAX_REFERENCE_BYTES {
                    self.ai.state = "参考文件合计超过 2 MiB，请减少文件后重试".into();
                    return;
                }
                self.ai.state =
                    format!("已添加 {} 个参考文件，发送后所选引擎可以读取", files.len());
                draft.files.extend(files);
            }
            Err(error) => self.ai.state = error,
        }
    }

    pub(super) fn ai_reference_files_height(&self) -> f32 {
        let draft = &self.ai.reference_files;
        if draft.files.is_empty() && !draft.loading() {
            0.0
        } else {
            28.0
        }
    }

    pub(super) fn ai_reference_files_strip(&mut self, ui: &mut egui::Ui) {
        if self.ai_reference_files_height() == 0.0 {
            return;
        }
        let draft = &mut self.ai.reference_files;
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(10, 0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().interact_size.y = 28.0;
                    if !draft.files.is_empty() {
                        let mut remove = None;
                        ui.menu_button(format!("参考文件 · {}", draft.files.len()), |ui| {
                            ui.set_width(220.0);
                            for (index, file) in draft.files.iter().enumerate() {
                                ui.push_id(index, |ui| {
                                    ui.horizontal(|ui| {
                                        if ui
                                            .small_button("×")
                                            .on_hover_text("移除参考文件")
                                            .clicked()
                                        {
                                            remove = Some(index);
                                        }
                                        ui.add(egui::Label::new(&file.name).truncate())
                                            .on_hover_text(&file.name);
                                    });
                                });
                            }
                        })
                        .response
                        .on_hover_text("查看或移除本轮参考文件");
                        if let Some(index) = remove {
                            draft.files.remove(index);
                        }
                    }
                    if draft.loading() {
                        ui.spinner();
                        ui.label("正在选择或读取文件…");
                    }
                });
            });
    }
}
