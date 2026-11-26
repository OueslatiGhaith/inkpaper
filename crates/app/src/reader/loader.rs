use alloc::{string::String, vec::Vec};

use inkpaper_epub::{
    ArchivePath, BookLocation, ChapterStyles, Epub, Error as EpubError, SpineIndex,
};
use inkpaper_io::ByteSource;
use inkpaper_reader::{ReadingPosition, index_chapter, render_page};
use inkpaper_trace::{async_span, span};
use inkpaper_ui::{FontRegistryError, ShapeError};

use crate::{
    cover::read_cover_image,
    fonts::CardFont,
    reader::{
        ChapterImages, GrayImage, TextSettings,
        images::{decode_page_images, page_image_paths, probe_chapter_image_metrics},
        progress::BookProgressMap,
        toc::{TocEntry, toc_entries},
    },
};

use super::{
    document::{ReaderChapter, ReaderDocument},
    measurer::{ReaderFonts, ReaderMeasurer},
    state::{JumpTarget, ReaderChapterDirection},
};

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) enum ReaderLoadError<E> {
    Epub(EpubError<E>),
    FontRegistry(FontRegistryError),
    Shape(ShapeError),
    SpineIndexOverflow,
    NoReadableChapter,
}

pub(crate) struct ReaderSession<S>
where
    S: ByteSource,
{
    path: String,
    epub: Epub<S>,
    progress: BookProgressMap,
    text: TextSettings,
    /// the faces `text` lays out with
    fonts: ReaderFonts,
}

