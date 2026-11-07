use super::*;
use egui::text_selection::CCursorRange;
use egui::{Event, ImeEvent, Key, Modifiers, PointerButton, TextEdit, text::CCursor};

pub(super) fn handle_keys(ui: &egui::Ui, id: egui::Id, composing: &mut bool) -> bool {
    if !ui.memory(|memory| memory.has_focus(id)) {
        *composing = false;
        return false;
    }
    let queued = ui.ctx().data_mut(|data| {
        data.remove_temp::<Option<Event>>(id.with("edit-command"))
            .flatten()
    });
    ui.input_mut(|input| {
        let mut send = false;
        let events = std::mem::take(&mut input.events);
        let ime_frame = *composing || events.iter().any(|event| matches!(event, Event::Ime(_)));
        for event in events {
            match event {
                Event::Ime(ImeEvent::Preedit(ref text)) => {
                    *composing = !text.is_empty();
                    input.events.push(event);
                }
                Event::Ime(ImeEvent::Commit(text)) => {
                    *composing = false;
                    // Winit can end preedit before delivering the result.
                    // TextEdit replaces the current selection on ordinary text input.
                    if !text.is_empty() && text != "\r" && text != "\n" {
                        input.events.push(Event::Text(text));
                    }
                    input.events.push(Event::Ime(ImeEvent::Disabled));
                }
                Event::Ime(ImeEvent::Disabled) => {
                    if *composing {
                        input
                            .events
                            .push(Event::Ime(ImeEvent::Preedit(String::new())));
                    }
                    *composing = false;
                    input.events.push(event);
                }
                Event::Text(_)
                | Event::Key {
                    key: Key::Enter, ..
                } if ime_frame => {}
                Event::Key {
                    key: Key::Enter,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                    repeat,
                    ..
                } => {
                    send |= !repeat;
                }
                _ => input.events.push(event),
            }
        }
        if let Some(event) = queued {
            input.events.push(event);
        }
        send
    })
}

pub(super) fn selection_before_right_click(ui: &egui::Ui, id: egui::Id) -> Option<CCursorRange> {
    if !ui.input(|input| input.pointer.button_pressed(PointerButton::Secondary)) {
        return None;
    }
    TextEdit::load_state(ui.ctx(), id)?
        .cursor
        .char_range()
        .filter(|range| !range.is_empty())
}

pub(super) fn context_menu(
    response: &egui::Response,
    text: &str,
    selection: Option<CCursorRange>,
) -> bool {
    let ctx = &response.ctx;
    if let Some(selection) = selection
        && response.contains_pointer()
        && let Some(mut state) = TextEdit::load_state(ctx, response.id)
    {
        state.cursor.set_char_range(Some(selection));
        state.store(ctx, response.id);
    }
    let mut paste_image = false;
    let right_clicked = response.contains_pointer()
        && ctx.input(|input| input.pointer.button_clicked(PointerButton::Secondary));
    egui::Popup::context_menu(response)
        .open_memory(right_clicked.then_some(egui::SetOpenCommand::Bool(true)))
        .show(|ui| {
            let has_selection = TextEdit::load_state(ctx, response.id)
                .and_then(|state| state.cursor.char_range())
                .is_some_and(|range| !range.is_empty());
            let queue_edit = |event| {
                response.request_focus();
                ctx.data_mut(|data| {
                    data.insert_temp(response.id.with("edit-command"), Some(event))
                });
                ctx.request_repaint();
            };
            if ui
                .add_enabled(has_selection, egui::Button::new("剪切"))
                .clicked()
            {
                queue_edit(Event::Cut);
                ui.close();
            }
            if ui
                .add_enabled(has_selection, egui::Button::new("复制"))
                .clicked()
            {
                queue_edit(Event::Copy);
                ui.close();
            }
            if ui.button("粘贴").clicked() {
                if let Ok(text) =
                    arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get_text())
                    && !text.is_empty()
                {
                    queue_edit(Event::Paste(text.replace("\r\n", "\n")));
                } else {
                    paste_image = true;
                }
                ui.close();
            }
            if ui
                .add_enabled(!text.is_empty(), egui::Button::new("全选"))
                .clicked()
                && let Some(mut state) = TextEdit::load_state(ctx, response.id)
            {
                state.cursor.set_char_range(Some(CCursorRange::two(
                    CCursor::new(0),
                    CCursor::new(text.chars().count()),
                )));
                state.store(ctx, response.id);
                response.request_focus();
                ui.close();
            }
        });
    paste_image
}
