use alloc::{format, string::String};
use futures_lite::future;
use inkpaper_epub::{SliceSource, SpineIndex};

use crate::{
    ReaderChapter, ReaderChapterDirection, ReaderRequest, ReaderSession, load_reader_document,
    reader::ReaderState,
};

#[test]
fn real_epub_load_reaches_a_paginated_text_page() {
    let source = SliceSource::new(include_bytes!("../../../../fixtures/book-boundaries.epub"));

    let document = future::block_on(load_reader_document(
        String::from("/Fixtures/book-boundaries.epub"),
        source,
    ))
    .unwrap();

    assert_eq!(document.path(), "/Fixtures/book-boundaries.epub");

    assert!(document.page_count() > 0);

    assert!(!document.first_page().items().is_empty());

    assert_eq!(document.first_page().start().spine(), document.spine());
}

#[test]
fn reader_state_rejects_a_stale_document() {
    let source = SliceSource::new(include_bytes!("../../../../fixtures/book-boundaries.epub"));

    let document = future::block_on(load_reader_document(
        String::from("/Fixtures/book-boundaries.epub"),
        source,
    ))
    .unwrap();

    let mut state = ReaderState::default();

    state.open(String::from("/Books/new.epub"), String::from("new"));

    assert!(!state.apply_document(document));

    assert_eq!(state.title(), "new");

    assert!(state.page().is_none());
}

#[test]
fn reader_state_turns_pages_inside_loaded_pagination() {
    let path = String::from("/Fixtures/book-boundaries.epub");

    let source = SliceSource::new(include_bytes!("../../../../fixtures/book-boundaries.epub"));

    let document = future::block_on(load_reader_document(path.clone(), source)).unwrap();

    let page_count = document.page_count();

    assert!(page_count > 0);

    let mut state = ReaderState::default();

    state.open(path, String::from("book-boundaries"));

    assert!(state.apply_document(document));

    assert_eq!(state.page_index(), 0);

    assert!(state.page().is_some());

    assert!(!state.previous_page());

    for expected in 1..page_count {
        assert!(state.next_page());

        assert_eq!(state.page_index(), expected);

        assert!(state.page().is_some());
    }

    assert!(!state.next_page());

    assert_eq!(state.page_index(), page_count - 1);

    for expected in (0..page_count.saturating_sub(1)).rev() {
        assert!(state.previous_page());

        assert_eq!(state.page_index(), expected);

        assert!(state.page().is_some());
    }

    assert_eq!(state.page_index(), 0);

    assert!(!state.previous_page());
}

#[test]
fn next_chapter_request_replaces_chapter_and_lands_on_first_page() {
    let path = String::from("/Fixtures/book-boundaries.epub");

    let source = SliceSource::new(include_bytes!("../../../../fixtures/book-boundaries.epub"));

    let document = future::block_on(load_reader_document(path.clone(), source)).unwrap();

    let from = document.spine();

    let next_spine = SpineIndex::new(from.get().saturating_add(1));

    let next_chapter = ReaderChapter::new(
        String::from("Text/next.xhtml"),
        next_spine,
        document.chapter().pagination().clone(),
    );

    let last_page = document.page_count().saturating_sub(1);

    let mut state = ReaderState::default();

    state.open(path.clone(), String::from("book-boundaries"));

    assert!(state.apply_document(document));

    while state.page_index() < last_page {
        assert!(state.next_page());

        let request = state.take_request();

        if matches!(request, Some(ReaderRequest::UpdateProgress(_))) {
            continue;
        }
    }

    assert!(!state.next_page());

    assert_eq!(
        state.take_request(),
        Some(ReaderRequest::LoadAdjacentChapter {
            path: path.clone(),
            from,
            direction: ReaderChapterDirection::Next,
        }),
    );

    assert!(state.apply_chapter(&path, from, ReaderChapterDirection::Next, next_chapter));

    assert_eq!(state.page_index(), 0);

    assert_eq!(state.document().unwrap().spine(), next_spine);

    assert!(state.page().is_some());
}

