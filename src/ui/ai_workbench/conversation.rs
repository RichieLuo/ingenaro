//! Readable native message presentation; transport/history remain plain, unmodified content.
mod table;

use super::*;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag};

const BLOCK_OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS);
const VIEWER_OPTIONS: Options = BLOCK_OPTIONS
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_DEFINITION_LIST);

#[derive(Default)]
pub(super) struct ConversationView {
    markdown: CommonMarkCache,
}

pub(super) fn markdown(ui: &mut egui::Ui, cache: &mut CommonMarkCache, text: &str) {
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width());
        ui.style_mut().visuals.override_text_color = Some(INK);
        ui.style_mut().visuals.extreme_bg_color = Color32::from_rgb(246, 247, 250);
        ui.style_mut().visuals.code_bg_color = Color32::from_rgb(239, 241, 246);
        ui.style_mut().url_in_tooltip = true;
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.spacing_mut().item_spacing = vec2(5.0, 7.0);
        ui.spacing_mut().button_padding = vec2(7.0, 5.0);
        for (style, size) in [
            (egui::TextStyle::Body, 13.0),
            (egui::TextStyle::Heading, 17.0),
            (egui::TextStyle::Monospace, 12.0),
        ] {
            let family = if style == egui::TextStyle::Monospace {
                egui::FontFamily::Monospace
            } else {
                egui::FontFamily::Proportional
            };
            ui.style_mut()
                .text_styles
                .insert(style, FontId::new(size, family));
        }
        cache.link_hooks_clear();
        let mut events = Parser::new_ext(text, VIEWER_OPTIONS).into_offset_iter();
        let mut tables = Vec::new();
        while let Some((event, range)) = events.next() {
            if let Event::Start(Tag::Table(alignments)) = event {
                let table = table::Table::read(ui, alignments, events.by_ref().map(|(e, _)| e));
                tables.push((range, table));
            }
        }

        // Route parsed tables through the viewer's HTML block hook. A render-only
        // marker keeps lists, quotes and reference definitions in their original context.
        let mut marker = String::from("<!--rpshop-chat-table:");
        let mut source = String::new();
        let text = if tables.is_empty() {
            text
        } else {
            while text.contains(&marker) {
                marker.push('_');
            }
            let mut end = 0;
            for (index, (range, _)) in tables.iter().enumerate() {
                use std::fmt::Write as _;
                source.push_str(&text[end..range.start]);
                write!(&mut source, "{marker}{index}-->").expect("write to String");
                if text[..range.end].ends_with('\n') {
                    source.push('\n');
                }
                end = range.end;
            }
            source.push_str(&text[end..]);
            &source
        };
        let has_tables = !tables.is_empty();
        let render_html = move |ui: &mut egui::Ui, html: &str| {
            let index = html
                .trim()
                .strip_prefix(&marker)
                .and_then(|html| html.strip_suffix("-->"))
                .and_then(|index| index.parse::<usize>().ok());
            if let Some(index) = index
                && let Some((_, table)) = tables.get(index)
            {
                // A table is a block, including when the viewer is inside a list item.
                if ui.cursor().left() > ui.max_rect().left() + 0.5 {
                    ui.end_row();
                }
                ui.push_id(index, |ui| table.show(ui));
                ui.end_row();
            } else {
                ui.label(html);
            }
        };
        let show_block = |ui: &mut egui::Ui, cache: &mut CommonMarkCache, source: &str| {
            // Check the same source the viewer parses, including links in extension blocks.
            for event in Parser::new_ext(source, VIEWER_OPTIONS) {
                if let Event::Start(Tag::Link { dest_url, .. }) = event
                    && !safe_link(&dest_url)
                {
                    cache.add_link_hook(dest_url.as_ref());
                }
            }
            // No file, HTTP or data-image loaders are enabled for CommonMark.
            CommonMarkViewer::new()
                .explicit_image_uri_scheme(true)
                .render_html_fn(
                    has_tables.then_some(&render_html as &egui_commonmark::RenderHtmlFn),
                )
                .show(ui, cache, source);
        };

        // Render top-level blocks independently: the library inserts a full blank
        // text row before headings, too loose for a 280–420px desktop sidebar.
        let parser = Parser::new_ext(text, BLOCK_OPTIONS);
        if parser.reference_definitions().iter().next().is_some() {
            // Cross-block reference links need the complete document.
            show_block(ui, cache, text);
            return;
        }
        let mut depth = 0usize;
        let mut start = 0;
        let mut heading = None;
        let mut title = String::new();
        for (event, range) in parser.into_offset_iter() {
            match event {
                Event::Start(tag) => {
                    if depth == 0 {
                        start = range.start;
                        heading = match tag {
                            Tag::Heading { level, .. } => Some(level),
                            _ => None,
                        };
                        title.clear();
                    }
                    depth += 1;
                }
                Event::End(_) => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        if let Some(level) = heading {
                            let size = match level {
                                HeadingLevel::H1 => 18.0,
                                HeadingLevel::H2 => 16.0,
                                _ => 14.0,
                            };
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&title).size(size).strong().color(INK),
                                )
                                .wrap()
                                .selectable(true),
                            );
                        } else {
                            ui.push_id(start, |ui| {
                                show_block(ui, cache, &text[start..range.end]);
                            });
                        }
                        ui.add_space(3.0);
                    }
                }
                Event::Text(part) | Event::Code(part) if heading.is_some() => title.push_str(&part),
                Event::SoftBreak | Event::HardBreak if heading.is_some() => title.push(' '),
                Event::Rule if depth == 0 => {
                    ui.separator();
                }
                Event::Html(part) if depth == 0 => {
                    ui.label(part.as_ref());
                }
                _ => {}
            }
        }
    });
}

