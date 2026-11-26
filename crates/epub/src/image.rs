use inkpaper_io::ByteSource;

use crate::{Error, archive::EntryStream};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageDimensions {
    width: u32,
    height: u32,
}

impl ImageDimensions {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageFormat {
    Png,
    Jpeg,
    Gif,
}

impl ImageFormat {
    pub(crate) fn from_media_type(media_type: &str) -> Option<Self> {
        match media_type {
            "image/png" => Some(Self::Png),
            "image/jpeg" | "image/jpg" => Some(Self::Jpeg),
            "image/gif" => Some(Self::Gif),
            _ => None,
        }
    }
}

pub(crate) async fn read_dimensions<S: ByteSource>(
    format: ImageFormat,
    stream: &mut EntryStream<'_, S>,
) -> Result<Option<ImageDimensions>, Error<S::Error>> {
    match format {
        ImageFormat::Png => {
            let mut header = [0u8; 24];
            if !stream.read_exact(&mut header).await? {
                return Ok(None);
            }
            Ok(png_dimensions(&header))
        }
        ImageFormat::Gif => {
            let mut header = [0u8; 10];
            if !stream.read_exact(&mut header).await? {
                return Ok(None);
            }
            Ok(gif_dimensions(&header))
        }
        ImageFormat::Jpeg => read_jpeg_dimensions(stream).await,
    }
}

async fn read_jpeg_dimensions<S: ByteSource>(
    stream: &mut EntryStream<'_, S>,
) -> Result<Option<ImageDimensions>, Error<S::Error>> {
    let mut soi = [0u8; 2];
    if !stream.read_exact(&mut soi).await? || soi != [0xff, 0xd8] {
        return Ok(None);
    }

    loop {
        let mut byte = match stream.read_u8().await? {
            Some(byte) => byte,
            None => return Ok(None),
        };

        while byte != 0xff {
            byte = match stream.read_u8().await? {
                Some(byte) => byte,
                None => return Ok(None),
            };
        }

        let marker = loop {
            match stream.read_u8().await? {
                Some(0xff) => continue,
                Some(marker) => break marker,
                None => return Ok(None),
            }
        };

        if marker == 0x00 {
            continue;
        }

        if marker == 0xd9 || marker == 0xda {
            return Ok(None);
        }
        if marker == 0xd8 || marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }

        let mut len_bytes = [0u8; 2];
        if !stream.read_exact(&mut len_bytes).await? {
            return Ok(None);
        }

        let segment_len = usize::from(be_u16(&len_bytes));
        if segment_len < 2 {
            return Ok(None);
        }

        let payload_len = segment_len - 2;

        if is_start_of_frame(marker) {
            if payload_len < 5 {
                return Ok(None);
            }

            let mut sof = [0u8; 5];
            if !stream.read_exact(&mut sof).await? {
                return Ok(None);
            }

            let height = u32::from(be_u16(&sof[1..3]));
            let width = u32::from(be_u16(&sof[3..5]));

            return Ok(dimensions_if_nonzero(width, height));
        }

        if !stream.skip(payload_len).await? {
            return Ok(None);
        }
    }
}

#[cfg(test)]
fn dimensions(media_type: &str, bytes: &[u8]) -> Option<ImageDimensions> {
    match ImageFormat::from_media_type(media_type)? {
        ImageFormat::Png => png_dimensions(bytes),
        ImageFormat::Jpeg => jpeg_dimensions(bytes),
        ImageFormat::Gif => gif_dimensions(bytes),
    }
}

fn png_dimensions(bytes: &[u8]) -> Option<ImageDimensions> {
    const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

    if bytes.len() < 24 || &bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }

    let width = be_u32(&bytes[16..20]);
    let height = be_u32(&bytes[20..24]);

    dimensions_if_nonzero(width, height)
}

fn gif_dimensions(bytes: &[u8]) -> Option<ImageDimensions> {
    if bytes.len() < 10 || !matches!(&bytes[..6], b"GIF87a" | b"GIF89a") {
        return None;
    }

    let width = u32::from(le_u16(&bytes[6..8]));
    let height = u32::from(le_u16(&bytes[8..10]));

    dimensions_if_nonzero(width, height)
}

#[cfg(test)]
fn jpeg_dimensions(bytes: &[u8]) -> Option<ImageDimensions> {
    if bytes.len() < 4 || bytes[0] != 0xff || bytes[1] != 0xd8 {
        return None;
    }

    let mut offset = 2usize;

    while offset < bytes.len() {
        while offset < bytes.len() && bytes[offset] != 0xff {
            offset += 1;
        }
        while offset < bytes.len() && bytes[offset] == 0xff {
            offset += 1;
        }

        if offset >= bytes.len() {
            return None;
        }

        let marker = bytes[offset];

        offset += 1;

        if marker == 0x00 {
            continue;
        }

        if marker == 0xd9 || marker == 0xda {
            return None;
        }
        if marker == 0xd8 || marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }

        if offset.checked_add(2)? > bytes.len() {
            return None;
        }

        let segment_len = usize::from(be_u16(&bytes[offset..offset + 2]));

        if segment_len < 2 {
            return None;
        }

        let segment_end = offset.checked_add(segment_len)?;
        if segment_end > bytes.len() {
            return None;
        }

        if is_start_of_frame(marker) {
            if segment_len < 7 {
                return None;
            }

            let height = u32::from(be_u16(&bytes[offset + 3..offset + 5]));
            let width = u32::from(be_u16(&bytes[offset + 5..offset + 7]));

            return dimensions_if_nonzero(width, height);
        }

        offset = segment_end;
    }

    None
}

fn is_start_of_frame(marker: u8) -> bool {
    matches!(
        marker,
        0xc0 | 0xc1 | 0xc2 | 0xc3 | 0xc5 | 0xc6 | 0xc7 | 0xc9 | 0xca | 0xcb | 0xcd | 0xce | 0xcf
    )
}

fn dimensions_if_nonzero(width: u32, height: u32) -> Option<ImageDimensions> {
    if width == 0 || height == 0 {
        return None;
    }

    Some(ImageDimensions::new(width, height))
}

fn be_u16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}

fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn le_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_png_dimensions() {
        let mut bytes = [0u8; 24];

        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        bytes[12..16].copy_from_slice(b"IHDR");
        bytes[16..20].copy_from_slice(&640u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&480u32.to_be_bytes());

        assert_eq!(
            dimensions("image/png", &bytes),
            Some(ImageDimensions::new(640, 480)),
        );
    }

    #[test]
    fn reads_gif_dimensions() {
        let bytes = [b'G', b'I', b'F', b'8', b'9', b'a', 0x40, 0x01, 0xf0, 0x00];

        assert_eq!(
            dimensions("image/gif", &bytes),
            Some(ImageDimensions::new(320, 240)),
        );
    }

    #[test]
    fn reads_jpeg_dimensions() {
        let bytes = [
            0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x20, 0x00, 0x40, 0x03, 0x01, 0x11,
            0x00, 0x02, 0x11, 0x00, 0x03, 0x11, 0x00,
        ];

        assert_eq!(
            dimensions("image/jpeg", &bytes),
            Some(ImageDimensions::new(64, 32)),
        );
    }

    #[test]
    fn unsupported_image_format_has_no_dimensions() {
        assert_eq!(dimensions("image/svg+xml", b"<svg/>"), None);
    }
}
