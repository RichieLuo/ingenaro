//! Responsive tables for the narrow chat sidebar.
use super::safe_link;
use eframe::egui::{
    self, Align, Color32, FontSelection, Frame, Label, Layout, Margin, RichText, ScrollArea, Sense,
    Stroke, TextStyle, Ui,
    scroll_area::{ScrollBarVisibility, ScrollSource},
    text::LayoutJob,
    vec2,
};
use pulldown_cmark::{Alignment, Event, Tag, TagEnd};
use std::ops::Range;

const PADDING: Margin = Margin::symmetric(8, 5);
const MIN_COLUMN_WIDTH: f32 = 48.0;
const WRAP_COLUMN_WIDTH: f32 = 104.0;
const GRID_LINE: Color32 = Color32::from_rgb(225, 228, 236);

pub(super) struct Table {
    alignments: Vec<Alignment>,
    rows: Vec<Vec<Cell>>,
}

impl Table {
    pub(super) fn read<'a>(
        ui: &Ui,
        alignments: Vec<Alignment>,
        mut events: impl Iterator<Item = Event<'a>>,
    ) -> Self {
        let mut rows = Vec::new();
        while let Some(event) = events.next() {
            match event {
                Event::Start(Tag::TableHead | Tag::TableRow) => rows.push(Vec::new()),
                Event::Start(Tag::TableCell) => {
                    let header = rows.len() == 1;
                    let cell = Cell::read(ui, header, events.by_ref());
                    if let Some(row) = rows.last_mut() {
                        row.push(cell);
                    }
                }
                Event::End(TagEnd::Table) => break,
                _ => {}
            }
        }
        Self { alignments, rows }
    }

    pub(super) fn show(&self, ui: &mut Ui) {
        let frame = Frame::new().stroke(Stroke::new(1.0, GRID_LINE));
        let viewport = ui.available_width();
        let available = viewport - frame.total_margin().sum().x;
        let widths = self.column_widths(ui, available);
        let table_width: f32 = widths.iter().sum();
        let show_table = |ui: &mut Ui| {
            // CommonMark places inline widgets in a wrapping horizontal layout.
            ui.vertical(|ui| {
                frame.show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    for (index, row) in self.rows.iter().enumerate() {
                        let fill = match index {
                            0 => Color32::from_rgb(241, 243, 249),
                            index if index.is_multiple_of(2) => Color32::from_rgb(248, 249, 252),
                            _ => Color32::WHITE,
                        };
                        Frame::new().fill(fill).show(ui, |ui| {
                            let row_rect = ui
                                .horizontal_top(|ui| {
                                    for ((cell, width), alignment) in
                                        row.iter().zip(&widths).zip(&self.alignments)
                                    {
                                        let align = match alignment {
                                            Alignment::None | Alignment::Left => Align::Min,
                                            Alignment::Center => Align::Center,
                                            Alignment::Right => Align::Max,
                                        };
                                        ui.allocate_ui_with_layout(
                                            vec2(*width, 0.0),
                                            Layout::top_down(align),
                                            |ui| {
                                                ui.set_width(*width);
                                                Frame::new().inner_margin(PADDING).show(ui, |ui| {
                                                    cell.show(ui, align);
                                                });
                                            },
                                        );
                                    }
                                })
                                .response
                                .rect;
                            if index == 0 {
                                ui.painter().hline(
                                    row_rect.x_range(),
                                    row_rect.bottom(),
                                    (1.0, GRID_LINE),
                                );
                            }
                        });
                    }
                })
            });
        };

        if table_width > available + 0.5 {
            // Keep vertical scrolling on the conversation and leave drag gestures to text selection.
            ui.spacing_mut().scroll.floating = false;
            ScrollArea::horizontal()
                .id_salt("table-overflow")
                .max_width(viewport)
                .min_scrolled_width(0.0)
                .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                .scroll_source(ScrollSource::SCROLL_BAR | ScrollSource::MOUSE_WHEEL)
                .show(ui, show_table);
        } else {
            show_table(ui);
        }
    }

    fn column_widths(&self, ui: &Ui, available: f32) -> Vec<f32> {
        let mut preferred = vec![MIN_COLUMN_WIDTH; self.alignments.len()];
        ui.fonts_mut(|fonts| {
            for row in &self.rows {
                for (width, cell) in preferred.iter_mut().zip(row) {
                    let text_width = fonts.layout_job(cell.job.clone()).size().x;
                    *width = width.max(text_width.ceil() + PADDING.sum().x);
                }
            }
        });
        let minimum: f32 = preferred
            .iter()
            .map(|width| width.min(WRAP_COLUMN_WIDTH))
            .sum();
        let natural: f32 = preferred.iter().sum();
        // Leave short columns at their natural size. Share the space left over
        // between the longer ones, down to a readable minimum before scrolling.
        let fraction = if natural > minimum {
            ((available - minimum) / (natural - minimum)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        for width in &mut preferred {
            let min = width.min(WRAP_COLUMN_WIDTH);
            *width = (min + (*width - min) * fraction).floor();
        }
        preferred
    }
}

#[derive(Default)]
struct Cell {
    job: LayoutJob,
    links: Vec<(Range<usize>, String)>,
}

impl Cell {
    fn read<'a>(ui: &Ui, header: bool, events: impl Iterator<Item = Event<'a>>) -> Self {
        let mut cell = Self::default();
        let mut strong = usize::from(header);
        let mut italic = 0;
        let mut strike = 0;
        let mut chars = 0;
        let mut link = None;
        for event in events {
            let (text, code) = match event {
                Event::End(TagEnd::TableCell) => break,
                Event::Text(text) | Event::InlineHtml(text) => (text, false),
                Event::Code(text) => (text, true),
                Event::SoftBreak | Event::HardBreak => ("\n".into(), false),
                Event::Start(Tag::Strong) => {
                    strong += 1;
                    continue;
                }
                Event::End(TagEnd::Strong) => {
                    strong -= 1;
                    continue;
                }
                Event::Start(Tag::Emphasis) => {
                    italic += 1;
                    continue;
                }
                Event::End(TagEnd::Emphasis) => {
                    italic -= 1;
                    continue;
                }
                Event::Start(Tag::Strikethrough) => {
                    strike += 1;
                    continue;
                }
                Event::End(TagEnd::Strikethrough) => {
                    strike -= 1;
                    continue;
                }
                Event::Start(Tag::Link { dest_url, .. }) => {
                    link = safe_link(&dest_url).then_some((chars, dest_url));
                    continue;
                }
                Event::End(TagEnd::Link) => {
                    if let Some((start, url)) = link.take() {
                        cell.links.push((start..chars, url.into_string()));
                    }
                    continue;
                }
                Event::FootnoteReference(name) => (format!("[{name}]").into(), false),
                // Image children render as alt text, with no image loading.
                _ => continue,
            };
            chars += text.chars().count();
            let mut rich = RichText::new(text.as_ref());
            if code {
                rich = rich.code().text_style(TextStyle::Monospace);
            }
            if strong > 0 {
                rich = rich.strong();
            }
            if italic > 0 {
                rich = rich.italics();
            }
            if strike > 0 {
                rich = rich.strikethrough();
            }
            if link.is_some() {
                rich = rich.color(ui.visuals().hyperlink_color).underline();
            }
            rich.append_to(
                &mut cell.job,
                ui.style(),
                FontSelection::Default,
                Align::Center,
            );
        }
        cell
    }

    fn show(&self, ui: &mut Ui, align: Align) {
        let mut job = self.job.clone();
        job.wrap.max_width = ui.available_width();
        job.halign = align;
        let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
        let response = ui.add(Label::new(galley.clone()).selectable(true));
        let origin = match align {
            Align::Min => response.rect.left_top(),
            Align::Center => response.rect.center_top(),
            Align::Max => response.rect.right_top(),
        };
        // Interact with only the glyphs of each link, even if it spans several lines.
        for (index, (range, url)) in self.links.iter().enumerate() {
            let mut start = 0;
            for (line, row) in galley.rows.iter().enumerate() {
                let end = start + row.char_count_excluding_newline();
                let from = range.start.max(start);
                let to = range.end.min(end);
                if from < to {
                    let mut rect = row.rect();
                    rect.min.x = row.pos.x + row.x_offset(from - start);
                    rect.max.x = row.pos.x + row.x_offset(to - start);
                    let response = ui
                        .interact(
                            rect.translate(origin.to_vec2()),
                            response.id.with((index, line)),
                            Sense::click(),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(url.as_str());
                    if response.clicked_with_open_in_background() || response.clicked() {
                        ui.ctx().open_url(egui::OpenUrl {
                            url: url.clone(),
                            new_tab: response.clicked_with_open_in_background(),
                        });
                    }
                }
                if range.end <= end {
                    break;
                }
                start += row.char_count_including_newline();
            }
        }
    }
}
