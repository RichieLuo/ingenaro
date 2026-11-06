//! Clipboard access runs only after an explicit paste or a focused composer Ctrl+V.
use super::*;

pub(super) const EMPTY_MESSAGE: &str = "剪贴板中没有图片，可选择图片文件或拖入图片";

pub(super) fn read() -> Result<Vec<ai::ImageAttachment>, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|_| "无法读取系统剪贴板")?;
    let image = clipboard.get_image().map_err(image_error)?;
    let width = u32::try_from(image.width).map_err(|_| "剪贴板图片尺寸无效")?;
    let height = u32::try_from(image.height).map_err(|_| "剪贴板图片尺寸无效")?;
    let attachment = ai::ImageAttachment::from_rgba("剪贴板图片", width, height, &image.bytes)
        .map_err(|error| error.to_string())?;
    Ok(vec![attachment])
}

pub(super) fn image_error(error: arboard::Error) -> String {
    match error {
        arboard::Error::ContentNotAvailable => EMPTY_MESSAGE.into(),
        arboard::Error::ClipboardOccupied => "系统剪贴板正被占用，请重试".into(),
        arboard::Error::ConversionFailure => "无法转换剪贴板图片，请选择图片文件".into(),
        _ => "无法读取系统剪贴板图片，请重试".into(),
    }
}

#[cfg(any(target_os = "windows", test))]
pub(super) fn paste_edge(
    was_down: &mut bool,
    focused: bool,
    down: bool,
    pasted: bool,
    released: bool,
) -> bool {
    if !focused {
        *was_down = false;
        return false;
    }
    let once = (down || pasted || released) && !*was_down;
    if released {
        *was_down = false;
    } else if down || pasted {
        *was_down = true;
    }
    once
}

pub(super) fn pressed(ctx: &egui::Context, was_down: &mut bool) -> bool {
    #[cfg(target_os = "windows")]
    {
        // The native backend consumes the shortcut's key-down. Its paste and
        // key-up events also cover chords completed between two repaint polls.
        let (focused, pasted, released) = ctx.input(|input| {
            let pasted = input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Paste(_)));
            let released = input.events.iter().any(|event| {
                matches!(event, egui::Event::Key {
                    key: egui::Key::V, pressed: false, modifiers, ..
                } if modifiers.command || *was_down)
            });
            (input.viewport().focused.unwrap_or(false), pasted, released)
        });
        let down = focused
            && unsafe {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x11) < 0
                    && windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x56) < 0
            };
        paste_edge(was_down, focused, down, pasted, released)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = was_down;
        ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::V))
    }
}
