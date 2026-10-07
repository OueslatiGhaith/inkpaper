//! The current book's cover on the home screen. The service finds the cover
//! in the book once, shrinks it to the home screen's cover box and caches the
//! thumbnail on the SD card, so later visits only read that small file.

use alloc::{string::String, vec::Vec};

use inkpaper_epub::{Epub, EpubSource, Error as EpubError};
use inkpaper_ui::ImageSource;
use minicbor::{Decode, Encode};

use crate::{
    reader::{GrayImage, decode_image},
    storage::{self, StorageError},
};

/// The home screen's cover box, inside its 1 px border.
pub(crate) const COVER_WIDTH: u32 = 148;
pub(crate) const COVER_HEIGHT: u32 = 224;

/// The cover the home screen shows, and the book whose cover it wants next.
#[derive(Debug, Default)]
pub(crate) struct CoverState {
    /// a book and its registered thumbnail, `None` when it has no cover
    shown: Option<(String, Option<ImageSource>)>,
    pending: Option<String>,
}

impl CoverState {
    /// Asks for the cover of the book at `path`. The service answers right
    /// away when it already has it registered.
    pub(crate) fn request(&mut self, path: &str) {
        self.pending = Some(String::from(path));
    }

    pub(crate) fn take_request(&mut self) -> Option<String> {
        self.pending.take()
    }

    pub(crate) fn apply(&mut self, path: String, source: Option<ImageSource>) -> bool {
        let shown = Some((path, source));

        if self.shown == shown {
            return false;
        }

        self.shown = shown;

        true
    }

    /// The thumbnail of the book at `path`, if it has one and it is loaded.
    pub(crate) fn source(&self, path: &str) -> Option<ImageSource> {
        match &self.shown {
            Some((shown, source)) if shown == path => *source,
            _ => None,
        }
    }
}

/// A book's cover thumbnail, or the knowledge that it has none to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BookCover {
    path: String,
    image: Option<GrayImage>,
}

impl BookCover {
    /// `image` is shrunk to fit the cover box.
    pub(crate) fn new(path: String, image: Option<&GrayImage>) -> Self {
        let image = image.and_then(|image| image.fit_within(COVER_WIDTH, COVER_HEIGHT));

        Self { path, image }
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn image(&self) -> Option<&GrayImage> {
        self.image.as_ref()
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>, StorageError> {
        let stored = match &self.image {
            Some(image) => StoredCover {
                path: self.path.clone(),
                width: image.width(),
                height: image.height(),
                pixels: image.pixels().to_vec(),
            },

            None => StoredCover {
                path: self.path.clone(),
                width: 0,
                height: 0,
                pixels: Vec::new(),
            },
        };

        storage::encode(&stored)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, StorageError> {
        let stored: StoredCover = storage::decode(bytes)?;

        let image = if stored.pixels.is_empty() {
            None
        } else {
            let width = usize::try_from(stored.width).map_err(|_| StorageError::Decode)?;
            let height = usize::try_from(stored.height).map_err(|_| StorageError::Decode)?;

            Some(GrayImage::new(width, height, stored.pixels).ok_or(StorageError::Decode)?)
        };

        Ok(Self {
            path: stored.path,
            image,
        })
    }
}

/// The cover `epub` declares, decoded. `Ok(None)` when it declares none or
/// the image can't be decoded; an error when the book couldn't be read.
pub(crate) async fn read_cover_image<S: EpubSource>(
    epub: &mut Epub<S>,
) -> Result<Option<GrayImage>, EpubError<S::Error>> {
    let Some(path) = epub.cover_image().await? else {
        return Ok(None);
    };

    let Some(bytes) = epub.read_resource(&path).await? else {
        return Ok(None);
    };

    Ok(decode_image(&bytes))
}

#[derive(Debug, Encode, Decode)]
struct StoredCover {
    #[n(0)]
    path: String,
    #[n(1)]
    width: u32,
    #[n(2)]
    height: u32,
    /// one gray byte per pixel, row by row; empty when the book has no cover
    #[cbor(n(3), with = "minicbor::bytes")]
    pixels: Vec<u8>,
}
