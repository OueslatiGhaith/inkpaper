use std::{io, path::Path};

use futures_lite::future;

use inkpaper_app::{
    BookSummary,
    reader::{
        ChapterDirection, ChapterRequest, ReaderPageResources, ReaderSession, UiReaderMeasureError,
        UiReaderMeasurer,
    },
};

use inkpaper_epub::{
    ArchivePath, Chapter, Epub, Error as EpubError, ImageDimensions, Inline, SpineIndex,
};

use inkpaper_reader::{
    BlockKind, Page, PageItem, Pagination, ReaderSettings, ReadingPosition,
    TextStyle as ReaderTextStyle, Viewport, paginate_chapter,
};

use inkpaper_ui::{FontId, FontRegistry, ImageId, ImageResource, ImageSource, ShapedGlyph};

use crate::{host_epub::HostFileSource, host_image::HostImage};

#[cfg(test)]
mod tests;

const READER_FONT_SIZE: u16 = 18;
const READER_BLOCK_SPACING: u16 = 6;

#[derive(Debug)]
pub enum ReaderLoadError {
    Open(io::Error),
    Epub(EpubError<io::Error>),
    NoTextChapter,
    SpineIndexOverflow,
    InvalidSpine,
    InvalidPosition,
    TooManyImages {
        count: usize,
        capacity: usize,
    },
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

