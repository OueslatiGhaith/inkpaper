use alloc::string::String;

use inkpaper_ui::{Entity, ResourceRuntimeApi, RuntimeApi};

use crate::{
    InkPaperApp, ReaderPreferencesRequest, ReaderRequest, ReaderSession,
    fonts::{CardFont, card_for, load_card_font},
    reader::{ReaderFont, TextSettings},
    typography::register_card_font,
};

use super::{AppPlatform, AppService, AppServiceError};

impl<P> AppService<P>
where
    P: AppPlatform,
{
    pub(super) async fn service_reader_request<'resource, R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
        request: ReaderRequest,
    ) -> Result<(), AppServiceError>
    where
        R: RuntimeApi + ResourceRuntimeApi<'resource>,
    {
        match request {
            ReaderRequest::OpenEpub { path, text } => {
                self.open_document(runtime, app, path, text).await?;
            }

            ReaderRequest::LoadAdjacentChapter {
                path,
                from,
                direction,
            } => {
                let chapter = match self.reader_session.as_mut() {
                    Some(session) if session.path() == path => session
                        .load_adjacent_chapter(from, direction)
                        .await
                        .ok()
                        .flatten(),

                    _ => None,
                };

                match chapter {
                    Some(mut chapter) => {
                        chapter.register_images(runtime);
                        self.cover_source = None;

                        runtime.update(app, move |app, cx| {
                            app.apply_reader_chapter(path, from, direction, chapter, cx)
                        })?;
                    }

                    None => {
                        runtime.update(app, move |app, _| {
                            app.finish_reader_chapter_request(path, from, direction)
                        })?;
                    }
                }
            }

            ReaderRequest::RepaginateChapter { path, spine, text } => {
                let chapter = match self.reader_session.as_mut() {
                    Some(session) if session.path() == path => {
                        let card = card_for(text.font(), self.card_font.as_ref());

                        session
                            .repaginate_chapter(spine, text, card)
                            .await
                            .ok()
                            .flatten()
                    }

                    _ => None,
                };

                match chapter {
                    Some(mut chapter) => {
                        chapter.register_images(runtime);
                        self.cover_source = None;

                        runtime.update(app, move |app, cx| {
                            app.apply_reader_repagination(path, spine, text, chapter, cx);
                        })?;
                    }

                    None => {
                        runtime.update(app, move |app, _| {
                            app.finish_reader_repagination_request(path, spine, text);
                        })?;
                    }
                }
            }

            ReaderRequest::JumpTo { path, target } => {
                let landing = match self.reader_session.as_mut() {
                    Some(session) if session.path() == path => {
                        session.load_jump_target(&target).await.ok().flatten()
                    }

                    _ => None,
                };

                match landing {
                    Some((mut chapter, page_index)) => {
                        chapter.register_images(runtime);
                        self.cover_source = None;

                        runtime.update(app, move |app, cx| {
                            app.apply_reader_jump(path, target, chapter, page_index, cx);
                        })?;
                    }

                    None => {
                        runtime.update(app, move |app, _| {
                            app.finish_reader_jump_request(path, target);
                        })?;
                    }
                }
            }

            ReaderRequest::LoadTableOfContents { path } => {
                let entries = match self.reader_session.as_mut() {
                    Some(session) if session.path() == path => {
                        session.load_table_of_contents().await.ok()
                    }

                    _ => None,
                };

                runtime.update(app, move |app, cx| match entries {
                    Some(entries) => app.apply_reader_table_of_contents(path, entries, cx),
                    None => app.apply_reader_table_of_contents_error(path, cx),
                })?;
            }

            ReaderRequest::UpdateProgress(progress) => {
                self.history.record(progress);
                self.history_dirty = true;
            }
        }

        Ok(())
    }

    async fn open_document<'resource, R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
        path: String,
        text: TextSettings,
    ) -> Result<(), AppServiceError>
    where
        R: RuntimeApi + ResourceRuntimeApi<'resource>,
    {
        let reuse = self
            .reader_session
            .as_ref()
            .is_some_and(|session| session.path() == path);

        if !reuse {
            self.reader_session = None;

            let source = match self.platform.open_random_access(&path).await {
                Ok(source) => source,

                Err(_) => {
                    runtime.update(app, move |app, cx| {
                        app.apply_reader_error(path, cx);
                    })?;

                    return Ok(());
                }
            };

            let session = match ReaderSession::open(path.clone(), source).await {
                Ok(session) => session,

                Err(_) => {
                    runtime.update(app, move |app, cx| {
                        app.apply_reader_error(path, cx);
                    })?;

                    return Ok(());
                }
            };

            self.reader_session = Some(session);
        }

        let document = {
            let Some(session) = self.reader_session.as_mut() else {
                return Ok(());
            };

            let card = card_for(text.font(), self.card_font.as_ref());

            // without its fonts the book can't be laid out, so it fails to open
            if session.set_text_settings(text, card).is_err() {
                None
            } else {
                let resume = self.history.resume_position(&path, session.identifier());

                session.load_document_at(resume).await.ok()
            }
        };

        match document {
            Some(mut document) => {
                document.register_images(runtime);
                self.cover_source = None;

                runtime.update(app, move |app, cx| {
                    app.apply_reader_document(document, cx);
                })?;
            }

            None => {
                runtime.update(app, move |app, cx| {
                    app.apply_reader_error(path, cx);
                })?;
            }
        }

        Ok(())
    }

    /// Loads the card font the app wants, or frees the loaded one when it
    /// wants the built-in font, and registers it for the screen. Returns
    /// whether the fonts changed.
    pub(super) async fn service_reader_font<'resource, R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
        font: ReaderFont,
    ) -> Result<bool, AppServiceError>
    where
        R: RuntimeApi + ResourceRuntimeApi<'resource>,
    {
        if self
            .failed_font
            .is_some_and(|failed| font != ReaderFont::Card(failed))
        {
            self.failed_font = None;
        }

        let loaded = self.card_font.as_ref().map(CardFont::family);

        let index = match font {
            ReaderFont::BuiltIn if loaded.is_none() => return Ok(false),
            ReaderFont::BuiltIn => None,

            ReaderFont::Card(index) if loaded == Some(index) => return Ok(false),
            ReaderFont::Card(index) if self.failed_font == Some(index) => return Ok(false),
            ReaderFont::Card(index) => Some(index),
        };

        // the old faces go before the new ones are read, so two families
        // aren't in memory at once
        if self.card_font.take().is_some() {
            let _ = register_card_font(runtime, None);
            runtime.update(app, |app, cx| app.apply_card_font(None, cx))?;
        }

        let Some(index) = index else {
            return Ok(true);
        };

        let card = match load_card_font(&mut self.platform, &self.font_families, index).await {
            Ok(card) if register_card_font(runtime, Some(&card)).is_ok() => card,

            _ => {
                let _ = register_card_font(runtime, None);
                self.failed_font = Some(index);

                return Ok(true);
            }
        };

        self.card_font = Some(card.clone());
        runtime.update(app, move |app, cx| app.apply_card_font(Some(card), cx))?;

        Ok(true)
    }

    pub(super) async fn service_reader_preferences_request<R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
        request: ReaderPreferencesRequest,
    ) -> Result<(), AppServiceError>
    where
        R: RuntimeApi,
    {
        match request {
            ReaderPreferencesRequest::Load => {
                let preferences = self.preferences;

                runtime.update(app, move |app, cx| {
                    app.apply_reader_preferences(preferences, cx);
                })?;
            }

            ReaderPreferencesRequest::Update(preferences) => {
                self.preferences = preferences;
                self.preferences_dirty = true;

                self.persist_preferences().await;
            }
        }

        Ok(())
    }
}
