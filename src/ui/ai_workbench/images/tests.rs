use super::*;
use crate::ui::tests::{fixture, frame};

fn finish_loading(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.ai_image_loading() && Instant::now() < deadline {
        app.ai_poll_images();
        std::thread::yield_now();
    }
    assert!(!app.ai_image_loading(), "Image loading timed out");
}

fn wait_for_texture(
    draft: &mut ImageDraft,
    ctx: &egui::Context,
    image: &ai::ImageAttachment,
) -> egui::TextureHandle {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(texture) = draft.texture(ctx, image).unwrap() {
            return texture;
        }
        assert!(Instant::now() < deadline, "Image preview timed out");
        std::thread::yield_now();
    }
}

fn png() -> Vec<u8> {
    ai::ImageAttachment::from_rgba("reference", 2, 1, &[255, 0, 0, 255, 0, 255, 0, 255])
        .unwrap()
        .png_bytes()
        .to_vec()
}
#[test]
fn drop_image_becomes_attachment_not_project_replacement() {
    let mut app = fixture(Project::blank());
    app.ai.visible = true;
    let before = app.project.clone();
    let context = egui::Context::default();
    let _ = context.run(
        egui::RawInput {
            dropped_files: vec![egui::DroppedFile {
                name: "reference.png".into(),
                mime: "image/png".into(),
                bytes: Some(png().into()),
                ..Default::default()
            }],
            ..Default::default()
        },
        |ctx| app.ai_handle_image_input(ctx),
    );
    finish_loading(&mut app);
    assert_eq!(app.ai.images.len(), 1);
    assert_eq!(app.project, before);
    assert!(app.pending_open.is_none());
    assert!(app.confirm.is_none());
    frame(&mut app, &context, vec![]);
    let _ = wait_for_texture(&mut app.ai.image_draft, &context, &app.ai.images[0]);
    assert!(
        app.ai
            .image_draft
            .thumbnails
            .contains_key(app.ai.images[0].id())
    );
}
#[test]
fn invalid_image_and_too_many_images_leave_draft_and_document_intact() {
    let mut app = fixture(Project::blank());
    let before = app.project.clone();
    let image = ai::ImageAttachment::from_bytes("reference.png", &png()).unwrap();
    app.ai_append_images(vec![image.clone()]).unwrap();
    assert!(app.ai_append_images(vec![image.clone(); 4]).is_err());
    assert_eq!(app.ai.images.len(), 1);
    app.ai_load_images(vec![egui::DroppedFile {
        name: "bad.png".into(),
        mime: "image/png".into(),
        bytes: Some(vec![1, 2, 3].into()),
        ..Default::default()
    }]);
    finish_loading(&mut app);
    assert_eq!(app.ai.images.len(), 1);
    assert_eq!(app.project, before);
}
#[test]
fn late_image_read_after_engine_or_project_switch_is_discarded() {
    let mut app = fixture(Project::blank());
    let (tx, rx) = mpsc::channel();
    app.ai.image_draft.loading = Some(rx);
    app.ai.image_draft.loading_generation = Some(app.ai.generation.clone());
    app.ai_document_replaced();
    let image = ai::ImageAttachment::from_bytes("reference.png", &png()).unwrap();
    assert!(tx.send(Ok(vec![image])).is_err());
    app.ai_poll_images();
    assert!(!app.ai_image_loading());
    assert!(app.ai.images.is_empty());
}

#[test]
fn disconnected_loader_clears_loading_and_allows_retry() {
    let mut app = fixture(Project::blank());
    let (tx, rx) = mpsc::channel();
    app.ai.image_draft.loading = Some(rx);
    app.ai.image_draft.loading_generation = Some(app.ai.generation.clone());
    drop(tx);
    app.ai_poll_images();
    assert!(!app.ai_image_loading());
    assert!(app.ai.image_draft.loading_generation.is_none());
    assert!(app.ai.state.contains("意外结束"));
    app.ai_load_images(vec![egui::DroppedFile {
        name: "reference.png".into(),
        bytes: Some(png().into()),
        ..Default::default()
    }]);
    finish_loading(&mut app);
    assert_eq!(app.ai.images.len(), 1);
}