impl<S> ReaderSession<S>
where
    S: ByteSource,
{
    #[inkpaper_trace::instrument(target = "reader.session", name = "open")]
    pub async fn open(path: String, source: S) -> Result<Self, ReaderLoadError<S::Error>> {
        let mut epub = {
            let _trace = span!(target: "reader.epub", "open");
            Epub::open(source).await.map_err(ReaderLoadError::Epub)?
        };
        let progress = load_book_progress_map(&mut epub).await?;
        let fonts = ReaderFonts::new(None).map_err(ReaderLoadError::FontRegistry)?;

        Ok(Self {
            path,
            epub,
            progress,
            text: TextSettings::default(),
            fonts,
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn identifier(&self) -> Option<&str> {
        self.epub.metadata().identifier()
    }

    /// The book's cover, through the open book: a platform may keep only one
    /// file open, so opening it again would close this session's.
    pub async fn cover_image(&mut self) -> Result<Option<GrayImage>, EpubError<S::Error>> {
        read_cover_image(&mut self.epub).await
    }

    /// Lays out with `text`, and `card` when it uses a card font that loaded.
    pub fn set_text_settings(
        &mut self,
        text: TextSettings,
        card: Option<&CardFont>,
    ) -> Result<(), FontRegistryError> {
        self.fonts = ReaderFonts::new(card)?;
        self.text = text;

        Ok(())
    }

    #[cfg(test)]
    pub async fn load_document(&mut self) -> Result<ReaderDocument, ReaderLoadError<S::Error>> {
        self.load_document_at(None).await
    }

    pub async fn load_document_at(
        &mut self,
        position: Option<ReadingPosition>,
    ) -> Result<ReaderDocument, ReaderLoadError<S::Error>> {
        load_reader_document_from_epub(
            self.path.clone(),
            &mut self.epub,
            self.progress.clone(),
            position,
            self.text,
            &self.fonts,
        )
        .await
    }

    pub async fn load_adjacent_chapter(
        &mut self,
        from: SpineIndex,
        direction: ReaderChapterDirection,
    ) -> Result<Option<ReaderChapter>, ReaderLoadError<S::Error>> {
        load_adjacent_reader_chapter_from_epub(
            &mut self.epub,
            from,
            direction,
            self.text,
            &self.fonts,
        )
        .await
    }

    #[inkpaper_trace::instrument(target = "reader.document", name = "toc")]
    pub async fn load_table_of_contents(
        &mut self,
    ) -> Result<Vec<TocEntry>, ReaderLoadError<S::Error>> {
        let Some(navigation) = self
            .epub
            .load_navigation()
            .await
            .map_err(ReaderLoadError::Epub)?
        else {
            return Ok(Vec::new());
        };

        Ok(toc_entries(&navigation, self.epub.package()))
    }

    /// Decodes only the image paths needed for the currently visible page.
    pub async fn load_page_images(&mut self, paths: &[ArchivePath]) -> ChapterImages {
        decode_page_images(&mut self.epub, paths).await
    }

    /// Loads the chapter at `spine` and finds the page holding `anchor`, or the
    /// chapter's first page when there is no anchor or it cannot be found.
    #[inkpaper_trace::instrument(
        target = "reader.document",
        name = "jump",
        fields(spine = spine.get())
    )]
    pub async fn load_chapter_at(
        &mut self,
        spine: SpineIndex,
        anchor: Option<&str>,
    ) -> Result<Option<(ReaderChapter, usize)>, ReaderLoadError<S::Error>> {
        let index = spine
            .as_usize()
            .ok_or(ReaderLoadError::SpineIndexOverflow)?;

        load_chapter_with_landing(
            &mut self.epub,
            index,
            self.text,
            &self.fonts,
            ChapterLanding::Anchor(anchor),
        )
        .await
    }

    /// The chapter `target` lands in and the index of its page there. `None`
    /// when the target is not a readable chapter of the book.
    pub async fn load_jump_target(
        &mut self,
        target: &JumpTarget,
    ) -> Result<Option<(ReaderChapter, usize)>, ReaderLoadError<S::Error>> {
        match target {
            JumpTarget::Chapter { spine, anchor } => {
                self.load_chapter_at(*spine, anchor.as_deref()).await
            }

            JumpTarget::Link { resource, anchor } => {
                let Some(index) = self.epub.package().spine_index_for_path(resource) else {
                    return Ok(None);
                };

                let spine =
                    SpineIndex::try_from_usize(index).ok_or(ReaderLoadError::SpineIndexOverflow)?;

                self.load_chapter_at(spine, anchor.as_deref()).await
            }

            JumpTarget::Position(position) => {
                let index = position
                    .location()
                    .spine()
                    .as_usize()
                    .ok_or(ReaderLoadError::SpineIndexOverflow)?;

                load_chapter_with_landing(
                    &mut self.epub,
                    index,
                    self.text,
                    &self.fonts,
                    ChapterLanding::Position(*position),
                )
                .await
            }
        }
    }

    #[inkpaper_trace::instrument(
        target = "reader.document",
        name = "repaginate",
        fields(
            spine = spine.get(),
            font_size = text.font_size(),
        )
    )]
    pub async fn repaginate_chapter(
        &mut self,
        spine: SpineIndex,
        text: TextSettings,
        card: Option<&CardFont>,
    ) -> Result<Option<ReaderChapter>, ReaderLoadError<S::Error>> {
        let index = spine
            .as_usize()
            .ok_or(ReaderLoadError::SpineIndexOverflow)?;

        let fonts = ReaderFonts::new(card).map_err(ReaderLoadError::FontRegistry)?;
        let chapter = load_chapter_with_landing(
            &mut self.epub,
            index,
            text,
            &fonts,
            ChapterLanding::FirstPage,
        )
        .await?
        .map(|(chapter, _)| chapter);

        if chapter.is_some() {
            self.text = text;
            self.fonts = fonts;
        }

        Ok(chapter)
    }
}

#[cfg(test)]
pub(crate) async fn load_reader_document<S>(
    path: String,
    source: S,
) -> Result<ReaderDocument, ReaderLoadError<S::Error>>
where
    S: ByteSource,
{
    let mut session = ReaderSession::open(path, source).await?;

    session.load_document().await
}

