use super::*;

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            &vec![97; width as usize * height as usize * 4],
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    bytes
}

#[test]
fn image_normalizes_to_png_and_queue_clones_share_bytes_and_identity() {
    let attachment = ImageAttachment::from_bytes("reference.png", &png(3, 2)).unwrap();
    assert_eq!((attachment.width(), attachment.height()), (3, 2));
    assert_eq!(attachment.media_type(), "image/png");
    assert_eq!(attachment.byte_len(), attachment.png_bytes().len());
    assert_eq!(
        image::guess_format(attachment.png_bytes()).unwrap(),
        ImageFormat::Png
    );
    let clone = attachment.clone();
    assert_eq!(clone.id(), attachment.id());
    assert!(std::ptr::eq(clone.png_bytes(), attachment.png_bytes()));
    assert!(Arc::ptr_eq(&clone.png, &attachment.png));
    assert!(
        base64::engine::general_purpose::STANDARD
            .decode(attachment.base64())
            .unwrap()
            == attachment.png_bytes()
    );
    assert!(attachment.data_url() == format!("data:image/png;base64,{}", attachment.base64()));
}

#[test]
fn non_png_supported_formats_are_decoded_not_renamed() {
    let rgb = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        4,
        3,
        image::Rgb([80, 120, 160]),
    ));
    for (format, name) in [
        (ImageFormat::Jpeg, "photo.jpeg"),
        (ImageFormat::Gif, "reference.gif"),
        (ImageFormat::WebP, "reference.webp"),
    ] {
        let mut encoded = Cursor::new(Vec::new());
        rgb.write_to(&mut encoded, format).unwrap();
        let attachment = ImageAttachment::from_bytes(name, encoded.get_ref()).unwrap();
        assert_eq!((attachment.width(), attachment.height()), (4, 3));
        assert_eq!(attachment.media_type(), "image/png");
        assert!(attachment.png_bytes().starts_with(b"\x89PNG\r\n\x1a\n"));
    }
}

#[test]
fn validates_actual_format_instead_of_trusting_extension_or_mime() {
    let bytes = png(1, 1);
    assert!(ImageAttachment::from_bytes("fake.jpg", &bytes).is_err());
    assert!(ImageAttachment::from_bytes("script.svg", &bytes).is_err());
    assert!(ImageAttachment::from_bytes("image", &bytes).is_ok());
    assert!(ImageAttachment::from_bytes("image.png", b"not an image").is_err());
    assert!(ImageAttachment::from_bytes("image.png", b"<svg/>").is_err());
    assert!(ImageAttachment::from_bytes("image.png", &[]).is_err());
    let mut corrupt = bytes;
    corrupt.truncate(corrupt.len() / 2);
    assert!(ImageAttachment::from_bytes("image.png", &corrupt).is_err());
}

#[test]
fn image_dimensions_and_rgba_length_are_checked_before_encoding() {
    for (width, height) in [
        (0, 1),
        (1, 0),
        (8193, 1),
        (8192, 8192),
        (u32::MAX, u32::MAX),
    ] {
        assert!(ImageAttachment::from_rgba("clipboard", width, height, &[]).is_err());
    }
    assert!(ImageAttachment::from_rgba("clipboard", 1, 1, &[0; 3]).is_err());
    assert!(ImageAttachment::from_rgba("clipboard", 1, 1, &[0; 5]).is_err());
    assert!(ImageAttachment::from_rgba("clipboard", 1, 1, &[0; 4]).is_ok());
    assert!(validate_dimensions(8192, 1).is_ok());
    assert!(validate_dimensions(8000, 4000).is_ok());
    assert!(validate_dimensions(8000, 4001).is_err());
}

#[test]
fn oversized_source_is_rejected_without_attempting_decode() {
    let bytes = vec![0; MAX_IMAGE_SOURCE_BYTES + 1];
    let error = ImageAttachment::from_bytes("private-name.png", &bytes).unwrap_err();
    assert_eq!(error.to_string(), "Image source exceeds the 12 MiB limit");
    assert!(!error.to_string().contains("private-name"));
}

