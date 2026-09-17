use defmt::warn;
use inkpaper_app::{
    BrowseEntry, BrowseListing, BrowseRequest, InkPaperApp, ReaderPreferencesRequest,
    ReaderRequest, ReadingHistoryRequest,
};
use inkpaper_ui::prelude::*;

use crate::firmware::{presenter::UiRuntime, storage};

pub(super) async fn service_app_requests(runtime: &mut UiRuntime, app: Entity<InkPaperApp>) {
    loop {
        let preferences_request =
            match runtime.update(app, |app, _| app.take_reader_preferences_request()) {
                Ok(request) => request,
                Err(_) => return warn!("failed to read app requests"),
            };

        if let Some(request) = preferences_request {
            service_reader_preferences_request(runtime, app, request).await;
            continue;
        }

        let requests = match runtime.update(app, |app, _| {
            (
                app.take_browse_request(),
                app.take_reader_request(),
                app.take_reading_history_request(),
            )
        }) {
            Ok(requests) => requests,
            Err(_) => return warn!("failed to read app requests"),
        };

        let (browse_request, reader_request, history_request) = requests;

        if browse_request.is_none() && reader_request.is_none() && history_request.is_none() {
            return;
        }

        if let Some(request) = browse_request {
            service_browse_request(runtime, app, request).await;
        }

        // keep this before history snapshots so a page-position update queued by
        // the same UI action is visible in the returned snapshot.
        if let Some(request) = reader_request {
            service_reader_request(runtime, app, request).await;
        }

        if let Some(request) = history_request {
            service_reading_history_request(runtime, app, request).await;
        }
    }
}

async fn service_reader_preferences_request(
    runtime: &mut UiRuntime,
    app: Entity<InkPaperApp>,
    request: ReaderPreferencesRequest,
) {
    match request {
        ReaderPreferencesRequest::Load => {
            let preferences = storage::reader_preferences_and_wait().await;

            if runtime
                .update(app, move |app, cx| {
                    app.apply_reader_preferences(preferences, cx);
                })
                .is_err()
            {
                warn!("failed to apply reader preferences");
            }
        }

        ReaderPreferencesRequest::Update(preferences) => {
            storage::update_reader_preferences(preferences).await;
        }
    }
}

async fn service_browse_request(
    runtime: &mut UiRuntime,
    app: Entity<InkPaperApp>,
    request: BrowseRequest,
) {
    match request {
        BrowseRequest::ListDirectory(path) => match storage::list_directory_and_wait(&path).await {
            Some(entries) => {
                let entries = entries
                    .into_iter()
                    .map(|entry| {
                        let (name, is_directory) = entry.into_parts();

                        if is_directory {
                            BrowseEntry::directory(name)
                        } else {
                            BrowseEntry::file(name)
                        }
                    })
                    .collect();

                let listing = BrowseListing::new(path, entries);

                if runtime
                    .update(app, move |app, cx| {
                        app.apply_browse_listing(listing, cx);
                    })
                    .is_err()
                {
                    warn!("failed to apply browse listing");
                }
            }

            None => {
                if runtime
                    .update(app, |app, cx| {
                        app.apply_browse_error(cx);
                    })
                    .is_err()
                {
                    warn!("failed to apply browse error");
                }
            }
        },
    }
}

async fn service_reader_request(
    runtime: &mut UiRuntime,
    app: Entity<InkPaperApp>,
    request: ReaderRequest,
) {
    match request {
        ReaderRequest::OpenEpub { path, font_size } => {
            match storage::load_epub_document_and_wait(&path, font_size).await {
                Some(mut document) => {
                    document.register_images(runtime);

                    if runtime
                        .update(app, move |app, cx| {
                            app.apply_reader_document(document, cx);
                        })
                        .is_err()
                    {
                        warn!("failed to apply reader document");
                    }
                }

                None => {
                    if runtime
                        .update(app, move |app, cx| {
                            app.apply_reader_error(path, cx);
                        })
                        .is_err()
                    {
                        warn!("failed to apply reader error");
                    }
                }
            }
        }

        ReaderRequest::LoadAdjacentChapter {
            path,
            from,
            direction,
        } => match storage::load_epub_chapter_and_wait(&path, from, direction).await {
            Some(mut chapter) => {
                chapter.register_images(runtime);

                if runtime
                    .update(app, move |app, cx| {
                        app.apply_reader_chapter(path, from, direction, chapter, cx);
                    })
                    .is_err()
                {
                    warn!("failed to apply reader chapter");
                }
            }

            None => {
                if runtime
                    .update(app, move |app, _| {
                        app.finish_reader_chapter_request(path, from, direction);
                    })
                    .is_err()
                {
                    warn!("failed to finish reader chapter request");
                }
            }
        },

        ReaderRequest::RepaginateChapter {
            path,
            spine,
            font_size,
        } => match storage::repaginate_epub_chapter_and_wait(&path, spine, font_size).await {
            Some(mut chapter) => {
                chapter.register_images(runtime);

                if runtime
                    .update(app, move |app, cx| {
                        app.apply_reader_repagination(path, spine, font_size, chapter, cx);
                    })
                    .is_err()
                {
                    warn!("failed to apply reader repagination");
                }
            }

            None => {
                if runtime
                    .update(app, move |app, _| {
                        app.finish_reader_repagination_request(path, spine, font_size);
                    })
                    .is_err()
                {
                    warn!("failed to finish reader repagination");
                }
            }
        },

        ReaderRequest::UpdateProgress(progress) => {
            storage::update_reading_progress(progress).await;
        }
    }
}

async fn service_reading_history_request(
    runtime: &mut UiRuntime,
    app: Entity<InkPaperApp>,
    request: ReadingHistoryRequest,
) {
    match request {
        ReadingHistoryRequest::Load => match storage::reading_history_and_wait().await {
            Some(entries) => {
                if runtime
                    .update(app, move |app, cx| {
                        app.apply_reading_history(entries, cx);
                    })
                    .is_err()
                {
                    warn!("failed to apply reading history");
                }
            }

            None => {
                if runtime
                    .update(app, |app, cx| {
                        app.apply_reading_history_error(cx);
                    })
                    .is_err()
                {
                    warn!("failed to apply reading-history error");
                }
            }
        },
    }
}