#[test]
fn reader_session_supports_multiple_operations_on_one_epub() {
    let path = String::from("/Fixtures/book-boundaries.epub");

    let source = SliceSource::new(include_bytes!("../../../../fixtures/book-boundaries.epub"));

    let mut session = future::block_on(ReaderSession::open(path.clone(), source)).unwrap();

    assert_eq!(session.path(), path);

    let document = future::block_on(session.load_document()).unwrap();

    assert_eq!(document.path(), "/Fixtures/book-boundaries.epub");

    let adjacent = future::block_on(
        session.load_adjacent_chapter(document.spine(), ReaderChapterDirection::Next),
    );

    assert!(adjacent.is_ok());
}

#[test]
fn reader_chrome_tracks_the_current_reading_position() {
    let path = String::from("/Fixtures/book-boundaries.epub");

    let source = SliceSource::new(include_bytes!("../../../../fixtures/book-boundaries.epub"));

    let document = future::block_on(load_reader_document(path.clone(), source)).unwrap();

    let page_count = document.page_count();

    let expected_position = document.first_page().position();

    let mut state = ReaderState::default();

    state.open(path, String::from("book-boundaries"));

    assert!(state.apply_document(document));

    let expected_page_label = format!("1 / {}", page_count);

    assert_eq!(state.page_label(), expected_page_label.as_str());

    assert_eq!(state.reading_position(), Some(expected_position));

    assert!(state.section_label().contains("%  S "));
}

#[test]
fn reader_controls_toggle_only_after_a_book_is_loaded() {
    let mut state = ReaderState::default();

    assert!(!state.toggle_controls());

    assert!(!state.controls_visible());

    let path = String::from("/Fixtures/book-boundaries.epub");

    let source = SliceSource::new(include_bytes!("../../../../fixtures/book-boundaries.epub"));

    let document = future::block_on(load_reader_document(path.clone(), source)).unwrap();

    state.open(path, String::from("book-boundaries"));

    assert!(state.apply_document(document));

    assert!(!state.controls_visible());

    assert!(state.toggle_controls());

    assert!(state.controls_visible());

    assert!(state.toggle_controls());

    assert!(!state.controls_visible());
}

#[test]
fn reader_session_reopens_at_saved_position() {
    let path = String::from("/Fixtures/book-boundaries.epub");

    let source = SliceSource::new(include_bytes!("../../../../fixtures/book-boundaries.epub"));

    let document = future::block_on(load_reader_document(path.clone(), source)).unwrap();

    let target_index = document.page_count().saturating_sub(1);

    let target_position = document.page(target_index).unwrap().position();

    let source = SliceSource::new(include_bytes!("../../../../fixtures/book-boundaries.epub"));

    let mut session = future::block_on(ReaderSession::open(path, source)).unwrap();

    let resumed = future::block_on(session.load_document_at(Some(target_position))).unwrap();

    assert_eq!(resumed.opening_page_index(), target_index);

    assert_eq!(
        resumed
            .page(resumed.opening_page_index())
            .unwrap()
            .position(),
        target_position,
    );
}

#[test]
fn progress_snapshot_contains_epub_metadata_and_book_progress() {
    let path = String::from("/Fixtures/book-boundaries.epub");

    let fallback_title = String::from("book-boundaries");

    let source = SliceSource::new(include_bytes!("../../../../fixtures/book-boundaries.epub"));

    let document = future::block_on(load_reader_document(path.clone(), source)).unwrap();

    let expected_title = String::from(
        document
            .title()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(&fallback_title),
    );

    let expected_creator = document
        .creators()
        .iter()
        .find(|creator| !creator.trim().is_empty())
        .cloned();

    let expected_identifier = document.identifier().map(String::from);

    let expected_position = document.first_page().position();

    let expected_progress = document.progress_at_page(0);

    let mut state = ReaderState::default();

    state.open(path.clone(), fallback_title);

    assert!(state.apply_document(document));

    let Some(ReaderRequest::UpdateProgress(entry)) = state.take_request() else {
        panic!("applying a document must queue reading progress");
    };

    assert_eq!(entry.path(), path);

    assert_eq!(entry.identifier(), expected_identifier.as_deref());

    assert_eq!(entry.title(), expected_title);

    assert_eq!(entry.creator(), expected_creator.as_deref());

    assert_eq!(entry.position(), expected_position);

    assert_eq!(entry.progress(), expected_progress);
}

