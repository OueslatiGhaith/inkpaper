use std::{io, path::Path};

use futures_lite::future;

use inkpaper_app::reader::{
    ReaderPageResources, ReaderPageView, UiReaderMeasureError, UiReaderMeasurer,
};

use inkpaper_epub::{Chapter, Epub, Error as EpubError, Inline, SpineIndex};

use inkpaper_reader::{
    BlockKind, Pagination, ReaderSettings, TextStyle as ReaderTextStyle, Viewport, paginate_chapter,
};

use inkpaper_ui::{Context, FontId, FontRegistry, IntoElement, Render, ShapedGlyph};

use crate::host_epub::HostFileSource;

const READER_FONT_SIZE: u16 = 18;
const READER_BLOCK_SPACING: u16 = 6;

#[derive(Debug)]
pub enum ReaderPreviewError {
    Open(io::Error),
    Epub(EpubError<io::Error>),
    NoTextChapter,
    SpineIndexOverflow,
    Measure(UiReaderMeasureError),
}

#[derive(Debug, Clone, Copy)]
struct SimulatorReaderResources {
    body_font: FontId,
    heading_font: FontId,
}

impl SimulatorReaderResources {
    const fn new(body_font: FontId, heading_font: FontId) -> Self {
        Self {
            body_font,
            heading_font,
        }
    }
}

impl ReaderPageResources for SimulatorReaderResources {
    fn font_for(&self, style: ReaderTextStyle) -> FontId {
        match style.block_kind() {
            BlockKind::Heading(_) => self.heading_font,
            BlockKind::Paragraph | BlockKind::ListItem => self.body_font,
        }
    }
}

pub struct ReaderPreview {
    pagination: Pagination<'static>,
    viewport: Viewport,
    resources: SimulatorReaderResources,
    spine: SpineIndex,
}

impl ReaderPreview {
    pub fn page_count(&self) -> usize {
        self.pagination.len()
    }

    pub const fn spine(&self) -> SpineIndex {
        self.spine
    }
}

impl Render for ReaderPreview {
    fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let page = self
            .pagination
            .pages()
            .first()
            .expect("reader pagination always contains at least one page");

        ReaderPageView::new(page, self.viewport, &self.resources)
    }
}

pub fn load_reader_preview<const FONTS: usize>(
    path: &Path,
    fonts: FontRegistry<'static, FONTS>,
    body_font: FontId,
    heading_font: FontId,
    viewport: Viewport,
) -> Result<ReaderPreview, ReaderPreviewError> {
    let source = HostFileSource::open(path).map_err(ReaderPreviewError::Open)?;

    let mut epub = future::block_on(Epub::open(source)).map_err(ReaderPreviewError::Epub)?;

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
            future::block_on(epub.load_spine_chapter(index)).map_err(ReaderPreviewError::Epub)?
        else {
            continue;
        };

        if chapter.content_len().get() == 0 {
            continue;
        }

        selected = Some((index, chapter));

        break;
    }

    let (spine_index, chapter) = selected.ok_or(ReaderPreviewError::NoTextChapter)?;

    let styles =
        future::block_on(epub.load_chapter_styles(&chapter)).map_err(ReaderPreviewError::Epub)?;

    let scratch_len = shaping_scratch_len(&chapter);

    // pagination deliberately borrows chapter content to avoid cloning every text
    // and image reference. Runtime entities, meanwhile, are 'static.
    // this leak exists only in the host preview process. The embedded reader will
    // have a proper book/session owner with a matching lifetime.
    let chapter: &'static Chapter = Box::leak(Box::new(chapter));

    let spine =
        SpineIndex::try_from_usize(spine_index).ok_or(ReaderPreviewError::SpineIndexOverflow)?;

    let resources = SimulatorReaderResources::new(body_font, heading_font);

    let settings = ReaderSettings::new(READER_FONT_SIZE, READER_BLOCK_SPACING)
        .expect("reader preview settings are statically valid");

    let mut scratch = vec![ShapedGlyph::EMPTY; scratch_len];

    let pagination = {
        let mut measurer = UiReaderMeasurer::new(fonts, &resources, &mut scratch);

        paginate_chapter(chapter, &styles, spine, viewport, settings, &mut measurer)
            .map_err(ReaderPreviewError::Measure)?
    };

    eprintln!(
        "reader preview: spine={} pages={} shaping_scratch={} glyphs",
        spine.get(),
        pagination.len(),
        scratch_len,
    );

    Ok(ReaderPreview {
        pagination,
        viewport,
        resources,
        spine,
    })
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
}
