use inkpaper_app::{BrowseRequest, InkPaperApp, ReaderRequest, RecentBooksRequest};
use inkpaper_ui::prelude::*;

use crate::{SimulatorReaderService, fake_fs::simulator_listing};

pub(super) fn service_app_requests(
    runtime: &impl RuntimeApi,
    app: Entity<InkPaperApp>,
    reader_service: &mut SimulatorReaderService,
) {
    loop {
        let (browse_request, reader_request, recent_books_request) = runtime
            .update(app, |app, _| {
                (
                    app.take_browse_request(),
                    app.take_reader_request(),
                    app.take_recent_books_request(),
                )
            })
            .expect("app root must be available");

        if browse_request.is_none() && reader_request.is_none() && recent_books_request.is_none() {
            return;
        }

        if let Some(request) = browse_request {
            service_browse_request(runtime, app, request);
        }

        if let Some(request) = reader_request {
            service_reader_request(runtime, app, reader_service, request);
        }

        if let Some(request) = recent_books_request {
            service_recent_books_request(runtime, app, reader_service, request);
        }
    }
}

fn service_browse_request(
    runtime: &impl RuntimeApi,
    app: Entity<InkPaperApp>,
    request: BrowseRequest,
) {
    match request {
        BrowseRequest::ListDirectory(path) => match simulator_listing(&path) {
            Some(listing) => {
                runtime
                    .update(app, move |app, cx| {
                        app.apply_browse_listing(listing, cx);
                    })
                    .expect("app root must be available");
            }

            None => {
                runtime
                    .update(app, |app, cx| {
                        app.apply_browse_error(cx);
                    })
                    .expect("app root must be available");
            }
        },
    }
}

fn service_reader_request(
    runtime: &impl RuntimeApi,
    app: Entity<InkPaperApp>,
    reader_service: &mut SimulatorReaderService,
    request: ReaderRequest,
) {
    match request {
        ReaderRequest::OpenEpub(path) => match reader_service.open_document(path.clone()) {
            Some(document) => {
                runtime
                    .update(app, move |app, cx| {
                        app.apply_reader_document(document, cx);
                    })
                    .expect("app root must be available");
            }

            None => {
                runtime
                    .update(app, move |app, cx| {
                        app.apply_reader_error(path, cx);
                    })
                    .expect("app root must be available");
            }
        },

        ReaderRequest::LoadAdjacentChapter {
            path,
            from,
            direction,
        } => match reader_service.load_adjacent_chapter(&path, from, direction) {
            Some(chapter) => {
                runtime
                    .update(app, move |app, cx| {
                        app.apply_reader_chapter(path, from, direction, chapter, cx);
                    })
                    .expect("app root must be available");
            }

            None => {
                runtime
                    .update(app, move |app, _| {
                        app.finish_reader_chapter_request(path, from, direction);
                    })
                    .expect("app root must be available");
            }
        },

        ReaderRequest::UpdateProgress(progress) => {
            reader_service.update_progress(progress);
        }
    }
}

fn service_recent_books_request(
    runtime: &impl RuntimeApi,
    app: Entity<InkPaperApp>,
    reader_service: &SimulatorReaderService,
    request: RecentBooksRequest,
) {
    match request {
        RecentBooksRequest::Load => {
            let entries = reader_service.recent_books();

            runtime
                .update(app, move |app, cx| {
                    app.apply_recent_books(entries, cx);
                })
                .expect("app root must be available");
        }
    }
}
