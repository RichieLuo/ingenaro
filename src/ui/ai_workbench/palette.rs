use super::*;
use ingenaro::design::palette::{
    ColorEdit, ColorRole, HexColor, ImageColors, MAX_SWATCHES, Palette, PaletteProposal, Swatch,
    extract_colors, review_edits,
};

mod editor;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct PaletteEditor {
    open: bool,
    draft: Option<Vec<DraftRow>>,
    candidates: Option<ImageColors>,
    text: String,
    worker: Option<Worker>,
    changes: Vec<String>,
    conflict: Option<Conflict>,
    basis: Basis,
    notice: String,
    error: bool,
}

struct DraftRow {
    hex: String,
    role: ColorRole,
    locked: bool,
}

struct Conflict {
    lines: Vec<String>,
    unlocked: Palette,
}

#[derive(Default)]
struct Basis {
    text: String,
    links: Vec<String>,
    rationale: String,
}

enum Worker {
    Image(mpsc::Receiver<Result<Option<ImageColors>, String>>),
    Model(PaletteRun),
}

struct PaletteRun {
    running: ai::RunningSession,
    web_search: bool,
    reply: String,
    query: Option<String>,
    opened: Vec<String>,
    image_context: bool,
}

impl PaletteEditor {
    pub fn is_open(&self) -> bool {
        self.open
    }

    fn has_colors(&self) -> bool {
        self.draft.as_ref().is_some_and(|rows| !rows.is_empty())
    }

