mod glyphs;

use super::*;

const SUBTLE: Color32 = Color32::from_rgb(97, 104, 124);
const SURFACE: Color32 = Color32::from_rgb(249, 250, 253);
const FILTERS: [(Option<Category>, &str); 7] = [
    (None, "全部"),
    (Some(Category::Basic), "基础"),
    (Some(Category::Forms), "表单"),
    (Some(Category::Patterns), "组合"),
    (Some(Category::Markup), "标注"),
    (Some(Category::Flow), "流程"),
    (Some(Category::Icons), "图标"),
];

pub(super) struct LibraryDrag {
    pub(super) item: LibraryItem,
    widgets: Vec<Widget>,
    bounds: Rect,
}

impl LibraryDrag {
    pub(super) fn new(item: LibraryItem) -> Self {
        // Build the preview once using scratch IDs so dragging has no document edits.
        let widgets = item.create(&mut Project::blank(), 0.0, 0.0);
        let bounds = widgets.iter().fold(Rect::NOTHING, |bounds, widget| {
            bounds.union(Rect::from_min_size(
                Pos2::new(widget.x, widget.y),
                vec2(widget.width, widget.height),
            ))
        });
        Self {
            item,
            widgets,
            bounds,
        }
    }

    pub(super) fn paint(&self, painter: &egui::Painter, origin: Pos2, zoom: f32) {
        let mut ghost = painter.clone();
        ghost.multiply_opacity(0.72);
        for widget in &self.widgets {
            draw_widget(&ghost, widget, origin, zoom, None);
        }
        let bounds = Rect::from_min_max(
            origin + self.bounds.min.to_vec2() * zoom,
            origin + self.bounds.max.to_vec2() * zoom,
        );
        // Outline the footprint, including transparent presets such as hotspots.
        painter.rect_stroke(
            bounds,
            2.0,
            Stroke::new(1.0, PURPLE.gamma_multiply(0.75)),
            StrokeKind::Middle,
        );
    }
}

impl App {
    pub(super) fn library_pane(&mut self, ui: &mut egui::Ui) {
        let items = widget_library::ITEMS;
        ui.label(
            RichText::new(format!("{} 项内置组件", items.len()))
                .size(11.0)
                .color(SUBTLE),
        );
        let query_changed = search(ui, &mut self.library_query);
        let category_changed = filters(ui, &mut self.library_category);
        ui.add_space(2.0);
        ui.separator();

        let mut scroll = egui::ScrollArea::vertical()
            .id_salt("library-scroll")
            .auto_shrink([false, false])
            .max_height((ui.available_height() - 32.0).max(0.0));
        if query_changed || category_changed {
            scroll = scroll.vertical_scroll_offset(0.0);
        }
        scroll.show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            // Reserve a little space for the floating scrollbar.
            let width = ui.available_width() - 7.0;
            let columns = if width >= 234.0 { 3 } else { 2 };
            let tile_width = (width - (columns - 1) as f32 * 6.0) / columns as f32;
            let mut found = false;
            for category in FILTERS.iter().filter_map(|(category, _)| *category) {
                if self
                    .library_category
                    .is_some_and(|selected| selected != category)
                {
                    continue;
                }
                let matching: Vec<_> = items
                    .iter()
                    .copied()
                    .filter(|item| item.category == category && item.matches(&self.library_query))
                    .collect();
                if matching.is_empty() {
                    continue;
                }
                found = true;
                let palette = Palette::for_category(category);
                section_heading(ui, width, category, matching.len(), palette.ink);
                for row in matching.chunks(columns) {
                    ui.horizontal(|ui| {
                        for &item in row {
                            ui.push_id(item.id, |ui| {
                                let tile = library_tile(ui, item, tile_width, palette);
                                if tile.clicked() {
                                    let stagger =
                                        (self.project.pages[self.page].widgets.len() % 6) as f32;
                                    self.add_library_item(item, 64.0 + stagger * 16.0, 80.0);
                                }
                                if tile.drag_started_by(egui::PointerButton::Primary) {
                                    self.pending_widget = Some(LibraryDrag::new(item));
                                }
                            });
                        }
                    });
                }
                ui.add_space(8.0);
            }
            if !found {
                ui.add_space(26.0);
                ui.vertical_centered(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
                    ui.painter().rect_filled(rect, 10.0, SURFACE);
                    glyphs::magnifier(ui.painter(), rect.center(), SUBTLE);
                    ui.label(RichText::new("没有匹配的组件").strong().size(12.0));
                    ui.label(
                        RichText::new("试试其他关键词或分类")
                            .size(11.0)
                            .color(SUBTLE),
                    );
                });
            }
        });
        ui.separator();
        ui.label(
            RichText::new("点击添加 · 拖入画布")
                .size(11.0)
                .color(SUBTLE),
        );
    }
}

