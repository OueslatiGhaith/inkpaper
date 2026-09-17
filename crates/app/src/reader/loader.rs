use alloc::string::String;

use inkpaper_epub::{ContentOffset, Epub, EpubSource, Error as EpubError, SpineIndex};
use inkpaper_reader::{ReadingPosition, paginate_chapter};
use inkpaper_ui::{FontRegistryError, ShapeError};

use crate::reader::{images::load_chapter_images, progress::BookProgressMap};

use super::{
    document::{ReaderChapter, ReaderDocument},
    measurer::ReaderMeasurer,
    reader_settings, reader_viewport,
    state::ReaderChapterDirection,
};

#[derive(Debug)]
pub enum ReaderLoadError<E> {
    Epub(EpubError<E>),
    FontRegistry(FontRegistryError),
    Shape(ShapeError),
    SpineIndexOverflow,
    NoReadableChapter,
}

pub struct ReaderSession<S>
where
    S: EpubSource,
{
    path: String,
    epub: Epub<S>,
    progress: BookProgressMap,
}

impl<S> ReaderSession<S>
where
    S: EpubSource,
{
    pub async fn open(path: String, source: S) -> Result<Self, ReaderLoadError<S::Error>> {
        let mut epub = Epub::open(source).await.map_err(ReaderLoadError::Epub)?;
        let progress = load_book_progress_map(&mut epub).await?;

        Ok(Self {
            path,
            epub,
            progress,
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn identifier(&self) -> Option<&str> {
        self.epub.metadata().identifier()
    }

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
        )
        .await
    }

    pub async fn load_adjacent_chapter(
        &mut self,
        from: SpineIndex,
        direction: ReaderChapterDirection,
    ) -> Result<Option<ReaderChapter>, ReaderLoadError<S::Error>> {
        load_adjacent_reader_chapter_from_epub(&mut self.epub, from, direction).await
    }
}

pub async fn load_reader_document<S>(
    path: String,
    source: S,
) -> Result<ReaderDocument, ReaderLoadError<S::Error>>
where
    S: EpubSource,
{
    let mut session = ReaderSession::open(path, source).await?;

    session.load_document().await
}

pub async fn load_adjacent_reader_chapter<S>(
    source: S,
    from: SpineIndex,
    direction: ReaderChapterDirection,
) -> Result<Option<ReaderChapter>, ReaderLoadError<S::Error>>
where
    S: EpubSource,
{
    let mut epub = Epub::open(source).await.map_err(ReaderLoadError::Epub)?;

    load_adjacent_reader_chapter_from_epub(&mut epub, from, direction).await
}

async fn load_reader_document_from_epub<S>(
    path: String,
    epub: &mut Epub<S>,
    progress: BookProgressMap,
    resume: Option<ReadingPosition>,
) -> Result<ReaderDocument, ReaderLoadError<S::Error>>
where
    S: EpubSource,
{
    let identifier = epub.metadata().identifier().map(String::from);

    let title = epub.metadata().title().map(String::from);

    let creators = epub.metadata().creators().to_vec();

    let package_path = String::from(epub.package().path().as_str());

    let spine_len = epub.spine().items().len();

    if let Some(position) = resume
        && let Some(index) = position.location().spine().as_usize()
        && index < spine_len
        && let Some(chapter) = load_readable_chapter_at(epub, index).await?
    {
        let opening_page_index = chapter.page_at_position(position).unwrap_or(0);

        return Ok(ReaderDocument::new(
            path,
            identifier,
            title,
            creators,
            package_path,
            spine_len,
            progress,
            chapter,
            opening_page_index,
        ));
    }

    for index in 0..spine_len {
        if let Some(chapter) = load_readable_chapter_at(epub, index).await? {
            return Ok(ReaderDocument::new(
                path,
                identifier,
                title,
                creators,
                package_path,
                spine_len,
                progress,
                chapter,
                0,
            ));
        }
    }

    Err(ReaderLoadError::NoReadableChapter)
}

async fn load_adjacent_reader_chapter_from_epub<S>(
    epub: &mut Epub<S>,
    from: SpineIndex,
    direction: ReaderChapterDirection,
) -> Result<Option<ReaderChapter>, ReaderLoadError<S::Error>>
where
    S: EpubSource,
{
    let spine_len = epub.spine().items().len();

    let from = from.as_usize().ok_or(ReaderLoadError::SpineIndexOverflow)?;

    if from >= spine_len {
        return Ok(None);
    }

    match direction {
        ReaderChapterDirection::Next => {
            for index in from.saturating_add(1)..spine_len {
                if let Some(chapter) = load_readable_chapter_at(epub, index).await? {
                    return Ok(Some(chapter));
                }
            }
        }

        ReaderChapterDirection::Previous => {
            for index in (0..from).rev() {
                if let Some(chapter) = load_readable_chapter_at(epub, index).await? {
                    return Ok(Some(chapter));
                }
            }
        }
    }

    Ok(None)
}

async fn load_book_progress_map<S>(
    epub: &mut Epub<S>,
) -> Result<BookProgressMap, ReaderLoadError<S::Error>>
where
    S: EpubSource,
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
            .spine_manifest_item(index)
            .is_some_and(|item| item.media_type() == "application/xhtml+xml");

        if linear && xhtml {
            size.unwrap_or(0)
        } else {
            0
        }
    });

    Ok(BookProgressMap::from_weights(weights))
}

async fn load_readable_chapter_at<S>(
    epub: &mut Epub<S>,
    index: usize,
) -> Result<Option<ReaderChapter>, ReaderLoadError<S::Error>>
where
    S: EpubSource,
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
        .spine_manifest_item(index)
        .is_some_and(|item| item.media_type() == "application/xhtml+xml");

    if !is_xhtml {
        return Ok(None);
    }

    let Some(chapter) = epub
        .load_spine_chapter(index)
        .await
        .map_err(ReaderLoadError::Epub)?
    else {
        return Ok(None);
    };

    // keep image-only covers/front matter skipped until pixel rendering lands.
    // mixed text/image chapters can now be paginated with correct image bounds.
    if chapter.content_len() == ContentOffset::ZERO {
        return Ok(None);
    }

    let styles = epub
        .load_chapter_styles(&chapter)
        .await
        .map_err(ReaderLoadError::Epub)?;

    let loaded_images = load_chapter_images(epub, &chapter).await;
    let (image_metrics, chapter_images) = loaded_images.into_parts();

    let spine = SpineIndex::try_from_usize(index).ok_or(ReaderLoadError::SpineIndexOverflow)?;

    let chapter_path = String::from(chapter.path().as_str());

    let mut measurer = ReaderMeasurer::new(image_metrics).map_err(ReaderLoadError::FontRegistry)?;

    let pagination = paginate_chapter(
        &chapter,
        &styles,
        spine,
        reader_viewport(),
        reader_settings(),
        &mut measurer,
    )
    .map_err(ReaderLoadError::Shape)?;

    if pagination
        .pages()
        .iter()
        .all(|page| page.items().is_empty())
    {
        return Ok(None);
    }

    Ok(Some(ReaderChapter::with_images(
        chapter_path,
        spine,
        pagination.into_owned(),
        chapter_images,
    )))
}
