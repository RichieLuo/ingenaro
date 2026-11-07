use super::*;

impl Default for Project {
    fn default() -> Self {
        let mut next_id = 3;
        let mut overview = sample_page(1, "工作台");
        let mut library = sample_page(2, "组件与交互");
        for (page, active) in [(&mut overview, 1u64), (&mut library, 2u64)] {
            sample(
                &mut next_id,
                page,
                WidgetKind::Panel,
                0.0,
                0.0,
                224.0,
                800.0,
                "",
                "#FFFFFF",
                "#243047",
                16.0,
            );
            sample(
                &mut next_id,
                page,
                WidgetKind::Rectangle,
                28.0,
                30.0,
                38.0,
                38.0,
                "R",
                "#6759E8",
                "#FFFFFF",
                22.0,
            );
            sample(
                &mut next_id,
                page,
                WidgetKind::Text,
                79.0,
                31.0,
                128.0,
                34.0,
                "Ingenaro",
                "#FFFFFF00",
                "#243047",
                25.0,
            );
            sample(
                &mut next_id,
                page,
                WidgetKind::Text,
                28.0,
                91.0,
                170.0,
                24.0,
                "想法，从这里开始。",
                "#FFFFFF00",
                "#929BAA",
                13.0,
            );
            for (target, y, label) in [(1, 150.0, "工作台"), (2, 204.0, "组件与交互")] {
                let id = sample(
                    &mut next_id,
                    page,
                    WidgetKind::Button,
                    16.0,
                    y,
                    192.0,
                    44.0,
                    label,
                    if target == active {
                        "#EEEAFE"
                    } else {
                        "#FFFFFF00"
                    },
                    if target == active {
                        "#6759E8"
                    } else {
                        "#647085"
                    },
                    15.0,
                );
                page.widgets
                    .iter_mut()
                    .find(|w| w.id == id)
                    .unwrap()
                    .interaction = Interaction {
                    action: Action::Navigate,
                    target_page: Some(target),
                    ..Default::default()
                };
            }
            sample(
                &mut next_id,
                page,
                WidgetKind::Line,
                24.0,
                278.0,
                176.0,
                1.0,
                "",
                "#E9EDF3",
                "#243047",
                16.0,
            );
            sample(
                &mut next_id,
                page,
                WidgetKind::Text,
                28.0,
                300.0,
                170.0,
                24.0,
                "我的工作空间",
                "#FFFFFF00",
                "#929BAA",
                12.0,
            );
            sample(
                &mut next_id,
                page,
                WidgetKind::Text,
                28.0,
                340.0,
                170.0,
                28.0,
                "○   产品设计",
                "#FFFFFF00",
                "#647085",
                14.0,
            );
            sample(
                &mut next_id,
                page,
                WidgetKind::Text,
                28.0,
                384.0,
                170.0,
                28.0,
                "○   灵感收集",
                "#FFFFFF00",
                "#647085",
                14.0,
            );
            sample(
                &mut next_id,
                page,
                WidgetKind::Panel,
                16.0,
                690.0,
                192.0,
                88.0,
                "",
                "#F5F3FF",
                "#243047",
                14.0,
            );
            sample(
                &mut next_id,
                page,
                WidgetKind::Text,
                30.0,
                705.0,
                165.0,
                24.0,
                "自由创作，灵感成形",
                "#FFFFFF00",
                "#6759E8",
                14.0,
            );
            sample(
                &mut next_id,
                page,
                WidgetKind::Text,
                30.0,
                735.0,
                165.0,
                22.0,
                "拖入组件 · 连接页面",
                "#FFFFFF00",
                "#929BAA",
                12.0,
            );
        }
        sample(
            &mut next_id,
            &mut overview,
            WidgetKind::Text,
            264.0,
            38.0,
            650.0,
            40.0,
            "让每个想法，都有形状。",
            "#FFFFFF00",
            "#243047",
            30.0,
        );
        sample(
            &mut next_id,
            &mut overview,
            WidgetKind::Text,
            265.0,
            87.0,
            740.0,
            28.0,
            "欢迎来到你的设计工作台。拖入组件，连接页面，预览下一次灵感。",
            "#FFFFFF00",
            "#929BAA",
            14.0,
        );
        let nav = sample(
            &mut next_id,
            &mut overview,
            WidgetKind::Button,
            1060.0,
            48.0,
            176.0,
            44.0,
            "探索组件  →",
            "#6759E8",
            "#FFFFFF",
            15.0,
        );
        overview
            .widgets
            .iter_mut()
            .find(|w| w.id == nav)
            .unwrap()
            .interaction = Interaction {
            action: Action::Navigate,
            target_page: Some(2),
            ..Default::default()
        };
        for (x, label, number, caption, accent) in [
            (264.0, "设计项目", "12", "本周新增 3 个项目", "#6759E8"),
            (596.0, "页面与画板", "48", "让思路有序展开", "#229B87"),
            (928.0, "可复用组件", "126", "专注设计，减少重复", "#E5A14A"),
        ] {
            sample(
                &mut next_id,
                &mut overview,
                WidgetKind::Panel,
                x,
                150.0,
                308.0,
                160.0,
                "",
                "#FFFFFF",
                "#243047",
                14.0,
            );
            sample(
                &mut next_id,
                &mut overview,
                WidgetKind::Text,
                x + 24.0,
                170.0,
                240.0,
                26.0,
                label,
                "#FFFFFF00",
                "#929BAA",
                14.0,
            );
            sample(
                &mut next_id,
                &mut overview,
                WidgetKind::Text,
                x + 24.0,
                205.0,
                240.0,
                48.0,
                number,
                "#FFFFFF00",
                "#243047",
                36.0,
            );
            sample(
                &mut next_id,
                &mut overview,
                WidgetKind::Text,
                x + 24.0,
                268.0,
                260.0,
                22.0,
                caption,
                "#FFFFFF00",
                accent,
                12.0,
            );
        }
        sample(
            &mut next_id,
            &mut overview,
            WidgetKind::Text,
            264.0,
            352.0,
            400.0,
            34.0,
            "最近的灵感",
            "#FFFFFF00",
            "#243047",
            22.0,
        );
        sample(
            &mut next_id,
            &mut overview,
            WidgetKind::Text,
            1060.0,
            356.0,
            176.0,
            26.0,
            "持续创造，保持好奇",
            "#FFFFFF00",
            "#929BAA",
            13.0,
        );
        for (x, tint, accent, title, detail, symbol) in [
            (
                264.0,
                "#EEEAFE",
                "#6759E8",
                "品牌官网 · 焕新计划",
                "12 个页面  ·  刚刚编辑",
                "01",
            ),
            (
                596.0,
                "#E6F5F0",
                "#229B87",
                "数据工作台",
                "8 个页面  ·  今天 10:24",
                "02",
            ),
            (
                928.0,
                "#FFF2DF",
                "#D89639",
                "移动端 · 生活方式",
                "24 个页面  ·  昨天",
                "03",
            ),
        ] {
            sample(
                &mut next_id,
                &mut overview,
                WidgetKind::Panel,
                x,
                410.0,
                308.0,
                284.0,
                "",
                "#FFFFFF",
                "#243047",
                16.0,
            );
            sample(
                &mut next_id,
                &mut overview,
                WidgetKind::Rectangle,
                x + 16.0,
                426.0,
                276.0,
                170.0,
                "",
                tint,
                accent,
                16.0,
            );
            sample(
                &mut next_id,
                &mut overview,
                WidgetKind::Text,
                x + 112.0,
                466.0,
                90.0,
                80.0,
                symbol,
                "#FFFFFF00",
                accent,
                58.0,
            );
            sample(
                &mut next_id,
                &mut overview,
                WidgetKind::Text,
                x + 20.0,
                614.0,
                268.0,
                28.0,
                title,
                "#FFFFFF00",
                "#243047",
                17.0,
            );
            sample(
                &mut next_id,
                &mut overview,
                WidgetKind::Text,
                x + 20.0,
                652.0,
                268.0,
                22.0,
                detail,
                "#FFFFFF00",
                "#929BAA",
                12.0,
            );
        }
        sample(
            &mut next_id,
            &mut overview,
            WidgetKind::Text,
            265.0,
            738.0,
            960.0,
            28.0,
            "小提示：选中元件可编辑样式；在「交互」中设置页面跳转、显示隐藏与文本变化。",
            "#FFFFFF00",
            "#929BAA",
            13.0,
        );
        sample(
            &mut next_id,
            &mut library,
            WidgetKind::Text,
            264.0,
            38.0,
            800.0,
            40.0,
            "把想法，变成可点击的原型。",
            "#FFFFFF00",
            "#243047",
            30.0,
        );
        sample(
            &mut next_id,
            &mut library,
            WidgetKind::Text,
            265.0,
            87.0,
            950.0,
            28.0,
            "从基础组件开始，尝试下方交互。导出的 HTML 同样可以独立运行。",
            "#FFFFFF00",
            "#929BAA",
            14.0,
        );
        sample(
            &mut next_id,
            &mut library,
            WidgetKind::Panel,
            264.0,
            150.0,
            972.0,
            262.0,
            "",
            "#FFFFFF",
            "#243047",
            16.0,
        );
        sample(
            &mut next_id,
            &mut library,
            WidgetKind::Text,
            292.0,
            170.0,
            860.0,
            32.0,
            "基础组件",
            "#FFFFFF00",
            "#243047",
            20.0,
        );
        sample(
            &mut next_id,
            &mut library,
            WidgetKind::Rectangle,
            296.0,
            230.0,
            168.0,
            112.0,
            "矩形",
            "#EEEAFE",
            "#6759E8",
            16.0,
        );
        sample(
            &mut next_id,
            &mut library,
            WidgetKind::Ellipse,
            516.0,
            230.0,
            112.0,
            112.0,
            "椭圆",
            "#E6F5F0",
            "#229B87",
            16.0,
        );
        sample(
            &mut next_id,
            &mut library,
            WidgetKind::Input,
            684.0,
            232.0,
            232.0,
            44.0,
            "输入一些内容…",
            "#F8FAFC",
            "#929BAA",
            14.0,
        );
        sample(
            &mut next_id,
            &mut library,
            WidgetKind::Checkbox,
            684.0,
            302.0,
            236.0,
            32.0,
            "保持我的创作节奏",
            "#FFFFFF00",
            "#647085",
            14.0,
        );
        sample(
            &mut next_id,
            &mut library,
            WidgetKind::Button,
            972.0,
            232.0,
            224.0,
            44.0,
            "一个主按钮",
            "#6759E8",
            "#FFFFFF",
            15.0,
        );
        sample(
            &mut next_id,
            &mut library,
            WidgetKind::Line,
            972.0,
            318.0,
            224.0,
            2.0,
            "",
            "#DDE2EB",
            "#647085",
            14.0,
        );
        sample(
            &mut next_id,
            &mut library,
            WidgetKind::Text,
            296.0,
            367.0,
            880.0,
            24.0,
            "矩形、椭圆、文字、按钮、表单、图片与容器 —— 自由组合，随心表达。",
            "#FFFFFF00",
            "#929BAA",
            13.0,
        );
        sample(
            &mut next_id,
            &mut library,
            WidgetKind::Panel,
            264.0,
            444.0,
            972.0,
            278.0,
            "",
            "#FFFFFF",
            "#243047",
            16.0,
        );
        sample(
            &mut next_id,
            &mut library,
            WidgetKind::Text,
            292.0,
            464.0,
            860.0,
            32.0,
            "让页面回应你",
            "#FFFFFF00",
            "#243047",
            20.0,
        );
        let message = sample(
            &mut next_id,
            &mut library,
            WidgetKind::Text,
            296.0,
            521.0,
            860.0,
            38.0,
            "你好，创造者。点击按钮，看看会发生什么。",
            "#FFFFFF00",
            "#647085",
            17.0,
        );
        let badge = sample(
            &mut next_id,
            &mut library,
            WidgetKind::Rectangle,
            296.0,
            638.0,
            520.0,
            44.0,
            "✦ 灵感已就位，开始你的下一个作品吧。",
            "#EEEAFE",
            "#6759E8",
            14.0,
        );
        let set = sample(
            &mut next_id,
            &mut library,
            WidgetKind::Button,
            296.0,
            578.0,
            180.0,
            42.0,
            "改变一句话",
            "#6759E8",
            "#FFFFFF",
            14.0,
        );
        library
            .widgets
            .iter_mut()
            .find(|w| w.id == set)
            .unwrap()
            .interaction = Interaction {
            action: Action::SetText,
            target_widget: Some(message),
            value: "好设计，始于一次大胆的尝试。现在轮到你了！".to_owned(),
            ..Default::default()
        };
        let toggle = sample(
            &mut next_id,
            &mut library,
            WidgetKind::Button,
            496.0,
            578.0,
            180.0,
            42.0,
            "显示 / 隐藏提示",
            "#EEEAFE",
            "#6759E8",
            14.0,
        );
        library
            .widgets
            .iter_mut()
            .find(|w| w.id == toggle)
            .unwrap()
            .interaction = Interaction {
            action: Action::ToggleVisibility,
            target_widget: Some(badge),
            ..Default::default()
        };
        let back = sample(
            &mut next_id,
            &mut library,
            WidgetKind::Button,
            696.0,
            578.0,
            180.0,
            42.0,
            "返回工作台 →",
            "#F0F3F8",
            "#647085",
            14.0,
        );
        library
            .widgets
            .iter_mut()
            .find(|w| w.id == back)
            .unwrap()
            .interaction = Interaction {
            action: Action::Navigate,
            target_page: Some(1),
            ..Default::default()
        };
        Self {
            version: FORMAT_VERSION,
            name: "灵感工作台".to_owned(),
            pages: vec![overview, library],
            page_tree: Vec::new(),
            interaction_events: Vec::new(),
            next_id,
        }
    }
}

