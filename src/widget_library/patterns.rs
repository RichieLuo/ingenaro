//! Editable interface patterns assembled from ordinary grouped widgets.

use super::{Icon, paths};
use crate::model::{Project, TextLayout, Widget, WidgetKind};

const WHITE: &str = "#FFFFFF";
const CLEAR: &str = "#FFFFFF00";
const BORDER: &str = "#DCE1ED";
const INK: &str = "#242943";
const MUTED: &str = "#606B80";
const ACCENT: &str = "#6759E8";
const TINT: &str = "#EFEDFF";

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Pattern {
    Card,
    Profile,
    Stat,
    Login,
    Search,
    Tabs,
    Pagination,
    Breadcrumb,
    Dialog,
    Alert,
    Empty,
    Progress,
}

impl Pattern {
    pub(super) fn create(self, project: &mut Project, anchor: Widget, label: &str) -> Vec<Widget> {
        let mut group = Group {
            project,
            label,
            id: anchor.id,
            origin: [anchor.x, anchor.y],
            widgets: Vec::new(),
        };
        match self {
            Self::Card => {
                group.card(320.0, 176.0);
                group.surface([20.0, 20.0, 36.0, 36.0], TINT, CLEAR, 10.0);
                group.icon([25.0, 25.0, 26.0, 26.0], Icon::Folder, ACCENT);
                group.text([20.0, 66.0, 280.0, 28.0], "设计资源", 19.0, INK, true);
                group.text(
                    [20.0, 95.0, 280.0, 22.0],
                    "集中管理项目的文件和素材。",
                    14.0,
                    MUTED,
                    false,
                );
                group.button([20.0, 130.0, 104.0, 30.0], "查看资源", false);
            }
            Self::Profile => {
                group.card(320.0, 108.0);
                let avatar = group.add(WidgetKind::Ellipse, [18.0, 25.0, 56.0, 56.0], "");
                avatar.fill = TINT.into();
                avatar.stroke = CLEAR.into();
                group.icon([28.0, 35.0, 36.0, 36.0], Icon::User, ACCENT);
                group.text([88.0, 26.0, 142.0, 28.0], "林晓", 18.0, INK, true);
                group.text([88.0, 56.0, 142.0, 22.0], "产品设计师", 14.0, MUTED, false);
                group.button([232.0, 39.0, 70.0, 30.0], "查看", false);
            }
            Self::Stat => {
                group.card(280.0, 124.0);
                group.text([18.0, 15.0, 156.0, 24.0], "本周访问量", 14.0, MUTED, false);
                group.text([18.0, 45.0, 156.0, 40.0], "12,480", 30.0, INK, true);
                group.text(
                    [18.0, 88.0, 156.0, 22.0],
                    "较上周 +12.8%",
                    13.0,
                    "#11745A",
                    false,
                );
                let sparkline = group.surface([182.0, 52.0, 78.0, 38.0], CLEAR, ACCENT, 0.0);
                sparkline.vector_points = Some(paths(
                    &[vec![
                        [0.0, 1.0],
                        [0.24, 0.39],
                        [0.44, 0.47],
                        [0.68, 0.10],
                        [0.82, 0.17],
                        [1.0, 0.0],
                    ]],
                    &[false],
                ));
            }
            Self::Login => {
                group.card(320.0, 268.0);
                group.text([22.0, 15.0, 276.0, 30.0], "邮箱登录", 21.0, INK, true);
                group.text([22.0, 58.0, 276.0, 22.0], "邮箱", 14.0, INK, false);
                group.input([22.0, 83.0, 276.0, 38.0], "name@example.com");
                group.text([22.0, 134.0, 276.0, 22.0], "验证码", 14.0, INK, false);
                group.input([22.0, 159.0, 276.0, 38.0], "输入验证码");
                group.button([22.0, 211.0, 276.0, 38.0], "登录", true);
            }
            Self::Search => {
                group.input([0.0, 0.0, 252.0, 40.0], "输入关键词");
                group.button([260.0, 0.0, 76.0, 40.0], "搜索", true);
            }
            Self::Tabs => {
                group.surface([0.0, 37.0, 288.0, 1.0], BORDER, CLEAR, 0.0);
                group.surface([0.0, 36.0, 96.0, 2.0], ACCENT, CLEAR, 1.0);
                for (index, (label, color)) in [("概览", ACCENT), ("详情", MUTED), ("活动", MUTED)]
                    .into_iter()
                    .enumerate()
                {
                    group
                        .text(
                            [index as f32 * 96.0, 0.0, 96.0, 35.0],
                            label,
                            14.0,
                            color,
                            index == 0,
                        )
                        .horizontal = 1;
                }
            }
            Self::Pagination => {
                group.button([0.0, 0.0, 64.0, 34.0], "上一页", false);
                for (index, label) in ["1", "2", "3", "4", "5"].into_iter().enumerate() {
                    group.button(
                        [72.0 + index as f32 * 40.0, 0.0, 32.0, 34.0],
                        label,
                        index == 0,
                    );
                }
                group.button([272.0, 0.0, 64.0, 34.0], "下一页", false);
            }
            Self::Breadcrumb => {
                group.text([0.0, 0.0, 44.0, 32.0], "首页", 14.0, ACCENT, false);
                group.icon([42.0, 8.0, 16.0, 16.0], Icon::Next, MUTED);
                group.text([65.0, 0.0, 67.0, 32.0], "工作空间", 14.0, ACCENT, false);
                group.icon([135.0, 8.0, 16.0, 16.0], Icon::Next, MUTED);
                group.text([158.0, 0.0, 110.0, 32.0], "项目详情", 14.0, INK, true);
            }
            Self::Dialog => {
                group.card(352.0, 158.0);
                group.text([22.0, 18.0, 308.0, 28.0], "确认提交？", 20.0, INK, true);
                group.text(
                    [22.0, 54.0, 308.0, 24.0],
                    "请检查已填写的信息。",
                    14.0,
                    MUTED,
                    false,
                );
                group.button([170.0, 102.0, 76.0, 36.0], "取消", false);
                group.button([254.0, 102.0, 76.0, 36.0], "确认", true);
            }
            Self::Alert => {
                group.surface([0.0, 0.0, 352.0, 74.0], "#EFF5FF", "#C9DBFA", 10.0);
                group.icon([14.0, 14.0, 27.0, 27.0], Icon::Info, "#3158A9");
                group.text([52.0, 10.0, 280.0, 26.0], "更新提示", 15.0, "#243F77", true);
                group.text(
                    [52.0, 36.0, 280.0, 25.0],
                    "新的设置将在下次打开时生效。",
                    14.0,
                    MUTED,
                    false,
                );
            }
            Self::Empty => {
                group.card(320.0, 212.0);
                group.surface([132.0, 22.0, 56.0, 56.0], TINT, CLEAR, 15.0);
                group.icon([141.0, 31.0, 38.0, 38.0], Icon::Folder, ACCENT);
                group
                    .text([18.0, 88.0, 284.0, 28.0], "暂无项目", 19.0, INK, true)
                    .horizontal = 1;
                group
                    .text(
                        [18.0, 121.0, 284.0, 22.0],
                        "创建第一个项目，开始整理内容。",
                        14.0,
                        MUTED,
                        false,
                    )
                    .horizontal = 1;
                group.button([108.0, 160.0, 104.0, 34.0], "新建项目", true);
            }
            Self::Progress => {
                group.text([0.0, 0.0, 196.0, 26.0], "任务进度", 14.0, INK, true);
                group
                    .text([200.0, 0.0, 80.0, 26.0], "60%", 14.0, ACCENT, true)
                    .horizontal = 2;
                group.surface([0.0, 34.0, 280.0, 10.0], TINT, CLEAR, 5.0);
                group.surface([0.0, 34.0, 168.0, 10.0], ACCENT, CLEAR, 5.0);
            }
        }
        group.widgets
    }
}

