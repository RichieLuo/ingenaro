use eframe::egui::IconData;

#[path = "branding_icon.rs"]
mod branding_icon;

pub fn icon_data() -> IconData {
    let image = image::imageops::resize(
        &branding_icon::icon_image(),
        256,
        256,
        image::imageops::FilterType::Lanczos3,
    );
    IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn title_texture(ctx: &eframe::egui::Context) -> eframe::egui::TextureHandle {
    let icon = icon_data();
    let image = eframe::egui::ColorImage::from_rgba_unmultiplied(
        [icon.width as usize, icon.height as usize],
        &icon.rgba,
    );
    ctx.load_texture(
        "ingenaro-title-icon",
        image,
        eframe::egui::TextureOptions::LINEAR,
    )
}
