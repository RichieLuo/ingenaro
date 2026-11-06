//! Built-in, independently authored widget presets and reusable groups.
mod catalog;
pub use catalog::ITEMS;
mod geometry;
use geometry::*;
mod icons;
use icons::Icon;
mod patterns;
use patterns::Pattern;

use crate::model::{Project, TextLayout, Widget, WidgetKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Basic,
    Forms,
    Markup,
    Flow,
    Icons,
    Patterns,
}
impl Category {
    pub const ALL: [Self; 6] = [
        Self::Basic,
        Self::Forms,
        Self::Markup,
        Self::Flow,
        Self::Icons,
        Self::Patterns,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Forms => "forms",
            Self::Markup => "markup",
            Self::Flow => "flow",
            Self::Icons => "icons",
            Self::Patterns => "patterns",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Basic => "基本元件",
            Self::Forms => "表单与菜单",
            Self::Markup => "标注",
            Self::Flow => "流程图",
            Self::Icons => "常用图标",
            Self::Patterns => "界面组合",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Preset {
    Kind(WidgetKind),
    Heading(u8),
    Paragraph,
    Link,
    OutlineButton,
    Rounded,
    Box(u8),
    Sticky(u8),
    Marker(bool),
    VerticalLine,
    Placeholder,
    Hotspot,
    Shape(&'static str),
    Table,
    Menu(bool),
    Tree,
    Icon(Icon),
    Pattern(Pattern),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LibraryItem {
    pub id: &'static str,
    pub label: &'static str,
    pub category: Category,
    pub keywords: &'static str,
    preset: Preset,
}

impl LibraryItem {
    pub fn matches(self, query: &str) -> bool {
        if query.trim().is_empty() {
            return true;
        }
        let haystack = format!("{} {} {}", self.label, self.keywords, self.id).to_lowercase();
        query
            .split_whitespace()
            .all(|word| haystack.contains(&word.to_lowercase()))
    }
    pub fn kind(self) -> WidgetKind {
        match self.preset {
            Preset::Kind(kind) => kind,
            Preset::Heading(_) | Preset::Paragraph | Preset::Link => WidgetKind::Text,
            _ => WidgetKind::Rectangle,
        }
    }
    /// Normalized paths for tiles that depict an editable vector silhouette.
    pub fn preview_contours(self) -> Vec<(Vec<[f32; 2]>, bool)> {
        match self.preset {
            Preset::Shape(name) => crate::shape_geometry::contours_from_points(&shape(name)),
            Preset::Icon(icon) => icon.contours(),
            _ => Vec::new(),
        }
    }
    pub fn create(self, project: &mut Project, x: f32, y: f32) -> Vec<Widget> {
        let mut widget = Widget::new(project.allocate_id(), self.kind(), x, y);
        widget.name = format!("{} {}", self.label, widget.id);
        match self.preset {
            Preset::Kind(_) => {}
            Preset::Heading(level) => {
                widget.font_size = match level {
                    1 => 32.0,
                    2 => 24.0,
                    _ => 18.0,
                };
                widget.width = 300.0;
                widget.height = widget.font_size * 1.6;
                widget.text = self.label.into();
                widget.text_layout = Some(TextLayout {
                    horizontal: 0,
                    vertical: 1,
                    padding: [0.0; 4],
                    wrap: false,
                    bold: true,
                });
            }
            Preset::Paragraph => {
                widget.width = 300.0;
                widget.height = 100.0;
                widget.text = "在这里输入段落内容。\n用清晰的文字描述你的设计。".into();
                widget.text_layout = Some(TextLayout {
                    horizontal: 0,
                    vertical: 0,
                    padding: [0.0; 4],
                    wrap: true,
                    bold: false,
                });
            }
            Preset::Link => {
                widget.text = "文本链接".into();
                widget.text_color = "#6759E8".into();
                widget.width = 120.0;
            }
            Preset::OutlineButton => {
                widget.width = 140.0;
                widget.height = 44.0;
                widget.text = "按钮".into();
                widget.fill = "#FFFFFF".into();
                widget.stroke = "#6759E8".into();
                widget.text_color = "#6759E8".into();
                widget.radius = 6.0;
            }
            Preset::Rounded => {
                widget.radius = 24.0;
            }
            Preset::Box(style) => {
                widget.radius = 0.0;
                widget.fill = if style == 2 { "#F0EDFF" } else { "#E8EAF0" }.into();
                widget.stroke = if style == 2 { "#AFA5E8" } else { "#FFFFFF00" }.into();
            }
            Preset::Sticky(style) => {
                widget.width = 200.0;
                widget.height = 130.0;
                widget.radius = 0.0;
                widget.fill = match style {
                    1 => "#FFF3B0",
                    2 => "#E3F3D4",
                    3 => "#DBEDFF",
                    _ => "#FFE0EB",
                }
                .into();
                widget.stroke = "#FFFFFF00".into();
                widget.text = "在这里添加说明".into();
                widget.text_layout = Some(TextLayout {
                    horizontal: 0,
                    vertical: 0,
                    padding: [16.0; 4],
                    wrap: true,
                    bold: false,
                });
            }
            Preset::Marker(drop) => {
                widget.width = 40.0;
                widget.height = if drop { 52.0 } else { 40.0 };
                widget.text = "1".into();
                widget.fill = "#6759E8".into();
                widget.stroke = "#FFFFFF00".into();
                widget.text_color = "#FFFFFF".into();
                if drop {
                    widget.vector_points = Some(crate::shape_geometry::polygon(&[
                        [0.5, 1.0],
                        [0.05, 0.45],
                        [0.02, 0.25],
                        [0.15, 0.08],
                        [0.5, 0.0],
                        [0.85, 0.08],
                        [0.98, 0.25],
                        [0.95, 0.45],
                    ]));
                } else {
                    widget.kind = WidgetKind::Ellipse;
                }
            }
            Preset::VerticalLine => {
                widget.kind = WidgetKind::Line;
                widget.width = 2.0;
                widget.height = 160.0;
                widget.fill = "#A9AFBE".into();
                widget.vector_points = Some(paths(&[vec![[0.5, 0.0], [0.5, 1.0]]], &[false]));
            }
            Preset::Placeholder => {
                widget.width = 240.0;
                widget.height = 160.0;
                widget.fill = "#F3F4F8".into();
                widget.text = "图片占位符".into();
                widget.vector_points = Some(paths(
                    &[
                        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                        vec![[0.0, 0.0], [1.0, 1.0]],
                        vec![[1.0, 0.0], [0.0, 1.0]],
                    ],
                    &[true, false, false],
                ));
            }
            Preset::Hotspot => {
                widget.fill = "#FFFFFF00".into();
                widget.stroke = "#FFFFFF00".into();
                widget.radius = 0.0;
            }
            Preset::Shape(name) => {
                widget.width = 160.0;
                widget.height = 100.0;
                widget.radius = 0.0;
                widget.text = if self.category == Category::Flow {
                    self.label.into()
                } else {
                    String::new()
                };
                if name == "start" {
                    widget.radius = 50.0;
                    widget.height = 60.0;
                } else if name == "connector" {
                    widget.kind = WidgetKind::Ellipse;
                    widget.width = 56.0;
                    widget.height = 56.0;
                    widget.text.clear();
                } else {
                    widget.vector_points = Some(shape(name));
                }
                if name == "up-arrow" {
                    widget.width = 100.0;
                    widget.height = 160.0;
                }
                if name == "actor" {
                    widget.width = 64.0;
                    widget.height = 100.0;
                    widget.text.clear();
                    widget.fill = "#FFFFFF00".into();
                }
                if name == "bracket" {
                    widget.width = 30.0;
                    widget.text.clear();
                }
            }
            Preset::Icon(icon) => {
                widget.width = 32.0;
                widget.height = 32.0;
                widget.fill = "#FFFFFF00".into();
                widget.stroke = "#243047".into();
                widget.radius = 0.0;
                widget.vector_points = Some(icon.vector_points());
            }
            Preset::Table | Preset::Menu(_) | Preset::Tree => {
                return self.composite(project, widget);
            }
            Preset::Pattern(pattern) => return pattern.create(project, widget, self.label),
        }
        vec![widget]
    }
    fn composite(self, project: &mut Project, frame: Widget) -> Vec<Widget> {
        let group = frame.id;
        let mut widgets = Vec::new();
        let (rows, columns) = match self.preset {
            Preset::Table => (4, 3),
            Preset::Menu(true) => (4, 1),
            Preset::Menu(false) => (1, 4),
            _ => (5, 1),
        };
        for row in 0..rows {
            for col in 0..columns {
                let tree = matches!(self.preset, Preset::Tree);
                let mut w = Widget::new(
                    project.allocate_id(),
                    if tree {
                        WidgetKind::Text
                    } else {
                        WidgetKind::Rectangle
                    },
                    frame.x + col as f32 * 100.0,
                    frame.y + row as f32 * 36.0,
                );
                w.width = if tree { 220.0 } else { 100.0 };
                w.height = 36.0;
                w.radius = 0.0;
                w.groups.push(group);
                w.name = format!("{} {}-{}", self.label, row + 1, col + 1);
                w.text = if tree {
                    match row {
                        0 => "▾ 目录",
                        1 => "    页面一",
                        2 => "    页面二",
                        3 => "▾ 文件夹",
                        _ => "    子页面",
                    }
                    .into()
                } else if matches!(self.preset, Preset::Table) {
                    if row == 0 {
                        format!("列 {}", col + 1)
                    } else {
                        format!("单元格 {}-{}", row, col + 1)
                    }
                } else {
                    format!("菜单 {}", row + col + 1)
                };
                if row == 0 && !tree {
                    w.fill = "#F0EDFF".into();
                }
                widgets.push(w);
            }
        }
        widgets
    }
}

#[cfg(test)]
mod tests;
