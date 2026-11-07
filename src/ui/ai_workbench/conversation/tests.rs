use super::*;
use crate::ui::tests::{click, fixture, frame, right_click, text_center};

#[test]
fn assistant_renders_markdown_and_copy_preserves_raw_source() {
    let mut app = fixture(Project::blank());
    app.ai.visible = true;
    let raw = "## 已完成\n\n**重点** 与 `selectedRoute`。\n\n- 保留导航\n- 更新卡片\n\n| 页面 | 状态 |\n| --- | --- |\n| 首页 | 已更新 |\n\n```rust\nlet ready = true;\n```";
    app.ai
        .messages
        .push(response::Message::new(ChatRole::Assistant, raw));
    let original = app.project.clone();
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    frame(&mut app, &ctx, vec![]);
    let output = frame(&mut app, &ctx, vec![]);
    assert!(text_center(&output, "已完成").x > 1000.0);
    let copy = ctx
        .data(|data| data.get_temp::<Rect>(egui::Id::new("ai-message-copy-test")))
        .unwrap()
        .center();
    let output = click(&mut app, &ctx, copy);
    assert!(
        output
            .platform_output
            .commands
            .iter()
            .any(|command| matches!(command, egui::OutputCommand::CopyText(text) if text == raw))
    );
    assert_eq!(app.ai.messages[0].text, raw);
    assert_eq!(app.project, original);
    assert!(app.undo.is_empty());
}

#[test]
fn assistant_context_menu_copies_rendered_selection_or_original_message() {
    let mut app = fixture(Project::blank());
    app.ai.visible = true;
    let source = "**alpha** beta";
    app.ai
        .messages
        .push(response::Message::new(ChatRole::Assistant, source));
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    let output = frame(&mut app, &ctx, vec![]);
    let alpha = text_center(&output, "alpha");
    click(&mut app, &ctx, alpha);
    click(&mut app, &ctx, alpha);

    let menu = right_click(&mut app, &ctx, alpha);
    click(&mut app, &ctx, text_center(&menu, "复制选中文字"));
    let output = frame(&mut app, &ctx, vec![]);
    assert!(
        output.platform_output.commands.iter().any(
            |command| matches!(command, egui::OutputCommand::CopyText(text) if text == "alpha")
        )
    );

    let menu = right_click(&mut app, &ctx, alpha);
    let output = click(&mut app, &ctx, text_center(&menu, "复制整条消息"));
    assert!(
        output.platform_output.commands.iter().any(
            |command| matches!(command, egui::OutputCommand::CopyText(text) if text == source)
        )
    );
}

fn draw_markdown(
    ctx: &egui::Context,
    width: f32,
    text: &str,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut cache = CommonMarkCache::default();
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 600.0))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| markdown(ui, &mut cache, text));
            });
        },
    )
}

#[test]
fn chinese_table_uses_the_chat_width_and_reflows_when_resized() {
    let ctx = egui::Context::default();
    crate::ui::theme::setup_style(&ctx);
    let details = "统一管理客户、供应商、商品分类和仓库资料，支持导入与批量编辑。";
    let source = format!(
        "建议补充以下功能：\n\n| 模块 | 应包含的基础功能 |\n| --- | --- |\n| **基础资料** | {details} |\n\n下一段正文"
    );
    let render = |width| {
        let output = draw_markdown(&ctx, width, &source, vec![]);
        let rows = |text| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(shape) if shape.galley.text() == text => {
                        Some(shape.galley.rows.len())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing table text: {text}"))
        };
        assert_eq!(rows("应包含的基础功能"), 1);
        assert!(text_center(&output, "应包含的基础功能").x > text_center(&output, "模块").x);
        assert!(text_center(&output, "下一段正文").y > text_center(&output, details).y);
        rows(details)
    };
    let narrow = render(252.0);
    let wide = render(420.0);
    assert!(narrow <= 4 && wide < narrow, "narrow={narrow}, wide={wide}");
    assert_eq!(render(252.0), narrow);
}