#[test]
fn image_filename_without_mime_or_path_is_accepted() {
    let mut app = fixture(Project::blank());
    app.ai.visible = true;
    let context = egui::Context::default();
    let _ = context.run(
        egui::RawInput {
            dropped_files: vec![egui::DroppedFile {
                name: "reference.PNG".into(),
                bytes: Some(png().into()),
                ..Default::default()
            }],
            ..Default::default()
        },
        |ctx| {
            app.ai_handle_image_input(ctx);
            assert!(ctx.input(|input| input.raw.dropped_files.is_empty()));
        },
    );
    finish_loading(&mut app);
    assert_eq!(app.ai.images.len(), 1);
    assert!(!is_image_drop(&egui::DroppedFile {
        name: "project.rp".into(),
        ..Default::default()
    }));
}

#[test]
fn capacity_rejects_excess_before_starting_a_worker() {
    let mut app = fixture(Project::blank());
    let image = ai::ImageAttachment::from_bytes("reference.png", &png()).unwrap();
    app.ai_append_images(vec![image; ai::MAX_IMAGE_ATTACHMENTS - 1])
        .unwrap();
    app.ai_load_images(vec![egui::DroppedFile::default(); 2]);
    assert!(!app.ai_image_loading());
    assert!(app.ai.state.contains("最多添加"));
    assert_eq!(app.ai.images.len(), ai::MAX_IMAGE_ATTACHMENTS - 1);
}

#[test]
fn clipboard_errors_distinguish_empty_content_from_read_failures() {
    assert_eq!(
        clipboard::image_error(arboard::Error::ContentNotAvailable),
        clipboard::EMPTY_MESSAGE
    );
    for error in [
        arboard::Error::ClipboardOccupied,
        arboard::Error::ConversionFailure,
    ] {
        let error = clipboard::image_error(error);
        assert_ne!(error, clipboard::EMPTY_MESSAGE);
        let mut app = fixture(Project::blank());
        let (tx, rx) = mpsc::channel();
        app.ai.image_draft.loading = Some(rx);
        app.ai.image_draft.loading_generation = Some(app.ai.generation.clone());
        app.ai.image_draft.quiet_clipboard = true;
        tx.send(Err(error.clone())).unwrap();
        app.ai_poll_images();
        assert_eq!(app.ai.state, error);
        assert!(!app.ai_image_loading());
    }
}

#[test]
fn preview_texture_is_large_enough_for_details_and_is_reused_and_pruned() {
    let mut app = fixture(Project::blank());
    let image =
        ai::ImageAttachment::from_rgba("reference", 640, 320, &vec![150; 640 * 320 * 4]).unwrap();
    app.ai_append_images(vec![image.clone()]).unwrap();
    let ctx = egui::Context::default();
    let texture = wait_for_texture(&mut app.ai.image_draft, &ctx, &image);
    assert!(texture.size()[0] >= 640);
    let same = app.ai.image_draft.texture(&ctx, &image).unwrap().unwrap();
    assert_eq!(texture.id(), same.id());
    assert!(app.ai.image_draft.rendering.is_empty());
    app.ai.images.clear();
    app.ai_poll_images();
    assert!(app.ai.image_draft.thumbnails.is_empty());
    assert!(app.ai.image_draft.rendering.is_empty());
}

#[test]
fn small_previews_are_not_upscaled_and_respect_gpu_texture_limits() {
    let mut draft = ImageDraft::default();
    let ctx = egui::Context::default();
    let small = ai::ImageAttachment::from_bytes("reference.png", &png()).unwrap();
    assert_eq!(wait_for_texture(&mut draft, &ctx, &small).size(), [2, 1]);
    let _ = ctx.run(
        egui::RawInput {
            max_texture_side: Some(1024),
            ..Default::default()
        },
        |_| {},
    );
    let image =
        ai::ImageAttachment::from_rgba("reference", 1600, 800, &vec![150; 1600 * 800 * 4]).unwrap();
    assert_eq!(
        wait_for_texture(&mut draft, &ctx, &image).size(),
        [1024, 512]
    );
}

#[test]
fn completed_shortcut_between_polls_is_detected_without_duplicate_release() {
    let mut down = false;
    assert!(clipboard::paste_edge(&mut down, true, false, false, true));
    assert!(!down);
    assert!(clipboard::paste_edge(&mut down, true, true, false, false));
    assert!(!clipboard::paste_edge(&mut down, true, false, false, false));
    assert!(!clipboard::paste_edge(&mut down, true, false, false, true));
    assert!(!down);
    assert!(clipboard::paste_edge(&mut down, true, false, true, false));
    assert!(!clipboard::paste_edge(&mut down, true, false, false, true));
    assert!(!clipboard::paste_edge(&mut down, false, true, true, false));
    assert!(!down);
}