#[inkpaper_trace::instrument(
    target = "reader.document",
    name = "load",
    fields(resume = resume.is_some(), spine_len = epub.spine().items().len())
)]
async fn load_reader_document_from_epub<S>(
    path: String,
    epub: &mut Epub<S>,
    progress: BookProgressMap,
    resume: Option<ReadingPosition>,
    text: TextSettings,
    fonts: &ReaderFonts,
) -> Result<ReaderDocument, ReaderLoadError<S::Error>>
where
    S: ByteSource,
{
    let identifier = epub.metadata().identifier().map(String::from);

    let title = epub.metadata().title().map(String::from);

    let creators = epub.metadata().creators().to_vec();

    let package_path = String::from(epub.package().path().as_str());

    let spine_len = epub.spine().items().len();

    if let Some(position) = resume
        && let Some(index) = position.location().spine().as_usize()
        && index < spine_len
        && let Some((chapter, opening_page_index)) = load_chapter_with_landing(
            epub,
            index,
            text,
            fonts,
            ChapterLanding::Position(position),
        )
        .await?
    {
        return Ok(ReaderDocument::new(
            path,
            identifier,
            title,
            creators,
            package_path,
            progress,
            chapter,
            opening_page_index,
        ));
    }

    for index in 0..spine_len {
        if let Some((chapter, opening_page_index)) =
            load_chapter_with_landing(epub, index, text, fonts, ChapterLanding::FirstPage).await?
        {
            return Ok(ReaderDocument::new(
                path,
                identifier,
                title,
                creators,
                package_path,
                progress,
                chapter,
                opening_page_index,
            ));
        }
    }

    Err(ReaderLoadError::NoReadableChapter)
}

#[inkpaper_trace::instrument(
    target = "reader.document",
    name = "load_adjacent",
    fields(
        from = from.get(),
        next = direction == ReaderChapterDirection::Next,
    )
)]
async fn load_adjacent_reader_chapter_from_epub<S>(
    epub: &mut Epub<S>,
    from: SpineIndex,
    direction: ReaderChapterDirection,
    text: TextSettings,
    fonts: &ReaderFonts,
) -> Result<Option<ReaderChapter>, ReaderLoadError<S::Error>>
where
    S: ByteSource,
{
    let spine_len = epub.spine().items().len();

    let from = from.as_usize().ok_or(ReaderLoadError::SpineIndexOverflow)?;

    if from >= spine_len {
        return Ok(None);
    }

    match direction {
        ReaderChapterDirection::Next => {
            for index in from.saturating_add(1)..spine_len {
                if let Some((chapter, _)) =
                    load_chapter_with_landing(epub, index, text, fonts, ChapterLanding::FirstPage)
                        .await?
                {
                    return Ok(Some(chapter));
                }
            }
        }

        ReaderChapterDirection::Previous => {
            for index in (0..from).rev() {
                if let Some((chapter, _)) =
                    load_chapter_with_landing(epub, index, text, fonts, ChapterLanding::LastPage)
                        .await?
                {
                    return Ok(Some(chapter));
                }
            }
        }
    }

    Ok(None)
}

#[inkpaper_trace::instrument(
    target = "reader.session",
    name = "progress_map",
    fields(spine_len = epub.spine().items().len())
)]
async fn load_book_progress_map<S>(
    epub: &mut Epub<S>,
) -> Result<BookProgressMap, ReaderLoadError<S::Error>>
where
    S: ByteSource,
{
    let sizes = epub
        .spine_resource_sizes()
        .await
        .map_err(ReaderLoadError::Epub)?;

    let weights = sizes.into_iter().enumerate().map(|(index, size)| {
        let linear = epub
            .spine()
            .items()
            .get(index)
            .is_some_and(|item| item.linear());

        let xhtml = epub
            .package()
            .spine_content_item(index)
            .is_some_and(|item| item.media_type() == "application/xhtml+xml");

        if linear && xhtml {
            size.unwrap_or(0)
        } else {
            0
        }
    });

    Ok(BookProgressMap::from_weights(weights))
}

#[derive(Debug, Clone, Copy)]
enum ChapterLanding<'a> {
    FirstPage,
    LastPage,
    Anchor(Option<&'a str>),
    Position(ReadingPosition),
}