fn safe_link(url: &str) -> bool {
    !url.chars().any(char::is_control)
        && (url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:"))
}

impl App {
    pub(super) fn ai_conversation(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let kind = self
            .ai
            .settings
            .engines
            .iter()
            .find(|engine| engine.id == self.ai.selected_engine)
            .map(|engine| engine.kind);
        for (index, (role, text)) in self.ai.messages.iter().enumerate() {
            ui.push_id(("ai-chat-message", index), |ui| {
                if role == "你" {
                    user_message(ui, text, width);
                } else {
                    if index + 1 == self.ai.messages.len()
                        && self.ai.running.is_none()
                        && self.ai.streamed.is_empty()
                    {
                        reasoning(ui, &self.ai.reasoning, false);
                    }
                    assistant_header(ui, kind, false);
                    markdown(ui, &mut self.ai.conversation.markdown, text);
                    message_actions(ui, text, false);
                }
            });
            ui.add_space(12.0);
        }
        if self.ai.running.is_some() || !self.ai.streamed.is_empty() {
            ui.push_id("ai-current-response", |ui| {
                reasoning(ui, &self.ai.reasoning, self.ai.running.is_some());
                assistant_header(ui, kind, self.ai.running.is_some());
                if self.ai.streamed.is_empty() {
                    ui.label(RichText::new("正在处理你的需求…").size(12.0).color(MUTED));
                } else {
                    markdown(ui, &mut self.ai.conversation.markdown, &self.ai.streamed);
                    if self.ai.running.is_none() {
                        message_actions(ui, &self.ai.streamed, false);
                    }
                }
            });
            ui.add_space(16.0);
        }
        if !self.ai.changes.is_empty() {
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = 5.0;
                egui::CollapsingHeader::new(
                    RichText::new(format!("工具与变更 · {}", self.ai.changes.len()))
                        .size(11.0)
                        .color(MUTED),
                )
                .id_salt("ai-conversation-changes")
                .show(ui, |ui| {
                    for change in self.ai.changes.iter().rev().take(50) {
                        ui.horizontal(|ui| {
                            let color = if change == "工具执行失败" {
                                Color32::from_rgb(184, 65, 65)
                            } else {
                                MUTED
                            };
                            let (rect, _) = ui.allocate_exact_size(vec2(9.0, 15.0), Sense::hover());
                            ui.painter().circle_filled(rect.center(), 2.0, color);
                            ui.add(
                                egui::Label::new(RichText::new(change).size(11.0).color(color))
                                    .wrap(),
                            );
                        });
                    }
                });
            });
        }
    }
}

