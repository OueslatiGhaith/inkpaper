use alloc::vec::Vec;

use inkpaper_epub::{ArchivePath, Chapter, Epub, EpubSource, ImageDimensions, Inline};

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

    fn contains(&self, path: &ArchivePath) -> bool {
        self.entries.iter().any(|entry| entry.path == *path)
    }

    fn record(&mut self, path: ArchivePath, dimensions: Option<ImageDimensions>) {
        if self.contains(&path) {
            return;
        }

        self.entries.push(ChapterImageMetric { path, dimensions });
    }
}

pub(super) async fn load_chapter_image_metrics<S>(
    epub: &mut Epub<S>,
    chapter: &Chapter,
) -> ChapterImageMetrics
where
    S: EpubSource,
{
    let mut metrics = ChapterImageMetrics::default();

    for block in chapter.blocks() {
        for inline in block.inlines() {
            let Inline::Image(image) = inline else {
                continue;
            };

            let path = image.path().clone();

            if metrics.contains(&path) {
                continue;
            }

            // images are optional content. A malformed or missing image must not
            // make the surrounding text chapter unreadable.
            let dimensions = epub.image_dimensions(&path).await.ok().flatten();

            metrics.record(path, dimensions);
        }
    }

    metrics
}
