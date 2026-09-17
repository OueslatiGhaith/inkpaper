use alloc::{boxed::Box, vec::Vec};
use core::mem;

use inkpaper_epub::{ArchivePath, Chapter, Epub, EpubSource, ImageDimensions, Inline};
use inkpaper_ui::{Color, ImageResource, ImageSource, Luminance, ResourceRuntimeApi, Size, px};
use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
use zune_jpeg::JpegDecoder;
use zune_png::PngDecoder;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChapterImageMetric {
    path: ArchivePath,
    dimensions: Option<ImageDimensions>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct ChapterImageMetrics {
    entries: Vec<ChapterImageMetric>,
}

impl ChapterImageMetrics {
    pub(super) fn dimensions(&self, path: &ArchivePath) -> Option<ImageDimensions> {
        self.entries
            .iter()
            .find(|entry| entry.path == *path)
            .and_then(|entry| entry.dimensions)
    }

    fn record(&mut self, path: ArchivePath, dimensions: Option<ImageDimensions>) {
        if self.entries.iter().any(|entry| entry.path == path) {
            return;
        }

        self.entries.push(ChapterImageMetric { path, dimensions });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GrayImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl GrayImage {
    fn new(width: usize, height: usize, pixels: Vec<u8>) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }

        let expected = width.checked_mul(height)?;

        if pixels.len() != expected {
            return None;
        }

        Some(Self {
            width: u32::try_from(width).ok()?,
            height: u32::try_from(height).ok()?,
            pixels,
        })
    }

    const fn dimensions(&self) -> ImageDimensions {
        ImageDimensions::new(self.width, self.height)
    }
}

impl ImageResource for GrayImage {
    fn size(&self) -> Size {
        Size::new(
            px(i32::try_from(self.width).unwrap_or(i32::MAX)),
            px(i32::try_from(self.height).unwrap_or(i32::MAX)),
        )
    }

    fn pixel(&self, x: u32, y: u32) -> Option<Color> {
        self.luminance(x, y).map(Color::gray)
    }

    fn luminance(&self, x: u32, y: u32) -> Option<Luminance> {
        if x >= self.width || y >= self.height {
            return None;
        }

        let width = usize::try_from(self.width).ok()?;
        let x = usize::try_from(x).ok()?;
        let y = usize::try_from(y).ok()?;

        let index = y.checked_mul(width)?.checked_add(x)?;

        self.pixels.get(index).copied().map(Luminance::new)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChapterImageState {
    Decoded(GrayImage),
    Registered(ImageSource),
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChapterImageEntry {
    path: ArchivePath,
    state: ChapterImageState,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct ChapterImages {
    entries: Vec<ChapterImageEntry>,
}

impl ChapterImages {
    fn contains(&self, path: &ArchivePath) -> bool {
        self.entries.iter().any(|entry| entry.path == *path)
    }

    fn record(&mut self, path: ArchivePath, image: Option<GrayImage>) {
        if self.contains(&path) {
            return;
        }

        let state = match image {
            Some(image) => ChapterImageState::Decoded(image),
            None => ChapterImageState::Unavailable,
        };

        self.entries.push(ChapterImageEntry { path, state });
    }

    pub(super) fn source(&self, path: &ArchivePath) -> Option<ImageSource> {
        self.entries
            .iter()
            .find(|entry| entry.path == *path)
            .and_then(|entry| match entry.state {
                ChapterImageState::Registered(source) => Some(source),
                ChapterImageState::Decoded(_) | ChapterImageState::Unavailable => None,
            })
    }

    pub(super) fn register<'resource>(&mut self, runtime: &mut impl ResourceRuntimeApi<'resource>) {
        if self
            .entries
            .iter()
            .any(|entry| matches!(entry.state, ChapterImageState::Registered(_)))
        {
            return;
        }

        runtime.clear_owned_images();

        for entry in &mut self.entries {
            let state = mem::replace(&mut entry.state, ChapterImageState::Unavailable);

            let ChapterImageState::Decoded(image) = state else {
                continue;
            };

            if let Ok(source) = runtime.register_owned_image(Box::new(image)) {
                entry.state = ChapterImageState::Registered(source);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct LoadedChapterImages {
    metrics: ChapterImageMetrics,
    images: ChapterImages,
}

impl LoadedChapterImages {
    pub(super) fn into_parts(self) -> (ChapterImageMetrics, ChapterImages) {
        (self.metrics, self.images)
    }
}

pub(super) async fn load_chapter_images<S>(
    epub: &mut Epub<S>,
    chapter: &Chapter,
) -> LoadedChapterImages
where
    S: EpubSource,
{
    let mut loaded = LoadedChapterImages::default();

    for block in chapter.blocks() {
        for inline in block.inlines() {
            let Inline::Image(image) = inline else {
                continue;
            };

            let path = image.path().clone();

            if loaded.images.contains(&path) {
                continue;
            }

            let decoded = match epub.read_resource(&path).await {
                Ok(Some(bytes)) => decode_image(&bytes),
                Ok(None) | Err(_) => None,
            };

            let dimensions = match decoded.as_ref() {
                Some(image) => Some(image.dimensions()),
                None => epub.image_dimensions(&path).await.ok().flatten(),
            };

            loaded.metrics.record(path.clone(), dimensions);
            loaded.images.record(path, decoded);
        }
    }

    loaded
}

fn decode_image(bytes: &[u8]) -> Option<GrayImage> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return decode_png(bytes);
    }

    if bytes.starts_with(&[0xff, 0xd8]) {
        return decode_jpeg(bytes);
    }

    None
}

fn decode_jpeg(bytes: &[u8]) -> Option<GrayImage> {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::Luma);

    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);

    let pixels = decoder.decode().ok()?;
    let (width, height) = decoder.dimensions()?;

    GrayImage::new(width, height, pixels)
}

fn decode_png(bytes: &[u8]) -> Option<GrayImage> {
    let options = DecoderOptions::default()
        .png_set_strip_to_8bit(true)
        .png_set_decode_animated(false);

    let mut decoder = PngDecoder::new_with_options(ZCursor::new(bytes), options);

    let raw = decoder.decode_raw().ok()?;

    let (width, height) = decoder.dimensions()?;
    let colorspace = decoder.colorspace()?;

    let pixels = png_to_grayscale(raw, colorspace, width, height)?;

    GrayImage::new(width, height, pixels)
}

fn png_to_grayscale(
    raw: Vec<u8>,
    colorspace: ColorSpace,
    width: usize,
    height: usize,
) -> Option<Vec<u8>> {
    let pixels = width.checked_mul(height)?;

    match colorspace {
        ColorSpace::Luma => {
            if raw.len() == pixels {
                Some(raw)
            } else {
                None
            }
        }

        ColorSpace::LumaA => {
            if raw.len() != pixels.checked_mul(2)? {
                return None;
            }

            let mut output = Vec::with_capacity(pixels);

            for pixel in raw.chunks_exact(2) {
                output.push(composite_on_white(pixel[0], pixel[1]));
            }

            Some(output)
        }

        ColorSpace::RGB => rgb_to_grayscale(&raw, pixels, false),
        ColorSpace::RGBA => rgba_to_grayscale(&raw, pixels, false, false),
        ColorSpace::BGR => rgb_to_grayscale(&raw, pixels, true),
        ColorSpace::BGRA => rgba_to_grayscale(&raw, pixels, true, false),
        ColorSpace::ARGB => rgba_to_grayscale(&raw, pixels, false, true),

        _ => None,
    }
}

fn rgb_to_grayscale(raw: &[u8], pixels: usize, bgr: bool) -> Option<Vec<u8>> {
    if raw.len() != pixels.checked_mul(3)? {
        return None;
    }

    let mut output = Vec::with_capacity(pixels);

    for pixel in raw.chunks_exact(3) {
        let (r, g, b) = if bgr {
            (pixel[2], pixel[1], pixel[0])
        } else {
            (pixel[0], pixel[1], pixel[2])
        };

        output.push(rgb_luminance(r, g, b));
    }

    Some(output)
}

fn rgba_to_grayscale(raw: &[u8], pixels: usize, bgr: bool, alpha_first: bool) -> Option<Vec<u8>> {
    if raw.len() != pixels.checked_mul(4)? {
        return None;
    }

    let mut output = Vec::with_capacity(pixels);

    for pixel in raw.chunks_exact(4) {
        let (r, g, b, alpha) = if alpha_first {
            (pixel[1], pixel[2], pixel[3], pixel[0])
        } else if bgr {
            (pixel[2], pixel[1], pixel[0], pixel[3])
        } else {
            (pixel[0], pixel[1], pixel[2], pixel[3])
        };

        let luminance = rgb_luminance(r, g, b);

        output.push(composite_on_white(luminance, alpha));
    }

    Some(output)
}

fn rgb_luminance(r: u8, g: u8, b: u8) -> u8 {
    let value = 77 * u32::from(r) + 150 * u32::from(g) + 29 * u32::from(b) + 128;

    (value / 256) as u8
}

fn composite_on_white(luminance: u8, alpha: u8) -> u8 {
    let alpha = u32::from(alpha);

    let value = u32::from(luminance) * alpha + 255 * (255 - alpha) + 127;

    (value / 255) as u8
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn gray_image_exposes_native_luminance() {
        let image = GrayImage::new(2, 2, vec![0, 64, 128, 255]).unwrap();

        assert_eq!(image.luminance(0, 0), Some(Luminance::new(0)));
        assert_eq!(image.luminance(1, 0), Some(Luminance::new(64)));
        assert_eq!(image.luminance(0, 1), Some(Luminance::new(128)));
        assert_eq!(image.luminance(1, 1), Some(Luminance::new(255)));
        assert_eq!(image.luminance(2, 0), None);
    }

    #[test]
    fn converts_rgb_pixels_to_grayscale() {
        let pixels =
            png_to_grayscale(vec![255, 0, 0, 0, 255, 0, 0, 0, 255], ColorSpace::RGB, 3, 1).unwrap();

        assert_eq!(pixels, vec![77, 149, 29]);
    }

    #[test]
    fn transparent_png_pixels_are_composited_over_white() {
        let pixels =
            png_to_grayscale(vec![0, 0, 0, 255, 128, 128], ColorSpace::LumaA, 3, 1).unwrap();

        assert_eq!(pixels[0], 255);
        assert_eq!(pixels[1], 0);
        assert!(pixels[2] > 128);
    }

    #[test]
    fn rejects_malformed_pixel_buffer_lengths() {
        assert_eq!(png_to_grayscale(vec![0, 0], ColorSpace::RGB, 1, 1), None,);
    }
}
