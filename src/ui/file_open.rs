use super::*;

impl App {
    pub(super) fn poll_shell_open(&mut self, ctx: &egui::Context) {
        if self.screenshot_mode {
            return;
        }
        let ready = self.confirm.is_none()
            && self.pending_open.is_none()
            && self.editing.is_none()
            && self.drag.is_none();
        let request = if ready {
            crate::shell_open::poll(ctx)
        } else {
            crate::shell_open::poll_when(ctx, false)
        };
        match request {
            Ok(Some(paths)) => {
                if let Some(path) = paths.into_iter().next() {
                    if !path.is_file()
                        || !path.extension().is_some_and(|extension| {
                            ["rp", "rplib", "ingenaro", "rpshop", "json"]
                                .iter()
                                .any(|kind| extension.eq_ignore_ascii_case(kind))
                        })
                    {
                        self.status =
                            format!("打开失败：{} 不是已有的 RP / Ingenaro 工程", path.display());
                        return;
                    }
                    self.pending_open = Some(path);
                    self.request_replace("open-path");
                }
            }
            Err(error) => self.status = format!("读取文件打开请求失败：{error}"),
            Ok(None) => {}
        }
    }

    pub(super) fn cancel_replace(&mut self) {
        if self.confirm.as_deref() == Some("open-path") {
            self.pending_open = None;
        }
        self.confirm = None;
    }
}

#[cfg(test)]
mod tests;