#[test]
fn png_header_dimensions_are_rejected_before_loading_pixel_data() {
    // A valid tiny PNG IHDR except for oversized dimensions, with an updated
    // CRC. No IDAT decode is needed to reject it.
    let mut bytes = png(1, 1);
    bytes[16..20].copy_from_slice(&8192u32.to_be_bytes());
    bytes[20..24].copy_from_slice(&8192u32.to_be_bytes());
    let mut crc = !0u32;
    for byte in &bytes[12..29] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    bytes[29..33].copy_from_slice(&(!crc).to_be_bytes());
    assert_eq!(
        ImageAttachment::from_bytes("oversized.png", &bytes)
            .unwrap_err()
            .to_string(),
        "Image exceeds the 32 million pixel limit"
    );
}

#[test]
fn display_name_discards_paths_controls_and_debug_never_contains_input() {
    let image = ImageAttachment::from_rgba(
        "C:\\private\\customer-secret\n.png",
        1,
        1,
        &[255, 0, 0, 255],
    )
    .unwrap();
    assert_eq!(image.name(), "customer-secret.png");
    let debug = format!("{image:?}");
    for private in ["customer-secret", "C:", &image.base64(), &image.data_url()] {
        assert!(!debug.contains(private));
    }
    assert_eq!(display_name("/private/"), "image");
}

#[test]
fn file_input_is_snapshotted_and_rejects_non_files_without_path_leaks() {
    let dir = std::env::temp_dir().join(format!("rpshop-image-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("private-reference.png");
    std::fs::write(&path, png(2, 2)).unwrap();
    let image = ImageAttachment::from_file(&path).unwrap();
    assert_eq!(image.name(), "private-reference.png");
    std::fs::remove_file(&path).unwrap();
    assert_eq!(image.width(), 2);
    let error = ImageAttachment::from_file(&path).unwrap_err().to_string();
    assert!(!error.contains("private-reference"));
    assert!(ImageAttachment::from_file(&dir).is_err());
    std::fs::remove_dir(&dir).unwrap();
}

#[test]
fn count_aggregate_and_normalized_size_limits_are_enforced() {
    let image = ImageAttachment::from_rgba("clipboard", 1, 1, &[0; 4]).unwrap();
    assert!(validate_image_attachments(&[]).is_ok());
    assert!(validate_image_attachments(&vec![image.clone(); 4]).is_ok());
    assert!(validate_image_attachments(&vec![image.clone(); 5]).is_err());
    // Private-field synthetic snapshots avoid expensive compression/allocation
    // just to exercise size boundaries (public constructors cannot bypass them).
    let mut large = image;
    large.png = vec![0; MAX_IMAGE_BYTES].into();
    assert!(validate_image_attachments(&[large.clone(), large.clone()]).is_ok());
    assert!(validate_image_attachments(&[large.clone(), large.clone(), large.clone()]).is_err());
    large.png = vec![0; MAX_IMAGE_BYTES + 1].into();
    assert!(validate_image_attachments(&[large]).is_err());
    let mut sink = BoundedPng(Vec::new());
    sink.write_all(&vec![0; MAX_IMAGE_BYTES]).unwrap();
    assert!(sink.write_all(&[0]).is_err());
    assert_eq!(sink.0.len(), MAX_IMAGE_BYTES);
}

#[test]
fn noisy_clipboard_image_is_rejected_if_normalized_png_exceeds_limit() {
    let mut seed = 0x7865_ab34u32;
    let rgba: Vec<u8> = (0..1536 * 1536 * 4)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as u8
        })
        .collect();
    let error = ImageAttachment::from_rgba("private-pixels", 1536, 1536, &rgba)
        .unwrap_err()
        .to_string();
    assert!(error.contains("6 MiB"));
    assert!(!error.contains("private-pixels"));
}
