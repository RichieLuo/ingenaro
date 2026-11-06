use super::*;
use crate::ai::ImageAttachment;

fn image(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> ImageAttachment {
    let mut rgba = Vec::new();
    for y in 0..height {
        for x in 0..width {
            rgba.extend(pixel(x, y));
        }
    }
    ImageAttachment::from_rgba("test", width, height, &rgba).unwrap()
}

#[test]
fn card_preserves_flat_colors_merges_near_swatches_and_ignores_transparency() {
    let card = image(240, 64, |x, _| match x / 48 {
        0 => [0, 110, 215, 255],
        1 => [4, 113, 215, 255],
        2 => [244, 50, 63, 255],
        3 => [250, 250, 250, 255],
        _ => [0, 255, 0, 0],
    });
    let result = extract_colors(&[card]).unwrap();
    assert_eq!(result.sampled, 3);
    assert_eq!(
        result.suggested.first(ColorRole::Accent),
        Some(HexColor([244, 50, 63]))
    );
    assert_eq!(
        result.suggested.first(ColorRole::Background),
        Some(HexColor([250; 3]))
    );
    assert!(
        result
            .suggested
            .first(ColorRole::Text)
            .unwrap()
            .contrast(HexColor([250; 3]))
            >= 4.5
    );
}

#[test]
fn picture_keeps_a_small_distinct_accent_alongside_dominant_colors() {
    let photo = image(200, 120, |x, y| {
        if (70..85).contains(&x) && (40..58).contains(&y) {
            return [255, 66, 35, 255];
        }
        let shade = (x * 17 + y * 7) % 100;
        [36 + shade as u8, 70 + shade as u8, 108 + shade as u8, 255]
    });
    let result = extract_colors(&[photo]).unwrap();
    assert!(
        result.suggested.0[..result.sampled]
            .iter()
            .any(|row| row.hex == HexColor([255, 66, 35]))
    );
    assert!(
        result.suggested.0[..result.sampled]
            .iter()
            .any(|row| row.hex.0[2] > row.hex.0[0])
    );
}

#[test]
fn invalid_proposals_are_atomic_and_locked_rows_require_a_second_choice() {
    let base = Palette(vec![
        Swatch {
            locked: true,
            ..Swatch::new(HexColor([23, 65, 167]), ColorRole::Primary)
        },
        Swatch::new(HexColor([250; 3]), ColorRole::Background),
    ]);
    let patch = PaletteProposal::parse(r##"{"edits":[{"op":"update","index":1,"hex":"#BB0066"},{"op":"update","index":2,"hex":"#FFF4E0"}]}"##).unwrap();
    let review = review_edits(&base, &patch.edits).unwrap();
    assert_eq!(review.draft.0[0], base.0[0]);
    assert_eq!(review.draft.0[1].hex, HexColor([255, 244, 224]));
    assert_eq!(review.conflicts.len(), 1);
    assert!(!review.unlocked_draft.0[0].locked);
    assert_eq!(review.unlocked_draft.0[0].hex, HexColor([187, 0, 102]));
    assert!(review_edits(&base, &[ColorEdit::Remove { index: 0 }]).is_err());
    assert!(
        PaletteProposal::parse(r#"{"edits":[{"op":"add","role":"primary","hex":"blue"}]}"#)
            .is_err()
    );
}
