use std::{io, path::Path};

use futures_lite::future;

use inkpaper_app::reader::{
    ReaderPageResources, ReaderSession, UiReaderMeasureError, UiReaderMeasurer,
};

use inkpaper_epub::{
    ArchivePath, Chapter, Epub, Error as EpubError, ImageDimensions, Inline, SpineIndex,
};

use inkpaper_reader::{
    BlockKind, Page, PageItem, Pagination, ReaderSettings, TextStyle as ReaderTextStyle, Viewport,
    paginate_chapter,
};

use inkpaper_ui::{FontId, FontRegistry, ImageSource, ShapedGlyph};

use crate::{host_epub::HostFileSource, host_image::HostImage};

const READER_FONT_SIZE: u16 = 18;
const READER_BLOCK_SPACING: u16 = 6;

#[derive(Debug)]
pub enum ReaderLoadError {
    Open(io::Error),
    Epub(EpubError<io::Error>),
    NoTextChapter,
    SpineIndexOverflow,
    MissingImageResource(ArchivePath),
    ImageDecode {
        path: ArchivePath,
        error: image::ImageError,
    },
    Measure(UiReaderMeasureError),
}

pub struct DecodedReaderImage {
    path: ArchivePath,
    image: HostImage,
}

impl DecodedReaderImage {
    pub fn path(&self) -> &ArchivePath {
        &self.path
    }

    pub const fn image(&self) -> &HostImage {
        &self.image
    }
}

struct SimulatorReaderImage {
    path: ArchivePath,
    dimensions: ImageDimensions,
    source: Option<ImageSource>,
}

struct SimulatorReaderResources {
    body_font: FontId,
    heading_font: FontId,
    images: Vec<SimulatorReaderImage>,
}

impl SimulatorReaderResources {
    fn new(body_font: FontId, heading_font: FontId) -> Self {
        Self {
            body_font,
            heading_font,
            images: Vec::new(),
        }
    }

    fn add_image(&mut self, path: ArchivePath, dimensions: ImageDimensions) {
        if self.images.iter().any(|image| image.path == path) {
            return;
        }

        self.images.push(SimulatorReaderImage {
            path,
            dimensions,
            source: None,
        });
    }

    fn image(&self, path: &ArchivePath) -> Option<&SimulatorReaderImage> {
        self.images.iter().find(|image| &image.path == path)
    }

    fn image_mut(&mut self, path: &ArchivePath) -> Option<&mut SimulatorReaderImage> {
        self.images.iter_mut().find(|image| &image.path == path)
    }
}

impl ReaderPageResources for SimulatorReaderResources {
    fn font_for(&self, style: ReaderTextStyle) -> FontId {
        match style.block_kind() {
            BlockKind::Heading(_) => self.heading_font,
            BlockKind::Paragraph | BlockKind::ListItem => self.body_font,
        }
    }

    fn image_dimensions(&self, image: &inkpaper_reader::ChapterImage) -> Option<ImageDimensions> {
        self.image(image.path()).map(|image| image.dimensions)
    }

    fn image_source(&self, image: &inkpaper_reader::ChapterImage) -> Option<ImageSource> {
        self.image(image.path()).and_then(|image| image.source)
    }
}

pub struct PreparedReader {
    pagination: Pagination<'static>,
    viewport: Viewport,
    resources: SimulatorReaderResources,
    spine: SpineIndex,
}

impl PreparedReader {
    pub fn page_count(&self) -> usize {
        self.pagination.len()
    }

    pub const fn spine(&self) -> SpineIndex {
        self.spine
    }

    pub fn set_image_source(&mut self, path: &ArchivePath, source: ImageSource) -> bool {
        let Some(image) = self.resources.image_mut(path) else {
            return false;
        };

        image.source = Some(source);

        true
    }

    pub fn into_app_session(self) -> ReaderSession {
        let viewport = self.viewport;
        let spine = self.spine;

        // runtime application entities must be 'static, while Pagination deliberately
        // borrows chapter content. The simulator opens one book for the lifetime of the
        // process, so leaking this prepared backing store is appropriate here.
        // the embedded platform will eventually own the corresponding long-lived reader
        // session explicitly.
        let prepared: &'static PreparedReader = Box::leak(Box::new(self));

        ReaderSession::new(&prepared.pagination, viewport, &prepared.resources, spine)
    }
}

