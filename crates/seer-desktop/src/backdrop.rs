use std::{
    fs::File,
    io::{self, Cursor, Read},
    path::Path,
    sync::{Arc, OnceLock},
};

use gpui::RenderImage;
use image::{DynamicImage, Frame, ImageDecoder, ImageReader, Limits, imageops::FilterType};

static DEFAULT: OnceLock<Option<Arc<Picture>>> = OnceLock::new();

pub(crate) fn default_picture() -> Option<Arc<Picture>> {
    DEFAULT
        .get_or_init(|| decode(include_bytes!("../assets/backdrop.jpg")).ok())
        .clone()
}

pub(crate) fn read(path: &Path) -> io::Result<Arc<Picture>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(io::Error::other("Picture is too large"));
    }
    decode(&bytes)
}

fn decode(bytes: &[u8]) -> io::Result<Arc<Picture>> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits.clone());
    let mut decoder = reader.into_decoder().map_err(io::Error::other)?;
    limits
        .reserve(decoder.total_bytes())
        .map_err(io::Error::other)?;
    decoder.set_limits(limits).map_err(io::Error::other)?;
    let orientation = decoder.orientation().map_err(io::Error::other)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(io::Error::other)?;
    image.apply_orientation(orientation);
    Ok(prepare(image))
}

const BLURS: [u8; 5] = [0, 6, 12, 24, 40];

pub(crate) struct Picture {
    layers: Vec<Arc<RenderImage>>,
}

impl Picture {
    pub(crate) fn blend(&self, blur: u8, brightness: u8) -> [(Arc<RenderImage>, f32); 2] {
        let blur = blur.min(40);
        let upper = BLURS.iter().position(|level| *level >= blur).unwrap_or(4);
        let lower = upper.saturating_sub(1);
        let span = f32::from(BLURS[upper] - BLURS[lower]).max(1.);
        let fraction = f32::from(blur - BLURS[lower]) / span;
        let brightness = f32::from(brightness.min(100)) / 100.;
        let top = brightness * fraction;
        let bottom = if top < 1. {
            brightness * (1. - fraction) / (1. - top)
        } else {
            0.
        };
        [
            (self.layers[lower].clone(), bottom),
            (self.layers[upper].clone(), top),
        ]
    }

    pub(crate) fn drop_images(&self, window: &mut gpui::Window) {
        for layer in &self.layers {
            let _ = window.drop_image(layer.clone());
        }
    }
}

fn prepare(image: DynamicImage) -> Arc<Picture> {
    // GPUI 0.2.2 has no backdrop filter. Cache blur levels before upload.
    let full = image.resize(2560, 2160, FilterType::Triangle);
    let small = full.resize(1280, 1080, FilterType::Triangle);
    let layers = BLURS
        .into_iter()
        .map(|blur| {
            let source = if blur == 0 || blur == 12 {
                &full
            } else {
                &small
            };
            let scale = source.width() as f32 / full.width() as f32;
            let mut pixels = source.fast_blur(f32::from(blur) * scale).to_rgba8();
            for pixel in pixels.pixels_mut() {
                let [r, g, b, alpha] = pixel.0.map(f32::from);
                let gray = r * 0.2126 + g * 0.7152 + b * 0.0722;
                for channel in &mut pixel.0[..3] {
                    let saturated = gray + (f32::from(*channel) - gray) * 1.17;
                    let contrasted = (saturated - 127.5) * 0.89 + 127.5;
                    *channel = (contrasted * alpha / 255.).clamp(0., 255.) as u8;
                }
                pixel.0[3] = 255;
                pixel.0.swap(0, 2);
            }
            Arc::new(RenderImage::new(vec![Frame::new(pixels)]))
        })
        .collect();
    Arc::new(Picture { layers })
}