    pub fn into_image(self) -> HostImage {
        self.image
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

enum EntryPage {
    First,
    Last,
    Position(ReadingPosition),
}

pub struct PreparedReader {
    pagination: Pagination<'static>,
    resume_position: Option<ReadingPosition>,
    viewport: Viewport,
    resources: SimulatorReaderResources,
    images: Vec<DecodedReaderImage>,
}

impl PreparedReader {
    pub fn into_app_session(
        mut self,
        slots: &[ImageId],
    ) -> Result<(ReaderSession, Vec<DecodedReaderImage>), ReaderLoadError> {
        if self.images.len() > slots.len() {
            return Err(ReaderLoadError::TooManyImages {
                count: self.images.len(),
                capacity: slots.len(),
            });
        }

        for (decoded, id) in self.images.iter().zip(slots) {
            let source = ImageSource::new(*id, decoded.image().size());
            self.resources
                .image_mut(decoded.path())
                .expect("decoded image must have pagination metadata")
                .source = Some(source);
        }

        let resources: Box<dyn ReaderPageResources> = Box::new(self.resources);
        let session = match self.resume_position {
            Some(position) => {
                ReaderSession::at_position(self.pagination, self.viewport, resources, position)
                    .ok_or(ReaderLoadError::InvalidPosition)?
            }
            None => ReaderSession::new(self.pagination, self.viewport, resources)
                .expect("pagination always contains a page"),
        };

        Ok((session, self.images))
    }
}

pub struct HostReader<const FONTS: usize> {
    epub: Epub<HostFileSource>,
    fonts: FontRegistry<'static, FONTS>,
    body_font: FontId,
    heading_font: FontId,
    viewport: Viewport,
}

impl<const FONTS: usize> HostReader<FONTS> {
    pub fn open(
        path: &Path,
        fonts: FontRegistry<'static, FONTS>,
        body_font: FontId,
        heading_font: FontId,
        viewport: Viewport,
    ) -> Result<Self, ReaderLoadError> {
        let source = HostFileSource::open(path).map_err(ReaderLoadError::Open)?;
        let epub = future::block_on(Epub::open(source)).map_err(ReaderLoadError::Epub)?;
        Ok(Self {
            epub,
            fonts,
            body_font,
            heading_font,
            viewport,
        })
    }

    pub fn load_first(&mut self) -> Result<PreparedReader, ReaderLoadError> {
        self.load_from(0, ChapterDirection::Next)?
            .ok_or(ReaderLoadError::NoTextChapter)
    }

    pub fn load_adjacent(
        &mut self,
        request: ChapterRequest,
    ) -> Result<Option<PreparedReader>, ReaderLoadError> {
        let from = request
            .from
            .as_usize()
            .ok_or(ReaderLoadError::InvalidSpine)?;

        if from >= self.epub.spine().items().len() {
            return Err(ReaderLoadError::InvalidSpine);
        }

        let index = match request.direction {
            ChapterDirection::Previous => from.checked_sub(1),
            ChapterDirection::Next => from.checked_add(1),
        };

        match index {
            Some(index) => self.load_from(index, request.direction),
            None => Ok(None),
        }
    }

    fn load_from(
        &mut self,
        mut index: usize,
        direction: ChapterDirection,
    ) -> Result<Option<PreparedReader>, ReaderLoadError> {
        while index < self.epub.spine().items().len() {
            let readable = self.epub.spine().items()[index].linear()
                && self
                    .epub
                    .package()
                    .spine_manifest_item(index)
                    .is_some_and(|item| item.media_type() == "application/xhtml+xml");

            if readable
                && let Some(chapter) = future::block_on(self.epub.load_spine_chapter(index))
                    .map_err(ReaderLoadError::Epub)?
            {
                let spine =
                    SpineIndex::try_from_usize(index).ok_or(ReaderLoadError::SpineIndexOverflow)?;
                let entry = match direction {
                    ChapterDirection::Previous => EntryPage::Last,
                    ChapterDirection::Next => EntryPage::First,
                };

                if let Some(prepared) = self.prepare(chapter, spine, entry)? {
                    return Ok(Some(prepared));
                }
            }

            let next = match direction {
                ChapterDirection::Previous => index.checked_sub(1),
                ChapterDirection::Next => index.checked_add(1),
            };

            let Some(next) = next else { break };
            index = next;
        }

        Ok(None)
    }

    fn prepare(
        &mut self,
        chapter: Chapter,
        spine: SpineIndex,
        entry: EntryPage,
    ) -> Result<Option<PreparedReader>, ReaderLoadError> {
        let styles = future::block_on(self.epub.load_chapter_styles(&chapter))
            .map_err(ReaderLoadError::Epub)?;

        let mut resources = SimulatorReaderResources::new(self.body_font, self.heading_font);
        load_image_metadata(&mut self.epub, &chapter, &mut resources)?;

        let settings = ReaderSettings::new(READER_FONT_SIZE, READER_BLOCK_SPACING).unwrap();

        let mut scratch = vec![ShapedGlyph::EMPTY; shaping_scratch_len(&chapter)];
        let mut measurer = UiReaderMeasurer::new(self.fonts, &resources, &mut scratch);
        let pagination = paginate_chapter(
            &chapter,
            &styles,
            spine,
            self.viewport,
            settings,
            &mut measurer,
        )
        .map_err(ReaderLoadError::Measure)?;

        if pagination
            .pages()
            .iter()
            .all(|page| page.items().is_empty())
        {
            return Ok(None);
        }

        let (page_index, resume_position) = match entry {
            EntryPage::First => (0, None),
            EntryPage::Last => (pagination.len() - 1, None),
            EntryPage::Position(position) => (
                pagination
                    .page_at_position(position)
                    .ok_or(ReaderLoadError::InvalidPosition)?,
                Some(position),
            ),
        };
        let entry_page = &pagination.pages()[page_index];

        let images = decode_page_images(&mut self.epub, entry_page)?;

        Ok(Some(PreparedReader {
            pagination: pagination.into_owned(),
            resume_position,
            viewport: self.viewport,
            resources,
            images,
        }))
    }

    pub fn load_page(
        &mut self,
        page: &Page<'_>,
        slots: &[ImageId],
    ) -> Result<(Box<dyn ReaderPageResources>, Vec<DecodedReaderImage>), ReaderLoadError> {
        let mut paths = Vec::new();

        for item in page.items() {
            let PageItem::Image(fragment) = item else {
                continue;
            };

            let path = fragment.image().path();

            if !paths.contains(&path) {
                paths.push(path);
            }
        }

        if paths.len() > slots.len() {
            return Err(ReaderLoadError::TooManyImages {
                count: paths.len(),
                capacity: slots.len(),
            });
        }

        let images = decode_page_images(&mut self.epub, page)?;
        // this fresh map contains only images backed by the incoming slot contents.
        let mut resources = SimulatorReaderResources::new(self.body_font, self.heading_font);

        for (decoded, id) in images.iter().zip(slots) {
            let size = decoded.image().size();

            resources.images.push(SimulatorReaderImage {
                path: decoded.path().clone(),
                dimensions: ImageDimensions::new(size.width.get() as u32, size.height.get() as u32),
                source: Some(ImageSource::new(*id, size)),
            });
        }

        Ok((Box::new(resources), images))
    }

    pub fn load_position(
        &mut self,
        position: ReadingPosition,
    ) -> Result<PreparedReader, ReaderLoadError> {
        let spine = position.location().spine();
        let index = spine.as_usize().ok_or(ReaderLoadError::InvalidPosition)?;

        let readable = self
            .epub
            .spine()
            .items()
            .get(index)
            .is_some_and(|item| item.linear())
            && self
                .epub
                .package()
                .spine_manifest_item(index)
                .is_some_and(|item| item.media_type() == "application/xhtml+xml");

        if !readable {
            return Err(ReaderLoadError::InvalidPosition);
        }

        let chapter = future::block_on(self.epub.load_spine_chapter(index))
            .map_err(ReaderLoadError::Epub)?
            .ok_or(ReaderLoadError::InvalidPosition)?;

        self.prepare(chapter, spine, EntryPage::Position(position))?
            .ok_or(ReaderLoadError::InvalidPosition)
    }

    pub fn book_summary(&self) -> BookSummary {
        let metadata = self.epub.metadata();
        let author = metadata
            .creators()
            .iter()
            .map(|creator| creator.trim())
            .find(|creator| !creator.is_empty());

        BookSummary::from_metadata(metadata.title(), author)
    }
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
