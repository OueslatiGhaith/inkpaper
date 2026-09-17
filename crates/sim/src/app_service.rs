use inkpaper_app::{BrowseRequest, InkPaperApp, ReaderRequest, ReadingHistoryRequest};
use inkpaper_ui::prelude::*;

use crate::{SimulatorReaderService, fake_fs::simulator_listing, runtime::SimulatorRuntime};

pub(super) fn service_app_requests(
    runtime: &mut SimulatorRuntime<'_>,
    app: Entity<InkPaperApp>,
    reader_service: &mut SimulatorReaderService,
) {
    loop {
        let (browse_request, reader_request, history_request) = runtime
            .update(app, |app, _| {
                (
                    app.take_browse_request(),
                    app.take_reader_request(),
                    app.take_reading_history_request(),
                )
            })
            .expect("app root must be available");

        if browse_request.is_none() && reader_request.is_none() && history_request.is_none() {
            return;
        }

        if let Some(request) = browse_request {
            service_browse_request(runtime, app, request);
        }

        // reader first intentionally: if leaving the reader also requested a history
        // refresh, persist the newest position before taking the snapshot.
        if let Some(request) = reader_request {
            service_reader_request(runtime, app, reader_service, request);
        }

        if let Some(request) = history_request {
            service_reading_history_request(runtime, app, reader_service, request);
        }
    }
}

fn service_browse_request(
    runtime: &mut SimulatorRuntime<'_>,
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
    runtime: &mut SimulatorRuntime<'_>,
    app: Entity<InkPaperApp>,
    reader_service: &mut SimulatorReaderService,
    request: ReaderRequest,
) {
    match request {
        ReaderRequest::OpenEpub { path, font_size } => {
            match reader_service.open_document(path.clone(), font_size) {
                Some(mut document) => {
                    document.register_images(runtime);

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
            }
        }

        ReaderRequest::LoadAdjacentChapter {
            path,
            from,
            direction,
        } => match reader_service.load_adjacent_chapter(&path, from, direction) {
            Some(mut chapter) => {
                chapter.register_images(runtime);

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

        ReaderRequest::RepaginateChapter {
            path,
            spine,
            font_size,
        } => match reader_service.repaginate_chapter(&path, spine, font_size) {
            Some(mut chapter) => {
                chapter.register_images(runtime);

                runtime
                    .update(app, move |app, cx| {
                        app.apply_reader_repagination(path, spine, font_size, chapter, cx);
                    })
                    .expect("app root must be available");
            }

            None => {
                runtime
                    .update(app, move |app, _| {
                        app.finish_reader_repagination_request(path, spine, font_size);
                    })
                    .expect("app root must be available");
            }
        },

        ReaderRequest::UpdateProgress(progress) => {
            reader_service.update_progress(progress);
        }
    }
}

fn service_reading_history_request(
    runtime: &mut SimulatorRuntime<'_>,
    app: Entity<InkPaperApp>,
    reader_service: &SimulatorReaderService,
    request: ReadingHistoryRequest,
) {
    match request {
        ReadingHistoryRequest::Load => {
            let entries = reader_service.reading_history();

            runtime
                .update(app, move |app, cx| {
                    app.apply_reading_history(entries, cx);
                })
                .expect("app root must be available");
        }
    }
}
