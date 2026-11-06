//! Validated, memory-only image inputs. Source files and metadata are never retained.
//! CLI-owned conversation/session storage is outside RPShop's persistence boundary.
use anyhow::{Result, anyhow, bail};
use base64::Engine as _;
use image::{ImageDecoder, ImageEncoder, ImageFormat, ImageReader};
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::Arc;

pub const MAX_IMAGE_ATTACHMENTS: usize = 4;
pub const MAX_IMAGE_SOURCE_BYTES: usize = 12 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: usize = 6 * 1024 * 1024;
pub const MAX_TOTAL_IMAGE_BYTES: usize = 12 * 1024 * 1024;
pub const MAX_IMAGE_EDGE: u32 = 8192;
pub const MAX_IMAGE_PIXELS: u64 = 32_000_000;
const MAX_DECODE_BYTES: u64 = MAX_IMAGE_PIXELS * 8 + 32 * 1024 * 1024;

/// A single normalized PNG, shared cheaply between composer/queue snapshots.
/// Deliberately not serializable: image inputs are not project attachments.
#[derive(Clone)]
pub struct ImageAttachment {
    id: Arc<str>,
    name: Arc<str>,
    width: u32,
    height: u32,
    png: Arc<[u8]>,
}
impl std::fmt::Debug for ImageAttachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Neither source names/paths nor encoded image contents belong in logs.
        f.debug_struct("ImageAttachment")
            .field("id", &self.id)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("byte_len", &self.byte_len())
            .finish_non_exhaustive()
    }
}
impl ImageAttachment {
    /// Read at most the source limit, even if the file grows after metadata is read.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = std::fs::File::open(path).map_err(|_| anyhow!("Cannot open image file"))?;
        let metadata = file
            .metadata()
            .map_err(|_| anyhow!("Cannot inspect image file"))?;
        if !metadata.is_file() {
            bail!("Image input must be a regular file");
        }
        if metadata.len() > MAX_IMAGE_SOURCE_BYTES as u64 {
            bail!("Image source exceeds the 12 MiB limit");
        }
        let mut bytes = Vec::new();
        file.take(MAX_IMAGE_SOURCE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| anyhow!("Cannot read image file"))?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "image".into());
        Self::from_bytes(name, &bytes)
    }

    /// Accept PNG/JPEG/GIF/WebP bytes, verify any declared filename format, then
    /// decode under resource limits and re-encode a metadata-free, single-frame PNG.
    pub fn from_bytes(name: impl Into<String>, bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() {
            bail!("Image source is empty");
        }
        if bytes.len() > MAX_IMAGE_SOURCE_BYTES {
            bail!("Image source exceeds the 12 MiB limit");
        }
        let name = name.into();
        let format = image::guess_format(bytes)
            .map_err(|_| anyhow!("Image is not a supported PNG, JPEG, GIF or WebP"))?;
        if !matches!(
            format,
            ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif | ImageFormat::WebP
        ) {
            bail!("Image is not a supported PNG, JPEG, GIF or WebP");
        }
        validate_declared_format(&name, format)?;
        let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_IMAGE_EDGE);
        limits.max_image_height = Some(MAX_IMAGE_EDGE);
        limits.max_alloc = Some(MAX_DECODE_BYTES);
        reader.limits(limits);
        // Header/dimension checks precede allocation of the decoded pixel buffer.
        let mut decoder = reader
            .into_decoder()
            .map_err(|_| anyhow!("Image header is invalid or exceeds decoding limits"))?;
        let (width, height) = decoder.dimensions();
        validate_dimensions(width, height)?;
        if decoder.total_bytes() > MAX_DECODE_BYTES {
            bail!("Decoded image exceeds the memory limit");
        }
        let orientation = decoder
            .orientation()
            .map_err(|_| anyhow!("Image orientation metadata is invalid"))?;
        let mut decoded = image::DynamicImage::from_decoder(decoder)
            .map_err(|_| anyhow!("Image data is invalid or exceeds decoding limits"))?;
        decoded.apply_orientation(orientation);
        let rgba = decoded.into_rgba8();
        Self::from_rgba(name, rgba.width(), rgba.height(), rgba.as_raw())
    }

    /// Clipboard RGBA is already decoded, so its length is checked against its
    /// dimensions rather than the encoded-source limit. It is never resized.
    pub fn from_rgba(
        name: impl Into<String>,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<Self> {
        validate_dimensions(width, height)?;
        let expected = u64::from(width) * u64::from(height) * 4;
        if expected != rgba.len() as u64 {
            bail!("Image RGBA length does not match its dimensions");
        }
        let mut output = BoundedPng(Vec::new());
        image::codecs::png::PngEncoder::new(&mut output)
            .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
            .map_err(|_| anyhow!("Normalized PNG exceeds the 6 MiB limit or cannot be encoded"))?;
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string().into(),
            name: display_name(&name.into()).into(),
            width,
            height,
            png: output.0.into(),
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn media_type(&self) -> &'static str {
        "image/png"
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn byte_len(&self) -> usize {
        self.png.len()
    }
    pub fn png_bytes(&self) -> &[u8] {
        &self.png
    }
    pub fn base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(&self.png)
    }
    pub fn data_url(&self) -> String {
        format!("data:{};base64,{}", self.media_type(), self.base64())
    }
}

/// Validate a complete turn before starting any CLI or network worker.
pub fn validate_image_attachments(images: &[ImageAttachment]) -> Result<()> {
    if images.len() > MAX_IMAGE_ATTACHMENTS {
        bail!("A turn supports at most 4 images");
    }
    let mut total = 0usize;
    for image in images {
        if image.byte_len() > MAX_IMAGE_BYTES {
            bail!("Normalized PNG exceeds the 6 MiB limit");
        }
        total = total
            .checked_add(image.byte_len())
            .ok_or_else(|| anyhow!("Image attachments exceed the 12 MiB total limit"))?;
        if total > MAX_TOTAL_IMAGE_BYTES {
            bail!("Image attachments exceed the 12 MiB total limit");
        }
    }
    Ok(())
}
fn validate_dimensions(width: u32, height: u32) -> Result<()> {
    if width == 0 || height == 0 {
        bail!("Image dimensions must be nonzero");
    }
    if width > MAX_IMAGE_EDGE || height > MAX_IMAGE_EDGE {
        bail!("Image dimensions exceed the 8192 pixel edge limit");
    }
    if u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        bail!("Image exceeds the 32 million pixel limit");
    }
    Ok(())
}
fn validate_declared_format(name: &str, actual: ImageFormat) -> Result<()> {
    if let Some(extension) = Path::new(name).extension() {
        let declared = extension.to_str().and_then(ImageFormat::from_extension);
        if declared != Some(actual) {
            bail!("Image filename format does not match its actual contents");
        }
    }
    Ok(())
}
fn display_name(name: &str) -> String {
    let name = name.rsplit(['/', '\\']).next().unwrap_or("");
    let name: String = name.chars().filter(|c| !c.is_control()).take(160).collect();
    let name = name.trim();
    if name.is_empty() {
        "image".into()
    } else {
        name.into()
    }
}

/// Stop the encoder before allocating an unbounded compressed output buffer.
struct BoundedPng(Vec<u8>);
impl Write for BoundedPng {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_IMAGE_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other(
                "Normalized PNG exceeds the size limit",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