/// Loads a readable spine entry into lightweight page checkpoints, materializing
/// and decoding images only for its landing page.
#[inkpaper_trace::instrument(
    target = "reader.chapter",
    name = "load",
    fields(spine = index, font_size = text.font_size())
)]
async fn load_chapter_with_landing<S>(
    epub: &mut Epub<S>,
    index: usize,
    text: TextSettings,
    fonts: &ReaderFonts,
    landing: ChapterLanding<'_>,
) -> Result<Option<(ReaderChapter, usize)>, ReaderLoadError<S::Error>>
where
    S: ByteSource,
{
    let linear = epub
        .spine()
        .items()
        .get(index)
        .is_some_and(|item| item.linear());

    if !linear {
        return Ok(None);
    }

    let is_xhtml = epub
        .package()
        .spine_content_item(index)
        .is_some_and(|item| item.media_type() == "application/xhtml+xml");

    if !is_xhtml {
        return Ok(None);
    }

    let chapter = {
        let _trace = async_span!(
            target: "reader.chapter",
            "content",
            spine = index,
        );

        epub.load_spine_chapter(index)
            .await
            .map_err(ReaderLoadError::Epub)?
    };

    let Some((chapter, style_context)) = chapter else {
        return Ok(None);
    };

    let anchor_offset = match landing {
        ChapterLanding::Anchor(Some(anchor)) => chapter.anchor_offset(anchor),
        _ => None,
    };

    let styles = {
        let _trace = async_span!(
            target: "reader.chapter",
            "styles",
            spine = index,
        );

        if text.embedded_style() {
            epub.load_chapter_styles(style_context)
                .await
                .map_err(ReaderLoadError::Epub)?
        } else {
            ChapterStyles::defaults(style_context)
        }
    };

    let image_metrics = {
        let _trace = async_span!(
            target: "reader.chapter",
            "image_headers",
            spine = index,
        );

        probe_chapter_image_metrics(epub, &chapter).await
    };

    let spine = SpineIndex::try_from_usize(index).ok_or(ReaderLoadError::SpineIndexOverflow)?;
    let chapter_path = String::from(chapter.path().as_str());
    let viewport = text.viewport();
    let reader_settings = text.reader_settings(epub.metadata().language());

    let mut measurer = ReaderMeasurer::new(fonts, image_metrics.clone());

    let page_index_table = {
        let _trace = inkpaper_trace::span!(
            target: "reader.pagination",
            "index",
            spine = index,
            font_size = text.font_size(),
        );

        index_chapter(
            &chapter,
            &styles,
            spine,
            viewport,
            reader_settings,
            &mut measurer,
        )
        .map_err(ReaderLoadError::Shape)?
    };

    if !page_index_table.has_visible_content() {
        return Ok(None);
    }

    let landing_page_index = match landing {
        ChapterLanding::FirstPage => 0,
        ChapterLanding::LastPage => page_index_table.len().saturating_sub(1),
        ChapterLanding::Anchor(_) => anchor_offset
            .and_then(|offset| {
                let position = ReadingPosition::new(BookLocation::new(spine, offset), 0);
                page_index_table.page_at_position(position)
            })
            .unwrap_or(0),
        ChapterLanding::Position(position) => page_index_table.page_at_position(position).unwrap_or(0),
    };

    let landing_page = {
        let _trace = inkpaper_trace::span!(
            target: "reader.pagination",
            "render_page",
            spine = index,
            page = landing_page_index,
        );

        render_page(
            &chapter,
            &styles,
            &page_index_table,
            landing_page_index,
            viewport,
            reader_settings,
            &mut measurer,
        )
        .map_err(ReaderLoadError::Shape)?
        .ok_or(ReaderLoadError::NoReadableChapter)?
        .into_owned()
    };

    let page_images = {
        let paths = page_image_paths(&landing_page);
        let _trace = async_span!(
            target: "reader.chapter",
            "page_images",
            spine = index,
            page = landing_page_index,
            count = paths.len(),
        );

        decode_page_images(epub, &paths).await
    };

    Ok(Some((
        ReaderChapter::from_index(
            chapter_path,
            spine,
            chapter,
            styles,
            image_metrics,
            viewport,
            reader_settings,
            page_index_table,
            landing_page_index,
            landing_page,
            page_images,
        ),
        landing_page_index,
    )))
}