pub fn load_reader_session<const FONTS: usize>(
    path: &Path,
    fonts: FontRegistry<'static, FONTS>,
    body_font: FontId,
    heading_font: FontId,
    viewport: Viewport,
) -> Result<(PreparedReader, Vec<DecodedReaderImage>), ReaderLoadError> {
    let source = HostFileSource::open(path).map_err(ReaderLoadError::Open)?;

    let mut epub = future::block_on(Epub::open(source)).map_err(ReaderLoadError::Epub)?;

    eprintln!(
        "epub: {} title={}",
        path.display(),
        epub.metadata().title().unwrap_or("<untitled>"),
    );

    let spine_len = epub.spine().items().len();
    let mut selected = None;

    for index in 0..spine_len {
        if !epub.spine().items()[index].linear() {
            continue;
        }

        let is_xhtml = epub
            .package()
            .spine_manifest_item(index)
            .is_some_and(|item| item.media_type() == "application/xhtml+xml");

        if !is_xhtml {
            continue;
        }

        let Some(chapter) =
            future::block_on(epub.load_spine_chapter(index)).map_err(ReaderLoadError::Epub)?
        else {
            continue;
        };

        if chapter.content_len().get() == 0 {
            continue;
        }

        selected = Some((index, chapter));

        break;
    }

    let (spine_index, chapter) = selected.ok_or(ReaderLoadError::NoTextChapter)?;

    let styles =
        future::block_on(epub.load_chapter_styles(&chapter)).map_err(ReaderLoadError::Epub)?;

    let scratch_len = shaping_scratch_len(&chapter);

    // pagination deliberately borrows chapter content to avoid cloning every text
    // and image reference. Runtime entities, meanwhile, are 'static.
    // this leak exists only in the host preview process. The embedded reader will
    // have a proper book/session owner with a matching lifetime.
    let mut resources = SimulatorReaderResources::new(body_font, heading_font);

    load_image_metadata(&mut epub, &chapter, &mut resources)?;

    // pagination borrows chapter strings and ChapterImages directly.
    // the simulator book remains alive for the whole process.
    let chapter: &'static Chapter = Box::leak(Box::new(chapter));

    let spine =
        SpineIndex::try_from_usize(spine_index).ok_or(ReaderLoadError::SpineIndexOverflow)?;

    let settings = ReaderSettings::new(READER_FONT_SIZE, READER_BLOCK_SPACING)
        .expect("reader simulator settings are statically valid");

    let mut scratch = vec![ShapedGlyph::EMPTY; scratch_len];

    let pagination = {
        let mut measurer = UiReaderMeasurer::new(fonts, &resources, &mut scratch);

        paginate_chapter(chapter, &styles, spine, viewport, settings, &mut measurer)
            .map_err(ReaderLoadError::Measure)?
    };

    let first_page = pagination
        .pages()
        .first()
        .expect("reader pagination always contains at least one page");

    let decoded_images = decode_page_images(&mut epub, first_page)?;

    eprintln!(
        "reader: spine={} pages={} shaping_scratch={} glyphs first_page_images={}",
        spine.get(),
        pagination.len(),
        scratch_len,
        decoded_images.len(),
    );

    Ok((
        PreparedReader {
            pagination,
            viewport,
            resources,
            spine,
        },
        decoded_images,
    ))
}

fn load_image_metadata(
    epub: &mut Epub<HostFileSource>,
    chapter: &Chapter,
    resources: &mut SimulatorReaderResources,
) -> Result<(), ReaderLoadError> {
    let mut paths = Vec::new();

    for image in chapter
        .blocks()
        .iter()
        .flat_map(|block| block.inlines())
        .filter_map(|inline| {
            let Inline::Image(image) = inline else {
                return None;
            };

            Some(image)
        })
    {
        let path = image.path();

        let supported = epub
            .package()
            .manifest_item_by_path(path)
            .is_some_and(|item| is_supported_reader_image(item.media_type()));

        if !supported || paths.iter().any(|candidate| candidate == path) {
            continue;
        }

        paths.push(path.clone());
    }

    for path in paths {
        let Some(dimensions) =
            future::block_on(epub.image_dimensions(&path)).map_err(ReaderLoadError::Epub)?
        else {
            continue;
        };

        resources.add_image(path, dimensions);
    }

    Ok(())
}

fn decode_page_images(
    epub: &mut Epub<HostFileSource>,
    page: &Page<'_>,
) -> Result<Vec<DecodedReaderImage>, ReaderLoadError> {
    let mut images = Vec::new();

    for item in page.items() {
        let PageItem::Image(fragment) = item else {
            continue;
        };

        let path = fragment.image().path();

        if images
            .iter()
            .any(|image: &DecodedReaderImage| image.path() == path)
        {
            continue;
        }

        let bytes = future::block_on(epub.read_resource(path))
            .map_err(ReaderLoadError::Epub)?
            .ok_or_else(|| ReaderLoadError::MissingImageResource(path.clone()))?;

        let image = HostImage::decode(&bytes).map_err(|error| ReaderLoadError::ImageDecode {
            path: path.clone(),
            error,
        })?;

        images.push(DecodedReaderImage {
            path: path.clone(),
            image,
        });
    }

    Ok(images)
}

fn is_supported_reader_image(media_type: &str) -> bool {
    matches!(media_type, "image/png" | "image/jpeg" | "image/jpg")
}

fn shaping_scratch_len(chapter: &Chapter) -> usize {
    chapter
        .blocks()
        .iter()
        .flat_map(|block| block.inlines())
        .filter_map(|inline| match inline {
            Inline::Text(run) => Some(run.text().len()),
            Inline::Image(_) | Inline::Break | Inline::Anchor(_) => None,
        })
        .max()
        .unwrap_or(1)
        .max(1)
}

#[cfg(test)]
mod tests {
    use inkpaper_reader::{FontStyle, FontWeight};

    use super::*;

    #[test]
    fn reader_resources_select_body_and_heading_fonts() {
        let body = FontId::new(0);
        let heading = FontId::new(1);
        let resources = SimulatorReaderResources::new(body, heading);
        let paragraph = ReaderTextStyle::new(
            18,
            BlockKind::Paragraph,
            FontWeight::Normal,
            FontStyle::Normal,
        );
        let heading_style = ReaderTextStyle::new(
            18,
            BlockKind::Heading(2),
            FontWeight::Bold,
            FontStyle::Normal,
        );

        assert_eq!(resources.font_for(paragraph), body);
        assert_eq!(resources.font_for(heading_style), heading);
    }

    #[test]
    fn reader_decodes_only_supported_image_formats() {
        assert!(is_supported_reader_image("image/png"));
        assert!(is_supported_reader_image("image/jpeg"));
        assert!(is_supported_reader_image("image/jpg"));
        assert!(!is_supported_reader_image("image/gif"));
        assert!(!is_supported_reader_image("image/svg+xml"));
    }
}
