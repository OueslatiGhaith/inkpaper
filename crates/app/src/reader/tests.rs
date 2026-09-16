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

    assert!(state.section_label().starts_with("S "));
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