#[test]
fn wide_table_scrolls_sideways_without_moving_the_chat_text() {
    let ctx = egui::Context::default();
    let text = "| First column | Column two | Column three | Column four | Last column |\n\
                | --- | --- | --- | --- | --- |\n\
                | A | B | C | D | E |\n\nText after the table";
    let draw = |events| draw_markdown(&ctx, 252.0, text, events);
    draw(vec![]);
    let before = draw(vec![]);
    let pointer = text_center(&before, "First column");
    let footer = text_center(&before, "Text after the table");
    draw(vec![
        egui::Event::PointerMoved(pointer),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(-1000.0, 0.0),
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    let after = draw(vec![]);
    let last = text_center(&after, "Last column");
    assert!((0.0..252.0).contains(&last.x), "{last:?}");
    assert_eq!(text_center(&after, "Text after the table"), footer);
}

#[test]
fn table_inside_a_quote_preserves_reference_links_and_blocks_unsafe_ones() {
    let ctx = egui::Context::default();
    let text = "> | Docs | Location |\n\
                > | --- | --- |\n\
                > | [Open docs][web] | [Open file](file:///private.txt) |\n\n\
                [web]: https://example.com/help";
    let draw = |events| draw_markdown(&ctx, 360.0, text, events);
    let initial = draw(vec![]);
    for (label, allowed) in [("Open docs", true), ("Open file", false)] {
        let pos = text_center(&initial, label);
        draw(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        let output = draw(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        let opened = output.platform_output.commands.iter().find_map(|command| {
            if let egui::OutputCommand::OpenUrl(url) = command {
                Some(url.url.as_str())
            } else {
                None
            }
        });
        assert_eq!(opened, allowed.then_some("https://example.com/help"));
    }
}

#[test]
fn user_message_is_a_right_aligned_bounded_bubble_and_plain_text_stays_plain() {
    let ctx = egui::Context::default();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 400.0))),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let id = egui::Id::new("ai-user-bubble-test");
                user_message(ui, "**这是用户原文**，不是助手的 Markdown。", 350.0);
                let rect = ctx.data(|data| data.get_temp::<Rect>(id)).unwrap();
                assert!(rect.width() <= 350.0 * 0.85 + 2.0, "{rect:?}");
                assert!(rect.right() > 340.0, "{rect:?}");
            });
        },
    );
    assert!(text_center(&output, "**这是用户原文**，不是助手的 Markdown。").x > 50.0);
}

#[test]
fn unsafe_link_schemes_are_not_opened_by_markdown() {
    for url in [
        "file:///C:/Windows/system.ini",
        "javascript:alert(1)",
        "data:text/html,test",
        "C:/secret.txt",
        "https://safe.test/\nfile",
    ] {
        assert!(!safe_link(url));
    }
    for url in [
        "https://example.com",
        "http://localhost:8080",
        "mailto:test@example.com",
    ] {
        assert!(safe_link(url));
    }
    let ctx = egui::Context::default();
    let mut cache = CommonMarkCache::default();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            markdown(ui, &mut cache, "[不可执行](javascript:alert)")
        });
    });
    assert_eq!(cache.get_link_hook("javascript:alert"), Some(false));
}

#[test]
fn referenced_links_keep_document_scope_and_unsafe_hooks_are_not_retained_between_messages() {
    let ctx = egui::Context::default();
    let mut cache = CommonMarkCache::default();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            markdown(
                ui,
                &mut cache,
                "[路径][reference]\n\n[reference]: file:///secret.txt",
            );
            assert_eq!(cache.get_link_hook("file:///secret.txt"), Some(false));
            markdown(ui, &mut cache, "下一条消息");
            assert!(cache.link_hooks().is_empty());
        });
    });
}

#[test]
fn growing_streaming_markdown_and_unclosed_fence_leave_editor_state_intact() {
    let mut app = fixture(Project::blank());
    app.ai.visible = true;
    let ctx = egui::Context::default();
    let original = app.project.clone();
    for text in [
        "## 设计",
        "## 设计\n\n**正在",
        "## 设计\n\n**正在绘制**\n\n```json\n{\"page\":",
        "## 设计\n\n**正在绘制**\n\n```json\n{\"page\":1}\n```",
    ] {
        app.ai.response.text = text.into();
        frame(&mut app, &ctx, vec![]);
    }
    assert_eq!(app.project, original);
    assert!(app.undo.is_empty());
}