#[test]
fn broken_epub_image_does_not_make_text_unreadable() {
    let source = SliceSource::new(include_bytes!("../../../../fixtures/broken-image.epub"));

    let document = future::block_on(load_reader_document(
        String::from("/Fixtures/broken-image.epub"),
        source,
    ))
    .unwrap();

    assert!(document.page_count() > 0);

    assert!(
        document
            .first_page()
            .items()
            .iter()
            .any(|item| { matches!(item, inkpaper_reader::PageItem::Text(_)) }),
        "a broken optional image must not discard readable text",
    );
}

#[test]
fn image_only_epub_is_a_readable_chapter() {
    let source = SliceSource::new(include_bytes!("../../../../fixtures/image-only.epub"));

    let document = future::block_on(load_reader_document(
        String::from("/Fixtures/image-only.epub"),
        source,
    ))
    .unwrap();

    assert_eq!(document.page_count(), 1);

    let page = document.first_page();

    assert_eq!(page.start().offset(), inkpaper_epub::ContentOffset::ZERO);
    assert_eq!(page.end().offset(), inkpaper_epub::ContentOffset::ZERO);

    assert_eq!(page.position().non_text(), 0);
    assert_eq!(page.end_position().non_text(), 1);

    assert_eq!(page.items().len(), 1);

    assert!(matches!(
        page.items()[0],
        inkpaper_reader::PageItem::Image(_)
    ));
}

#[test]
fn font_size_repagination_keeps_the_current_reading_position() {
    let path = String::from("/Fixtures/book-boundaries.epub");

    let source = SliceSource::new(include_bytes!("../../../../fixtures/book-boundaries.epub"));

    let mut session = future::block_on(ReaderSession::open(path.clone(), source)).unwrap();

    let document = future::block_on(session.load_document()).unwrap();

    let spine = document.spine();

    let target_index = document.page_count().saturating_sub(1).min(2);

    let target_position = document.page(target_index).unwrap().position();

    let mut state = ReaderState::default();

    state.open(path.clone(), String::from("book-boundaries"));

    // discard the initial open request because this test already loaded the document
    // through the session above.
    let _ = state.take_request();

    assert!(state.apply_document(document));

    // applying the document queues a progress update.
    let _ = state.take_request();

    while state.page_index() < target_index {
        assert!(state.next_page());

        // page turns queue progress updates.
        let _ = state.take_request();
    }

    assert_eq!(state.reading_position(), Some(target_position));

    assert_eq!(state.font_size(), 20);

    assert!(state.increase_font_size());

    assert_eq!(
        state.take_request(),
        Some(ReaderRequest::RepaginateChapter {
            path: path.clone(),
            spine,
            font_size: 22,
        }),
    );

    let chapter = future::block_on(session.repaginate_chapter(spine, 22))
        .unwrap()
        .unwrap();

    assert!(state.apply_repaginated_chapter(&path, spine, 22, chapter));

    assert_eq!(state.font_size(), 22);

    let page = state.page().unwrap();

    assert!(
        page.position() <= target_position && target_position < page.end_position(),
        "repagination must keep the old logical position on the visible page",
    );

    assert!(state.controls_visible());
}

#[test]
fn reader_font_size_stays_within_supported_bounds() {
    let mut state = ReaderState::default();

    assert_eq!(state.font_size(), 20);

    // without a loaded document, adjustments cannot queue repagination.
    assert!(!state.decrease_font_size());
    assert!(!state.increase_font_size());

    assert_eq!(state.font_size(), 20);
}
