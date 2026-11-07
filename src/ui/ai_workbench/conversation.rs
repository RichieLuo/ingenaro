//! Readable native message presentation; transport/history remain plain, unmodified content.
mod table;

use super::*;
use design::{ChatActivity, ChatRole, ChatStep};
use egui::text_selection::LabelSelectionState;
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
        let response = &self.ai.response;
        let streaming = self.ai.running.is_some();
        let show_response = streaming || !response.text.is_empty() || !response.steps.is_empty();
        if self.ai.messages.is_empty() && !show_response {
            return;
        }
        let width = ui.available_width();
        let copy = begin_message_selection(ui);
        ui.push_id(&self.ai.generation, |ui| {
            for (index, message) in self.ai.messages.iter().enumerate() {
                ui.push_id(index, |ui| {
                    if message.role == ChatRole::User {
                        user_message(ui, &message.text, width);
                    } else {
                        assistant_message(
                            ui,
                            &mut self.ai.conversation.markdown,
                            &message.text,
                            &message.steps,
                            false,
                        );
                    }
                });
                ui.add_space(12.0);
            }
            if show_response {
                ui.push_id(self.ai.messages.len(), |ui| {
                    assistant_message(
                        ui,
                        &mut self.ai.conversation.markdown,
                        &response.text,
                        &response.steps,
                        streaming,
                    );
                });
                ui.add_space(12.0);
            }
        });
        if copy {
            ui.input_mut(|input| {
                input
                    .events
                    .retain(|event| !matches!(event, egui::Event::Copy))
            });
        }
    }
}

fn assistant_message(
    ui: &mut egui::Ui,
    cache: &mut CommonMarkCache,
    text: &str,
    steps: &[ChatStep],
    streaming: bool,
) {
    ui.scope(|ui| {
        let mut start = 0;
        for group in steps.chunk_by(|a, b| a.after_text == b.after_text) {
            let end = group[0].after_text;
            if start < end {
                let segment = ui.push_id(("reply", start), |ui| {
                    markdown(ui, cache, &text[start..end])
                });
                message_context_menu(ui, &segment.response, text);
            }
            process(ui, group, streaming, streaming && end == text.len());
            start = end;
        }
        if start < text.len() {
            let segment = ui.push_id(("reply", start), |ui| markdown(ui, cache, &text[start..]));
            message_context_menu(ui, &segment.response, text);
        } else if streaming && steps.is_empty() {
            ui.label(RichText::new("正在处理你的需求…").size(12.0).color(MUTED));
        }
    });
    if !streaming {
        message_actions(ui, text, false);
    }
}

fn begin_message_selection(ui: &mut egui::Ui) -> bool {
    let ctx = ui.ctx();
    let selection = ctx.plugin::<LabelSelectionState>();
    if ui.input(|input| input.pointer.button_pressed(egui::PointerButton::Secondary)) {
        let selection = selection.lock();
        let snapshot = selection.has_selection().then(|| selection.clone());
        ctx.data_mut(|data| {
            let id = egui::Id::new("chat-context-selection");
            data.remove::<LabelSelectionState>(id);
            if let Some(snapshot) = snapshot {
                data.insert_temp(id, snapshot);
            }
        });
    }
    if let Some(snapshot) = ctx.data_mut(|data| {
        data.remove_temp::<LabelSelectionState>(egui::Id::new("chat-copy-selection"))
    }) {
        *selection.lock() = snapshot;
        ui.input_mut(|input| input.events.push(egui::Event::Copy));
        true
    } else {
        false
    }
}

fn message_context_menu(ui: &egui::Ui, response: &egui::Response, text: &str) {
    let right_clicked = ui.rect_contains_pointer(response.rect)
        && ui.input(|input| input.pointer.button_clicked(egui::PointerButton::Secondary));
    egui::Popup::context_menu(response)
        .open_memory(right_clicked.then_some(egui::containers::SetOpenCommand::Bool(true)))
        .show(|ui| {
            let selection = ui.ctx().data(|data| {
                data.get_temp::<LabelSelectionState>(egui::Id::new("chat-context-selection"))
            });
            if ui
                .add_enabled(selection.is_some(), egui::Button::new("复制选中文字"))
                .clicked()
                && let Some(selection) = selection
            {
                ui.ctx().data_mut(|data| {
                    data.insert_temp(egui::Id::new("chat-copy-selection"), selection)
                });
                ui.ctx().request_repaint();
                ui.close();
            }
            if ui.button("复制整条消息").clicked() {
                ui.ctx().copy_text(text.to_owned());
                ui.close();
            }
        });
}

fn process(ui: &mut egui::Ui, steps: &[ChatStep], streaming: bool, live: bool) {
    let id = ui.make_persistent_id(("ai-process", steps[0].after_text));
    let was_live = ui.ctx().data_mut(|data| {
        let previous = data.get_temp::<bool>(id).unwrap_or(false);
        data.insert_temp(id, live);
        previous
    });
    let tools = steps
        .iter()
        .filter(|step| matches!(step.activity, ChatActivity::Tool { .. }))
        .count();
    let summary_only = steps
        .iter()
        .all(|step| matches!(step.activity, ChatActivity::Thinking { summary: true, .. }));
    let title = match (tools, steps.len() - tools) {
        (0, _) if summary_only => "思考摘要".into(),
        (0, _) => "思考过程".into(),
        (tools, 0) => format!("工具调用 · {tools}"),
        (tools, _) => format!("思考与工具 · {tools} 次调用"),
    };
    egui::CollapsingHeader::new(
        RichText::new(if live {
            format!("{title} · 进行中")
        } else {
            title
        })
        .size(11.0)
        .color(MUTED),
    )
    .id_salt(id)
    .default_open(live)
    .open((was_live && !live).then_some(false))
    .show(ui, |ui| {
        for (index, step) in steps.iter().enumerate() {
            ui.push_id(index, |ui| match &step.activity {
                ChatActivity::Thinking { text, summary } => {
                    if *summary && !summary_only {
                        ui.label(RichText::new("思考摘要").size(10.0).color(MUTED));
                    }
                    let response = ui.add(
                        egui::Label::new(RichText::new(text).size(12.0).color(MUTED))
                            .wrap()
                            .selectable(true),
                    );
                    message_context_menu(ui, &response, text);
                }
                ChatActivity::Tool { name, outcome } => {
                    let (status, color) = match outcome {
                        Some(true) => ("已完成", MUTED),
                        Some(false) => ("失败", Color32::from_rgb(184, 65, 65)),
                        None if streaming => ("执行中", MUTED),
                        None => ("未返回结果", MUTED),
                    };
                    let label = format!("{name} · {status}");
                    let response = ui.add(
                        egui::Label::new(RichText::new(&label).size(11.0).color(color))
                            .wrap()
                            .selectable(true),
                    );
                    message_context_menu(ui, &response, &label);
                }
            });
        }
    });
    ui.add_space(6.0);
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
        message_context_menu(ui, &bubble.response, text);
        message_actions(ui, text, true);
    });
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

#[cfg(test)]
mod tests;