struct Group<'a> {
    project: &'a mut Project,
    label: &'a str,
    id: u64,
    origin: [f32; 2],
    widgets: Vec<Widget>,
}

impl Group<'_> {
    fn add(&mut self, kind: WidgetKind, bounds: [f32; 4], text: &str) -> &mut Widget {
        let [x, y, width, height] = bounds;
        let mut widget = Widget::new(
            self.project.allocate_id(),
            kind,
            self.origin[0] + x,
            self.origin[1] + y,
        );
        widget.name = format!("{} · {} {}", self.label, kind.label(), widget.id);
        widget.width = width;
        widget.height = height;
        widget.text = text.into();
        widget.groups.push(self.id);
        self.widgets.push(widget);
        self.widgets.last_mut().unwrap()
    }

    fn card(&mut self, width: f32, height: f32) {
        self.surface([0.0, 0.0, width, height], WHITE, BORDER, 14.0);
    }

    fn surface(&mut self, bounds: [f32; 4], fill: &str, stroke: &str, radius: f32) -> &mut Widget {
        let widget = self.add(WidgetKind::Rectangle, bounds, "");
        widget.fill = fill.into();
        widget.stroke = stroke.into();
        widget.radius = radius;
        widget
    }

    fn text(
        &mut self,
        bounds: [f32; 4],
        text: &str,
        size: f32,
        color: &str,
        bold: bool,
    ) -> &mut TextLayout {
        let widget = self.add(WidgetKind::Text, bounds, text);
        widget.font_size = size;
        widget.text_color = color.into();
        widget.text_layout.insert(TextLayout {
            horizontal: 0,
            vertical: 1,
            padding: [0.0; 4],
            wrap: false,
            bold,
        })
    }

    fn button(&mut self, bounds: [f32; 4], text: &str, primary: bool) {
        let widget = self.add(WidgetKind::Button, bounds, text);
        widget.radius = 7.0;
        widget.font_size = 14.0;
        if !primary {
            widget.fill = WHITE.into();
            widget.stroke = BORDER.into();
            widget.text_color = INK.into();
        }
    }

    fn input(&mut self, bounds: [f32; 4], value: &str) {
        let widget = self.add(WidgetKind::Input, bounds, value);
        widget.radius = 7.0;
        widget.font_size = 14.0;
        widget.stroke = BORDER.into();
    }

    fn icon(&mut self, bounds: [f32; 4], icon: Icon, color: &str) {
        let widget = self.surface(bounds, CLEAR, color, 0.0);
        widget.vector_points = Some(icon.vector_points());
    }
}
