use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use image::{AnimationDecoder, codecs::gif::GifDecoder};
use std::io::Cursor;

const GIF: &[u8] = include_bytes!("../../../assets/branding/naro-weaving.gif");

pub(super) struct NaroAnimation {
    frames: Vec<Frame>,
    duration: f64,
}

struct Frame {
    texture: TextureHandle,
    end: f64,
}

impl NaroAnimation {
    pub(super) fn load(ctx: &egui::Context) -> Self {
        let decoder = GifDecoder::new(Cursor::new(GIF)).expect("embedded Naro GIF header");
        let mut duration = 0.0;
        let frames = decoder
            .into_frames()
            .map(|frame| {
                let frame = frame.expect("embedded Naro GIF frame");
                let (numerator, denominator) = frame.delay().numer_denom_ms();
                duration += (f64::from(numerator) / f64::from(denominator) / 1000.0).max(0.01);
                let pixels = frame.into_buffer();
                let size = [pixels.width() as usize, pixels.height() as usize];
                let texture = ctx.load_texture(
                    "ingenaro-naro-weaving",
                    ColorImage::from_rgba_unmultiplied(size, pixels.as_raw()),
                    TextureOptions::LINEAR,
                );
                Frame {
                    texture,
                    end: duration,
                }
            })
            .collect::<Vec<_>>();
        assert!(!frames.is_empty(), "embedded Naro GIF has frames");
        Self { frames, duration }
    }

    pub(super) fn frame_at(&self, time: f64) -> &TextureHandle {
        let elapsed = time.rem_euclid(self.duration);
        let index = self.frames.partition_point(|frame| frame.end <= elapsed);
        &self.frames[index.min(self.frames.len() - 1)].texture
    }
}
