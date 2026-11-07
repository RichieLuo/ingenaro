use super::*;

impl App {
    pub(super) fn publish_preview(&mut self) {
        if self.native.is_none() {
            self.cached_native_preview_source = None;
        }
        if self.browser_preview.is_none() {
            return;
        }
        let keys = self.preview_page_keys();
        let native = self.native_source_snapshot();
        if let Some(server) = &mut self.browser_preview
            && let Err(error) =
                server.publish_with_native_document(self.project.clone(), keys, native)
        {
            self.status = format!("预览尚未更新：{error:#}");
        }
    }

    pub(super) fn native_source_snapshot(
        &mut self,
    ) -> Option<std::sync::Arc<ingenaro::rp_format::NativeDocument>> {
        let Some(native) = &self.native else {
            self.cached_native_preview_source = None;
            return None;
        };
        let identity = native.preview_identity();
        if let Some((cached_identity, source)) = &self.cached_native_preview_source
            && *cached_identity == identity
        {
            return Some(std::sync::Arc::clone(source));
        }
        let source = std::sync::Arc::new(native.clone());
        self.cached_native_preview_source = Some((identity, std::sync::Arc::clone(&source)));
        Some(source)
    }

    pub(super) fn native_preview(
        &self,
    ) -> anyhow::Result<Option<std::sync::Arc<ingenaro::rp_format::NativePreview>>> {
        self.native
            .as_ref()
            .map(|native| native.preview_scene(&self.project).map(std::sync::Arc::new))
            .transpose()
    }

    pub(super) fn preview_page_keys(&self) -> std::collections::BTreeMap<u64, String> {
        self.native
            .as_ref()
            .map_or_else(Default::default, |native| native.preview_page_keys())
    }

    pub(super) fn start_preview(&mut self, ctx: &egui::Context) {
        let result = (|| -> anyhow::Result<String> {
            let keys = self.preview_page_keys();
            let native = self.native_source_snapshot();
            if let Some(server) = &mut self.browser_preview {
                server.publish_with_native_document(self.project.clone(), keys, native)?;
            } else {
                self.browser_preview =
                    Some(ingenaro::preview::PreviewServer::start_with_native_document(
                        self.project.clone(),
                        keys,
                        native,
                    )?);
            }
            Ok(self
                .browser_preview
                .as_ref()
                .unwrap()
                .url(self.project.pages[self.page].id))
        })();
        match result {
            Ok(url) => {
                ctx.open_url(egui::OpenUrl::new_tab(url));
                self.status = "已在浏览器预览当前页；编辑后刷新浏览器查看最新内容".into();
            }
            Err(error) => self.status = format!("预览失败：{error:#}"),
        }
    }
}