    fn valid_palette(&self) -> Result<Palette, String> {
        let colors = self
            .draft
            .iter()
            .flatten()
            .enumerate()
            .map(|(i, row)| {
                Ok(Swatch {
                    hex: HexColor::parse(&row.hex)
                        .map_err(|error| format!("色块 {} · {error}", i + 1))?,
                    role: row.role,
                    locked: row.locked,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let palette = Palette(colors);
        palette.validate()?;
        Ok(palette)
    }

    fn set_draft(&mut self, palette: &Palette) {
        self.draft = Some(
            palette
                .0
                .iter()
                .map(|row| DraftRow {
                    hex: row.hex.to_string(),
                    role: row.role,
                    locked: row.locked,
                })
                .collect(),
        );
    }

    fn report(&mut self, notice: impl Into<String>, error: bool) {
        self.notice = notice.into();
        self.error = error;
    }

    fn apply(&mut self, edits: &[ColorEdit]) -> Result<(), String> {
        let review = review_edits(&self.valid_palette()?, edits)?;
        self.set_draft(&review.draft);
        self.conflict = (!review.conflicts.is_empty()).then_some(Conflict {
            lines: review.conflicts,
            unlocked: review.unlocked_draft,
        });
        self.changes = review.changes;
        let notice = if self.conflict.is_some() {
            "部分建议涉及锁定色，请选择是否解锁。"
        } else if self.changes.is_empty() {
            "这次建议未改变草案。"
        } else {
            "建议已加入草案，可以继续编辑或确认。"
        };
        self.report(notice, false);
        Ok(())
    }

    fn resolve_conflict(&mut self, unlock: bool) {
        if let Some(conflict) = self.conflict.take() {
            if unlock {
                self.set_draft(&conflict.unlocked);
                self.changes.extend(conflict.lines);
                self.report("已解锁并修改草案，请确认项目配色。", false);
            } else {
                self.report("锁定色已保留，可以继续编辑或确认。", false);
            }
        }
    }

    fn receive_image_colors(&mut self, colors: ImageColors) -> Result<(), String> {
        self.candidates = Some(colors);
        if self.has_colors() {
            self.basis = Basis {
                text: "候选颜色提取自上传的参考图片。".into(),
                ..Basis::default()
            };
            self.report("请选择要补充的色块，或用候选替换未锁定色。", false);
            Ok(())
        } else {
            self.replace_unlocked_from_image()
        }
    }

    fn replace_unlocked_from_image(&mut self) -> Result<(), String> {
        let draft = self.valid_palette()?;
        let colors = self.candidates.as_ref().ok_or("请先提取图片颜色")?;
        self.basis = Basis {
            text: "来源为参考图片的本地取色。补充的背景或文字中性色已在候选中单独标记。".into(),
            ..Basis::default()
        };
        let locked: Vec<_> = draft.0.iter().filter(|row| row.locked).collect();
        let replacements = colors
            .suggested
            .0
            .iter()
            .filter(|row| !locked.iter().any(|locked| locked.role == row.role))
            .take(MAX_SWATCHES.saturating_sub(locked.len()));
        let edits: Vec<_> = draft
            .0
            .iter()
            .enumerate()
            .filter(|(_, row)| !row.locked)
            .map(|(i, _)| ColorEdit::Remove { index: i + 1 })
            .chain(replacements.map(|row| ColorEdit::Add {
                hex: row.hex,
                role: row.role,
            }))
            .collect();
        self.apply(&edits)
    }
}

impl App {
    fn ai_confirmed_palette(&self) -> Option<&Palette> {
        self.ai.settings.project_palettes.get(&self.ai.document_id)
    }

    pub(super) fn ai_palette_context(&self) -> String {
        let mut context = self.ai_confirmed_palette().map_or_else(
            || "\n项目未设置已确认的配色预设。沿用已有主题或按本轮需求选色。\n".into(),
            |palette| palette.design_context(self.ai.kind == design::ArtifactKind::Html),
        );
        context.push_str("聊天中的配色要求按用户指定的范围用于本轮设计。项目配色预设由用户在「项目配色」面板中管理。\n");
        context
    }

    fn ai_open_palette(&mut self) {
        if self.ai.palette.draft.is_none() {
            let confirmed = self.ai_confirmed_palette().cloned().unwrap_or_default();
            self.ai.palette.set_draft(&confirmed);
        }
        self.ai.palette.open = true;
    }

    fn ai_start_palette_images(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.ai.palette.report("正在提取参考颜色…", false);
        self.ai.palette.worker = Some(Worker::Image(rx));
        std::thread::spawn(move || {
            let result = images::reference_image_picker()
                .pick_file()
                .map(|path| {
                    let image =
                        ai::ImageAttachment::from_file(path).map_err(|error| error.to_string())?;
                    extract_colors(&[image])
                })
                .transpose();
            let _ = tx.send(result);
        });
    }

    fn ai_generate_palette(&mut self, research: bool) -> Result<(), String> {
        let prompt = self.ai.palette.text.trim().to_owned();
        if prompt.is_empty() || prompt.len() > 4096 {
            return Err("请输入不超过 4096 字节的配色描述。".into());
        }
        let base = self.ai.palette.valid_palette()?;
        let engine = self.ai_submission_engine()?;
        let web_search = research && engine.kind == ai::EngineKind::Codex;
        let image_colors = self.ai.palette.candidates.as_ref();
        let search = if web_search {
            "先尝试搜索并打开与主题有关的视觉参考。搜索只使用公开的主题关键词。sources 只填你实际打开并用于提案的 HTTPS 链接，查不到则留空。"
        } else {
            "基于描述与已有知识生成建议，sources 留空。"
        };
        let context = format!(
            "只为 Ingenaro 项目草案提出配色修改。保持用户要求保留的颜色，范围以本次描述为准；只补强调色时不替换现有颜色，只调整背景时不修改其他用途。整体重新配色时保留锁定色；用户明确指定修改某个锁定色时返回该修改，由宿主单独确认解锁。新方案建议主色、背景、文字和强调色，适合界面且文字/背景对比度至少 4.5:1。已有方案仅在用户要求补充中性色或提高对比度时调整其他用途。\n{search}\n仅输出一个 JSON 对象。示例 {{\"edits\":[{{\"op\":\"add\",\"role\":\"accent\",\"hex\":\"#E54B35\"}},{{\"op\":\"update\",\"index\":2,\"hex\":\"#FFF7EA\"}},{{\"op\":\"remove\",\"index\":3}}],\"sources\":[],\"rationale\":\"一句话说明主题的视觉特征与颜色选择，不写来源或链接\"}}。角色 primary, secondary, accent, background, text, neutral。index 从 1 开始，始终引用原草案行，每行至多修改一次；更新项可分别提供 hex 或 role。\n原草案（最多 {MAX_SWATCHES} 色）：{}\n图片本地提取候选（仅在用户要求时使用）：{}",
            serde_json::to_string(&base).unwrap_or_default(),
            serde_json::to_string(&image_colors.map(|colors| &colors.suggested))
                .unwrap_or_default(),
        );
        let working_dir = std::env::temp_dir()
            .join("rpshop-agent-workspaces")
            .join(&self.ai.document_id);
        std::fs::create_dir_all(&working_dir).map_err(|_| "无法创建配色任务工作目录")?;
        let running = ai::AiRuntime::start_palette(
            ai::StartRequest {
                config: engine,
                key: None,
                prompt,
                context,
                images: Vec::new(),
                working_dir,
                scope: self.ai_project_key(),
                resume: None,
                tools: self.ai.client.clone(),
                mcp: None,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
            web_search,
        )?;
        self.ai.palette.worker = Some(Worker::Model(PaletteRun {
            running,
            web_search,
            reply: String::new(),
            query: None,
            opened: Vec::new(),
            image_context: image_colors.is_some(),
        }));
        self.ai.palette.report(
            if web_search {
                "正在查找视觉参考并生成配色…"
            } else {
                "正在生成配色建议…"
            },
            false,
        );
        Ok(())
    }

    pub(super) fn ai_poll_palette(&mut self, ctx: &egui::Context) {
        let Some(worker) = self.ai.palette.worker.take() else {
            return;
        };
        let result = match worker {
            Worker::Image(rx) => match rx.try_recv() {
                Ok(Ok(Some(colors))) => self.ai.palette.receive_image_colors(colors),
                Ok(Ok(None)) => {
                    self.ai.palette.report("可以继续编辑配色。", false);
                    Ok(())
                }
                Ok(Err(error)) => Err(error),
                Err(mpsc::TryRecvError::Disconnected) => Err("无法提取图片颜色，请重试。".into()),
                Err(mpsc::TryRecvError::Empty) => {
                    self.ai.palette.worker = Some(Worker::Image(rx));
                    Ok(())
                }
            },
            Worker::Model(mut run) => match run.poll() {
                None => {
                    self.ai.palette.worker = Some(Worker::Model(run));
                    Ok(())
                }
                Some(Ok(mut proposal)) => (|| {
                    let preserved = preserved_roles(&self.ai.palette.text);
                    let base = self.ai.palette.valid_palette()?;
                    proposal.edits.retain(|edit| {
                        let role = match *edit {
                            ColorEdit::Add { role, .. } => Some(role),
                            ColorEdit::Update {
                                role: Some(role), ..
                            } if preserved.contains(&role) => return false,
                            ColorEdit::Update { index, .. } | ColorEdit::Remove { index } => {
                                base.0.get(index.wrapping_sub(1)).map(|row| row.role)
                            }
                        };
                        role.is_none_or(|role| !preserved.contains(&role))
                    });
                    self.ai.palette.apply(&proposal.edits)?;
                    self.ai.palette.basis = run.basis(proposal);
                    self.ai.palette.text.clear();
                    Ok(())
                })(),
                Some(Err(_)) if run.web_search => self.ai_generate_palette(false).map(|()| {
                    self.ai
                        .palette
                        .report("搜索暂不可用，正在根据描述生成配色…", false);
                }),
                Some(Err(error)) => Err(error),
            },
        };
        if let Err(error) = result {
            self.ai.palette.report(error, true);
        }
        if self.ai.palette.worker.is_some() {
            ctx.request_repaint_after(Duration::from_millis(32));
        }
    }

    fn ai_confirm_palette(&mut self) -> Result<(), String> {
        if self.ai.running.is_some()
            || self.ai.palette.worker.is_some()
            || self.ai.palette.conflict.is_some()
        {
            return Err("请先等待当前任务结束并处理锁定色冲突。".into());
        }
        if self.ai.config_load_error.is_some() {
            return Err("AI 设置读取失败。请在 AI 设置中重新读取配置后保存。".into());
        }
        let palette = self.ai.palette.valid_palette()?;
        let key = self.ai_project_key();
        let mut settings = self.ai.settings.clone();
        if palette.0.is_empty() {
            settings.project_palettes.remove(&key);
        } else {
            settings.project_palettes.insert(key.clone(), palette);
        }
        if let Some(path) = self.ai_document_path_key() {
            settings.project_document_ids.insert(path, key);
        }
        settings.validate()?;
        if self.ai.persist {
            settings.save_default()?;
        }
        self.ai.settings = settings;
        self.ai.palette.changes.clear();
        self.ai.palette.open = false;
        self.ai.state = if self.ai_confirmed_palette().is_some() {
            "项目配色已确认，后续设计将使用该方案。".into()
        } else {
            "项目配色已清除。".into()
        };
        Ok(())
    }
}

impl PaletteRun {
    fn poll(&mut self) -> Option<Result<PaletteProposal, String>> {
        for _ in 0..256 {
            match self.running.events.try_recv() {
                Ok(ai::AiEvent::TextDelta { text }) => {
                    if self.reply.len() + text.len() > 32 * 1024 {
                        return Some(Err("配色建议过长，请缩短描述后重试。".into()));
                    }
                    self.reply.push_str(&text);
                }
                Ok(ai::AiEvent::PaletteSearch { query, url }) => {
                    if self.query.is_none() {
                        self.query = query.map(|text| text.chars().take(150).collect());
                    }
                    if self.opened.len() < 20
                        && let Some(link) = url.as_deref().and_then(reference_link)
                        && !self.opened.contains(&link)
                    {
                        self.opened.push(link);
                    }
                }
                Ok(ai::AiEvent::Error { message }) => return Some(Err(message)),
                Ok(ai::AiEvent::Complete { cancelled: false }) => {
                    return Some(PaletteProposal::parse(&self.reply));
                }
                Ok(ai::AiEvent::Complete { cancelled: true }) => {
                    return Some(Err("配色建议已停止。".into()));
                }
                Ok(_) => {}
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Some(Err("配色任务意外结束，请重试。".into()));
                }
            }
        }
        None
    }

    fn basis(&self, proposal: PaletteProposal) -> Basis {
        let mut links = Vec::new();
        for link in proposal
            .sources
            .iter()
            .filter_map(|source| reference_link(source))
        {
            if self.opened.contains(&link) && !links.contains(&link) {
                links.push(link);
                if links.len() == 6 {
                    break;
                }
            }
        }
        let mut text = if !links.is_empty() {
            "参考了实际打开的网页和你的描述。".to_owned()
        } else if let Some(query) = &self.query {
            format!("搜索了「{query}」，未找到可核对的来源。建议依据模型已有知识和你的描述。")
        } else {
            "建议依据模型已有知识和你的描述。".to_owned()
        };
        if self.image_context {
            text.push_str(" 本轮输入包含参考图的本地取色。");
        }
        Basis {
            text,
            links,
            rationale: proposal.rationale.chars().take(350).collect(),
        }
    }
}

fn reference_link(text: &str) -> Option<String> {
    if text.len() > 2048 {
        return None;
    }
    let mut url = reqwest::Url::parse(text).ok()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    url.set_fragment(None);
    Some(url.to_string())
}

fn preserved_roles(prompt: &str) -> Vec<ColorRole> {
    let text = prompt.to_lowercase();
    [
        (
            ColorRole::Primary,
            &["保留主色", "主色保留", "主色不变", "keep the primary"][..],
        ),
        (
            ColorRole::Secondary,
            &[
                "保留辅助色",
                "辅助色保留",
                "辅助色不变",
                "keep the secondary",
            ],
        ),
        (
            ColorRole::Accent,
            &["保留强调色", "强调色保留", "强调色不变", "keep the accent"],
        ),
        (
            ColorRole::Background,
            &["保留背景", "背景保留", "背景不变", "keep the background"],
        ),
        (
            ColorRole::Text,
            &[
                "保留文字色",
                "文字色保留",
                "文字色不变",
                "keep the text color",
            ],
        ),
        (
            ColorRole::Neutral,
            &["保留中性色", "中性色保留", "中性色不变", "keep the neutral"],
        ),
    ]
    .into_iter()
    .filter_map(|(role, phrases)| {
        phrases
            .iter()
            .any(|phrase| {
                text.match_indices(phrase)
                    .any(|(start, _)| !text[..start].ends_with("不"))
            })
            .then_some(role)
    })
    .collect()
}