fn user_message(ui: &mut egui::Ui, text: &str, width: f32) {
    ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
        ui.set_width(width);
        let bubble = egui::Frame::new()
            .fill(Color32::from_rgb(242, 240, 251))
            .stroke(Stroke::new(1.0, Color32::from_rgb(233, 229, 245)))
            .corner_radius(egui::CornerRadius {
                nw: 12,
                ne: 12,
                sw: 12,
                se: 3,
            })
            .inner_margin(egui::Margin::symmetric(13, 10))
            .show(ui, |ui| {
                ui.set_max_width((width * 0.85 - 26.0).max(60.0));
                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(text)
                                .size(13.0)
                                .line_height(Some(21.0))
                                .color(INK),
                        )
                        .wrap()
                        .selectable(true),
                    );
                });
            });
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(egui::Id::new("ai-user-bubble-test"), bubble.response.rect)
        });
        let _ = bubble;
        message_actions(ui, text, true);
    });
}

fn assistant_header(ui: &mut egui::Ui, kind: Option<ai::EngineKind>, streaming: bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if let Some(kind) = kind {
            engine_icons::inline(ui, kind, 16.0);
        }
        ui.label(
            RichText::new("RPShop 助手")
                .size(11.0)
                .strong()
                .color(Color32::from_rgb(100, 108, 126)),
        );
        if streaming {
            ui.label(RichText::new("正在回复").size(10.0).color(MUTED));
        }
    });
    ui.add_space(4.0);
}

fn message_actions(ui: &mut egui::Ui, text: &str, user: bool) {
    if text.trim().is_empty() {
        return;
    }
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 20.0;
        ui.spacing_mut().button_padding = vec2(3.0, 1.0);
        let id = ui.id().with("ai-message-copy");
        let copied = ui
            .ctx()
            .data(|data| data.get_temp::<f64>(id))
            .is_some_and(|until| ui.input(|i| i.time) < until);
        let response = ui
            .add(
                egui::Button::new(
                    RichText::new(if copied { "已复制" } else { "复制" })
                        .size(10.0)
                        .color(MUTED),
                )
                .frame(false),
            )
            .on_hover_text(if user {
                "复制这条需求"
            } else {
                "复制回复原文（Markdown）"
            });
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(egui::Id::new("ai-message-copy-test"), response.rect)
        });
        if response.clicked() {
            ui.ctx().copy_text(text.to_owned());
            let until = ui.input(|i| i.time) + 1.5;
            ui.ctx().data_mut(|data| data.insert_temp(id, until));
        }
    });
}

fn reasoning(ui: &mut egui::Ui, text: &str, streaming: bool) {
    if text.trim().is_empty() {
        return;
    }
    egui::CollapsingHeader::new(
        RichText::new(if streaming {
            "思考摘要 · 实时"
        } else {
            "思考摘要"
        })
        .size(11.0)
        .color(MUTED),
    )
    .id_salt("ai-public-reasoning")
    .default_open(streaming)
    .show(ui, |ui| {
        let response = egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 11,
                right: 4,
                top: 4,
                bottom: 5,
            })
            .show(ui, |ui| {
                ui.label(
                    RichText::new(text)
                        .size(12.0)
                        .color(Color32::from_rgb(127, 134, 150)),
                );
                ui.label(RichText::new("CLI 公开摘要").size(9.0).color(MUTED))
                    .on_hover_text("仅展示引擎公开提供的摘要，不代表隐藏的完整推理。");
            });
        let rect = response.response.rect;
        ui.painter().line_segment(
            [rect.left_top(), rect.left_bottom()],
            Stroke::new(2.0, Color32::from_rgb(222, 218, 235)),
        );
    });
    ui.add_space(6.0);
}

#[cfg(test)]
mod tests;
