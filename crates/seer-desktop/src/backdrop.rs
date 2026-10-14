use std::{
    fs::File,
    io::{self, Cursor, Read},
    path::Path,
    sync::{Arc, OnceLock},
};

use gpui::RenderImage;
use image::{DynamicImage, Frame, ImageReader, Limits};

static DEFAULT: OnceLock<Option<Arc<RenderImage>>> = OnceLock::new();

pub(crate) fn default_picture() -> Option<Arc<RenderImage>> {
    DEFAULT
        .get_or_init(|| decode(include_bytes!("../assets/backdrop.jpg")).ok())
        .clone()
}

pub(crate) fn read(path: &Path) -> io::Result<Arc<RenderImage>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(io::Error::other("Picture is too large"));
    }
    decode(&bytes)
}

fn decode(bytes: &[u8]) -> io::Result<Arc<RenderImage>> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(io::Error::other)?;
    Ok(prepare(image))
}

fn prepare(image: DynamicImage) -> Arc<RenderImage> {
    // GPUI 0.2.2 has no backdrop filter. Prepare the glass once, before upload.
    let mut pixels = image.thumbnail(1600, 1200).blur(6.).to_rgba8();
    let mut seed = 498_u32;
    for pixel in pixels.pixels_mut() {
        let [r, g, b, alpha] = pixel.0.map(f32::from);
        let gray = r * 0.2126 + g * 0.7152 + b * 0.0722;
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let grain = ((seed >> 24) as f32 / 255. - 0.5) * 255. * 0.035;
        for channel in &mut pixel.0[..3] {
            let saturated = gray + (f32::from(*channel) - gray) * 1.17;
            let contrasted = (saturated - 127.5) * 0.89 + 127.5;
            *channel = (contrasted * alpha / 255. * 0.18 + grain).clamp(0., 255.) as u8;
        }
        pixel.0[3] = 255;
        pixel.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![Frame::new(pixels)]))
}