fn sample_page(id: u64, name: &str) -> Page {
    Page {
        id,
        name: name.to_owned(),
        width: 1280.0,
        height: 800.0,
        canvas: CanvasMode::Fixed,
        responsive_min_width: None,
        background: "#F4F7FB".to_owned(),
        widgets: Vec::new(),
        group_names: Default::default(),
    }
}

#[allow(clippy::too_many_arguments)]
fn sample(
    next_id: &mut u64,
    page: &mut Page,
    kind: WidgetKind,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    text: &str,
    fill: &str,
    color: &str,
    font_size: f32,
) -> u64 {
    let id = *next_id;
    *next_id += 1;
    let mut widget = Widget::new(id, kind, x, y);
    widget.width = width;
    widget.height = height;
    widget.text = text.to_owned();
    widget.name = if text.is_empty() {
        format!("{} {}", kind.label(), id)
    } else {
        text.to_owned()
    };
    widget.fill = fill.to_owned();
    widget.stroke = "#FFFFFF00".to_owned();
    widget.text_color = color.to_owned();
    widget.font_size = font_size;
    if kind == WidgetKind::Panel {
        widget.radius = 14.0;
    }
    if kind == WidgetKind::Panel && x == 0.0 {
        widget.radius = 0.0;
    }
    page.widgets.push(widget);
    id
}
