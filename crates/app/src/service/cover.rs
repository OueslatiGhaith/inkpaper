use alloc::{boxed::Box, string::String};

use inkpaper_epub::Epub;
use inkpaper_ui::{Entity, ResourceRuntimeApi, RuntimeApi};

use crate::{BookCover, InkPaperApp, cover::read_cover_image, reader::GrayImage};

use super::{AppPlatform, AppService, AppServiceError};

/// the current book's cover thumbnail
const COVER_STATE: &str = "cover.dat";
const MAX_COVER_BYTES: usize = 48 * 1024;

impl<P> AppService<P>
where
    P: AppPlatform,
{
    /// Shows the cover of the book at `path`: the one already registered, the
    /// cached thumbnail, or one made from the book and cached.
    pub(super) async fn service_cover_request<'resource, R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
        path: String,
    ) -> Result<(), AppServiceError>
    where
        R: RuntimeApi + ResourceRuntimeApi<'resource>,
    {
        if self.cover.as_ref().is_none_or(|cover| cover.path() != path) {
            self.cover_source = None;
            self.cover = self.load_cover(&path).await;
        }

        let image = self.cover.as_ref().and_then(BookCover::image);

        if self.cover_source.is_none()
            && let Some(image) = image
        {
            self.cover_source = runtime.register_owned_image(Box::new(image.clone())).ok();
        }

        let source = self.cover_source;

        runtime.update(app, move |app, cx| app.apply_cover(path, source, cx))?;

        Ok(())
    }

    /// The cover of the book at `path`, from the cache or made from the book.
    /// `None` when the book couldn't be read, so it is tried again next time.
    async fn load_cover(&mut self, path: &str) -> Option<BookCover> {
        if let Some(bytes) = self.read_state(COVER_STATE, MAX_COVER_BYTES).await
            && let Ok(cover) = BookCover::decode(&bytes)
            && cover.path() == path
        {
            return Some(cover);
        }

        let image = self.read_cover_image(path).await?;
        let cover = BookCover::new(String::from(path), image.as_ref());

        self.write_state(COVER_STATE, cover.encode()).await;

        Some(cover)
    }

    async fn read_cover_image(&mut self, path: &str) -> Option<Option<GrayImage>> {
        // read through the reader's book when it is this one, rather than
        // opening it again
        if let Some(session) = self.reader_session.as_mut()
            && session.path() == path
        {
            return session.cover_image().await.ok();
        }

        let source = self.platform.open_random_access(path).await.ok()?;
        let mut epub = Epub::open(source).await.ok()?;

        read_cover_image(&mut epub).await.ok()
    }
}