fn search(ui: &mut egui::Ui, query: &mut String) -> bool {
    let mut frame = egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .begin(ui);
    frame
        .content_ui
        .set_min_width(frame.content_ui.available_width());
    let (field, cleared) = frame
        .content_ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            ui.spacing_mut().button_padding = vec2(0.0, 0.0);
            let (icon, _) = ui.allocate_exact_size(vec2(16.0, 22.0), Sense::hover());
            glyphs::magnifier(ui.painter(), icon.center(), SUBTLE);
            let field = ui.add(
                egui::TextEdit::singleline(query)
                    .id_salt("library-search")
                    .font(FontId::proportional(12.0))
                    .hint_text("搜索组件…")
                    .frame(false)
                    .margin(egui::Margin::ZERO)
                    .desired_width((ui.available_width() - 23.0).max(1.0)),
            );
            let cleared = !query.is_empty()
                && ui
                    .add_sized(
                        [18.0, 22.0],
                        egui::Button::new(RichText::new("×").size(15.0).color(SUBTLE))
                            .small()
                            .frame(false),
                    )
                    .on_hover_text("清除搜索")
                    .clicked();
            if cleared {
                query.clear();
                field.request_focus();
            }
            (field, cleared)
        })
        .inner;
    if field.has_focus() {
        frame.frame.stroke = Stroke::new(1.0, PURPLE);
    }
    frame.end(ui);
    field.changed() || cleared
}

fn filters(ui: &mut egui::Ui, selected: &mut Option<Category>) -> bool {
    let previous = *selected;
    for row in FILTERS.chunks(4) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.spacing_mut().button_padding = vec2(0.0, 0.0);
            let width = (ui.available_width() - 12.0) / 4.0;
            for &(category, label) in row {
                let active = *selected == category;
                let color = if active {
                    Color32::from_rgb(91, 58, 182)
                } else {
                    SUBTLE
                };
                let mut button = egui::Button::new(RichText::new(label).size(11.5).color(color))
                    .corner_radius(6)
                    .frame_when_inactive(false);
                if active {
                    button = button
                        .frame_when_inactive(true)
                        .fill(Color32::from_rgb(238, 233, 253))
                        .stroke(Stroke::NONE);
                }
                if ui
                    .add_sized([width, 24.0], button)
                    .on_hover_text(category.map_or("全部组件", Category::label))
                    .clicked()
                {
                    *selected = category;
                }
            }
        });
    }
    previous != *selected
}

fn section_heading(ui: &mut egui::Ui, width: f32, category: Category, count: usize, ink: Color32) {
    ui.allocate_ui_with_layout(
        vec2(width, 20.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let (marker, _) = ui.allocate_exact_size(vec2(4.0, 12.0), Sense::hover());
            ui.painter().circle_filled(marker.center(), 2.0, ink);
            ui.label(RichText::new(category.label()).size(12.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{count} 项"))
                        .size(11.0)
                        .color(SUBTLE),
                );
            });
        },
    );
}

#[derive(Clone, Copy)]
struct Palette {
    ink: Color32,
    tint: Color32,
}

impl Palette {
    fn for_category(category: Category) -> Self {
        let (ink, tint) = match category {
            Category::Basic => ([101, 70, 184], [241, 235, 253]),
            Category::Forms => ([26, 115, 104], [228, 246, 240]),
            Category::Patterns => ([53, 85, 168], [232, 240, 255]),
            Category::Markup => ([144, 83, 21], [255, 241, 222]),
            Category::Flow => ([39, 101, 133], [230, 244, 252]),
            Category::Icons => ([68, 83, 113], [233, 238, 248]),
        };
        Self {
            ink: Color32::from_rgb(ink[0], ink[1], ink[2]),
            tint: Color32::from_rgb(tint[0], tint[1], tint[2]),
        }
    }
}

fn library_tile(
    ui: &mut egui::Ui,
    item: LibraryItem,
    width: f32,
    palette: Palette,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 60.0), Sense::click_and_drag());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), item.label)
    });
    let hovered = response.hovered() || response.dragged();
    let painter = ui.painter();
    let edge = if response.has_focus() {
        palette.ink
    } else if hovered {
        palette.ink.gamma_multiply(0.38)
    } else {
        BORDER
    };
    painter.rect(
        rect,
        8.0,
        if hovered { palette.tint } else { SURFACE },
        Stroke::new(1.0, edge),
        StrokeKind::Inside,
    );
    let center = rect.center_top() + vec2(0.0, 21.0);
    let backdrop = Rect::from_center_size(center, vec2(32.0, 30.0));
    painter.rect_filled(
        backdrop,
        7.0,
        if hovered {
            Color32::WHITE
        } else {
            palette.tint
        },
    );
    glyphs::paint(ui, center, item, palette.ink);

    let mut label = egui::text::LayoutJob::simple(
        item.label.into(),
        FontId::proportional(11.5),
        INK,
        width - 10.0,
    );
    label.wrap.max_rows = 1;
    label.wrap.break_anywhere = true;
    let label = painter.layout_job(label);
    let center = rect.center_bottom() - vec2(0.0, 13.0);
    painter.galley(center - label.size() / 2.0, label, INK);

    response
        .on_hover_cursor(egui::CursorIcon::Grab)
        .on_hover_text(format!(
            "{} · {}\n点击添加，或拖入画布",
            item.label,
            item.category.label()
        ))
}
