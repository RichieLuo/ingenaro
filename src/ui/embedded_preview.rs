//! A script-only, opaque-origin HTML preview. No native IPC is exposed to generated code.
use super::*;

#[derive(Default)]
pub(super) struct EmbeddedPreview {
    #[cfg(target_os = "windows")]
    view: Option<wry::WebView>,
    loaded: String,
    pub error: Option<String>,
}

impl EmbeddedPreview {
    pub fn hide(&mut self) {
        #[cfg(target_os = "windows")]
        if let Some(view) = &self.view {
            let _ = view.set_visible(false);
        }
    }

    pub fn clear(&mut self) {
        #[cfg(target_os = "windows")]
        {
            self.view = None;
        }
        self.loaded.clear();
        self.error = None;
    }

    pub fn show(&mut self, frame: &eframe::Frame, ctx: &egui::Context, rect: Rect, html: &str) {
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (frame, ctx, rect, html);
            self.error =
                Some("当前平台请使用浏览器交互预览；应用内 WebView 目前支持 Windows。".into());
        }
        #[cfg(target_os = "windows")]
        {
            let scale = ctx.pixels_per_point() as f64;
            let bounds = wry::Rect {
                position: wry::dpi::PhysicalPosition::new(
                    rect.left() as f64 * scale,
                    rect.top() as f64 * scale,
                )
                .into(),
                size: wry::dpi::PhysicalSize::new(
                    (rect.width() as f64 * scale).max(1.0) as u32,
                    (rect.height() as f64 * scale).max(1.0) as u32,
                )
                .into(),
            };
            // Rebuild for a new immutable source; never interpolate generated code into evaluate_script.
            if self.loaded != html {
                self.view = None;
                self.loaded.clear();
                self.error = None;
            }
            if self.view.is_none() && self.error.is_none() {
                let wrapper = match ingenaro::design::preview_document(html) {
                    Ok(wrapper) => wrapper,
                    Err(error) => {
                        self.error = Some(error.to_string());
                        return;
                    }
                };
                // Wry/WebView2 may implement with_html as a data URL. Permit only
                // this exact host-authored wrapper, never arbitrary data navigation.
                let initial_data_url = format!(
                    "data:text/html;charset=utf-8;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(wrapper.as_bytes())
                );
                match wry::WebViewBuilder::new()
                    .with_bounds(bounds)
                    .with_html(wrapper)
                    .with_devtools(false)
                    .with_clipboard(false)
                    .with_download_started_handler(|_, _| false)
                    .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
                    .with_navigation_handler(move |url| {
                        matches!(url.as_str(), "about:blank" | "about:srcdoc") || url == initial_data_url
                    })
                    .build_as_child(frame)
                {
                    Ok(view) => { self.view = Some(view); self.loaded = html.to_owned(); }
                    Err(_) => self.error = Some("应用内预览无法启动。请安装 Microsoft Edge WebView2 Runtime，或使用浏览器预览。".into()),
                }
            }
            if let Some(view) = &self.view {
                let _ = view.set_bounds(bounds);
                let _ = view.set_visible(true);
            }
        }
    }
}
