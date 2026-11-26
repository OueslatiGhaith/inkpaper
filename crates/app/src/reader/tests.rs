use alloc::{format, string::String, vec, vec::Vec};
use futures_lite::future;
use inkpaper_epub::{Epub, SpineIndex};
use inkpaper_io::SliceSource;
use inkpaper_reader::PageItem;

use crate::{
    ReaderChapterDirection, ReaderPreferences, ReaderRequest, ReaderSession,
    reader::{
        JumpTarget, LineSpacing, ReaderState, TableOfContents, TextSetting, TextSettings,
        load_reader_document,
    },
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

    let next_chapter = document
        .chapter()
        .with_path_and_spine(String::from("Text/next.xhtml"), next_spine);

    let last_page = document.page_count().saturating_sub(1);

    let mut state = ReaderState::default();

    state.open(path.clone(), String::from("book-boundaries"));

    assert!(state.apply_document(document));
    assert!(matches!(
        state.take_request(),
        Some(ReaderRequest::UpdateProgress(_)),
    ));

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

    assert!(state.progress_label().ends_with('%'));
}

#[test]
fn reader_menu_opens_only_after_a_book_is_loaded() {
    let mut state = ReaderState::default();

    assert!(!state.open_menu());

    assert!(!state.menu_open());

    let path = String::from("/Fixtures/book-boundaries.epub");

    let source = SliceSource::new(include_bytes!("../../../../fixtures/book-boundaries.epub"));

    let document = future::block_on(load_reader_document(path.clone(), source)).unwrap();

    state.open(path, String::from("book-boundaries"));

    assert!(state.apply_document(document));

    assert!(!state.menu_open());

    assert!(state.open_menu());

    assert!(state.menu_open());

    assert!(state.close_menu());

    assert!(!state.menu_open());
}

#[test]
fn reader_session_reopens_at_saved_position() {
    let path = String::from("/Fixtures/book-boundaries.epub");

    let source = SliceSource::new(include_bytes!("../../../../fixtures/book-boundaries.epub"));

    let document = future::block_on(load_reader_document(path.clone(), source)).unwrap();

    let target_index = document.page_count().saturating_sub(1);

    let target_position = document.page_position(target_index).unwrap();

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

    let target_position = document.page_position(target_index).unwrap();

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

    let text = TextSettings::default().with_font_size(22).unwrap();

    assert!(state.request_text_settings(text));

    assert_eq!(
        state.take_request(),
        Some(ReaderRequest::RepaginateChapter {
            path: path.clone(),
            spine,
            text,
        }),
    );

    let chapter = future::block_on(session.repaginate_chapter(spine, text, None))
        .unwrap()
        .unwrap();

    assert!(state.apply_repaginated_chapter(&path, spine, text, chapter));

    assert_eq!(state.font_size(), 22);

    let page = state.page().unwrap();

    assert!(
        page.position() <= target_position && target_position < page.end_position(),
        "repagination must keep the old logical position on the visible page",
    );
}

#[test]
fn reader_font_size_stays_within_supported_bounds() {
    let text = TextSettings::default();

    assert_eq!(text.font_size(), 20);
    assert!(text.with_font_size(12).is_none());
    assert!(text.with_font_size(34).is_none());

    // without a loaded document, changes cannot queue repagination.
    let mut state = ReaderState::default();

    assert!(!state.request_text_settings(text.with_font_size(22).unwrap()));
    assert_eq!(state.font_size(), 20);
}

#[test]
fn persisted_reader_preferences_are_used_when_opening_a_book() {
    let mut state = ReaderState::default();

    // AppService::ensure_initialized pushes preferences at boot without an extra Load round-trip.
    assert_eq!(state.take_preferences_request(), None);

    let text = TextSettings::default().with_font_size(26).unwrap();
    let preferences = ReaderPreferences::new(text);

    assert!(state.apply_preferences(preferences));

    assert_eq!(state.font_size(), 26);

    let path = String::from("/Books/book.epub");

    state.open(path.clone(), String::from("Book"));

    assert_eq!(
        state.take_request(),
        Some(ReaderRequest::OpenEpub {
            path,
            text,
            resume: false
        }),
    );
}

#[test]
fn navigation_target_jump_opens_the_page_with_the_anchored_heading() {
    const BYTES: &[u8] = include_bytes!("../../../../fixtures/navigation-anchors.epub");
    let path = String::from("/Fixtures/navigation-anchors.epub");

    // Chapter 2 > Later Section points at chapter-2.xhtml#later-section
    let mut epub = future::block_on(Epub::open(SliceSource::new(BYTES))).unwrap();
    let navigation = future::block_on(epub.load_navigation()).unwrap().unwrap();
    let target = navigation.entries()[1].children()[0].target().unwrap();
    let spine = epub.package().spine_index_for_path(target.path()).unwrap();
    let spine = SpineIndex::try_from_usize(spine).unwrap();
    let anchor = target.fragment().map(String::from);

    let mut session =
        future::block_on(ReaderSession::open(path.clone(), SliceSource::new(BYTES))).unwrap();
    let document = future::block_on(session.load_document()).unwrap();

    let mut state = ReaderState::default();
    state.open(path.clone(), String::from("navigation-anchors"));
    assert!(state.apply_document(document));
    let _ = state.take_request();

    assert!(state.jump_to(spine, anchor.clone()));
    assert_eq!(
        state.take_request(),
        Some(ReaderRequest::JumpTo {
            path: path.clone(),
            target: JumpTarget::Chapter {
                spine,
                anchor: anchor.clone(),
            },
        }),
    );

    let (chapter, page_index) = future::block_on(session.load_chapter_at(spine, anchor.as_deref()))
        .unwrap()
        .unwrap();

    // the heading sits past the first page of its chapter
    assert!(page_index > 0);
    let target = JumpTarget::Chapter {
        spine,
        anchor: anchor.clone(),
    };
    assert!(state.apply_jump(&path, target.clone(), chapter, page_index));

    let page = state.page().unwrap();
    assert!(page.items().iter().any(|item| matches!(
        item,
        PageItem::Text(text) if text.text().contains("Later Section")
    )));

    // landing on the new page records progress, and a repeated result is stale
    assert!(matches!(
        state.take_request(),
        Some(ReaderRequest::UpdateProgress(_))
    ));
    assert!(!state.finish_jump_request(&path, target));
}

#[test]
fn every_text_setting_option_chooses_the_value_it_shows() {
    let text = TextSettings::default();

    let settings = TextSetting::PANEL
        .into_iter()
        .chain(TextSetting::LAYOUT)
        .chain(TextSetting::STYLE);

    for setting in settings {
        let options = setting.options();
        let current = setting.selected(text).unwrap();

        assert_eq!(options[current], setting.value(text));

        for (index, option) in options.iter().enumerate() {
            let chosen = setting.choose(text, index).unwrap();

            assert_eq!(&setting.value(chosen), option);
            assert_eq!(setting.selected(chosen), Some(index));
        }

        assert!(setting.choose(text, options.len()).is_none());
    }

    assert_eq!(TextSetting::FontSize.options().len(), 10);
    assert_eq!(TextSetting::ScreenMargin.options().len(), 8);
}

#[test]
fn text_picker_choice_repaginates_and_shows_while_repaginating() {
    let path = String::from("/Fixtures/book-boundaries.epub");
    let source = SliceSource::new(include_bytes!("../../../../fixtures/book-boundaries.epub"));
    let document = future::block_on(load_reader_document(path.clone(), source)).unwrap();

    let mut state = ReaderState::default();
    state.open(path.clone(), String::from("book-boundaries"));
    assert!(state.apply_document(document));
    let _ = state.take_request();

    // pickers only choose from the drawer
    assert!(!state.choose_text_option(TextSetting::LineSpacing, 3));
    assert!(state.open_menu());
    assert!(state.take_request().is_none());

    assert!(state.choose_text_option(TextSetting::LineSpacing, 3));
    assert!(matches!(
        state.take_request(),
        Some(ReaderRequest::RepaginateChapter { text, .. })
            if text.line_spacing() == LineSpacing::ExtraWide
    ));
    assert_eq!(
        state.menu_text_settings().line_spacing(),
        LineSpacing::ExtraWide
    );
    // no other choice applies while the chapter repaginates
    assert!(!state.choose_text_option(TextSetting::LineSpacing, 3));
}

#[test]
fn table_of_contents_loads_once_flattened_with_targets_and_marks_the_current_chapter() {
    const BYTES: &[u8] = include_bytes!("../../../../fixtures/navigation-anchors.epub");
    let path = String::from("/Fixtures/navigation-anchors.epub");

    let mut session =
        future::block_on(ReaderSession::open(path.clone(), SliceSource::new(BYTES))).unwrap();
    let document = future::block_on(session.load_document()).unwrap();

    let mut state = ReaderState::default();
    assert!(!state.request_table_of_contents());

    state.open(path.clone(), String::from("navigation-anchors"));
    assert!(state.apply_document(document));
    let _ = state.take_request();

    assert!(state.request_table_of_contents());
    assert_eq!(
        state.take_request(),
        Some(ReaderRequest::LoadTableOfContents { path: path.clone() }),
    );
    assert!(!state.request_table_of_contents());

    let entries = future::block_on(session.load_table_of_contents()).unwrap();
    let rows: Vec<_> = entries
        .iter()
        .map(|entry| {
            let target = entry.target().unwrap();
            (
                entry.label(),
                entry.depth(),
                target.spine.get(),
                target.anchor.as_deref(),
            )
        })
        .collect();

    assert_eq!(
        rows,
        [
            ("Chapter 1", 0, 0, None),
            ("Chapter 2", 0, 1, None),
            ("Later Section", 1, 1, Some("later-section")),
        ],
    );

    assert!(state.apply_table_of_contents(&path, entries));
    assert!(matches!(
        state.table_of_contents(),
        TableOfContents::Loaded(_)
    ));

    // the book opens on chapter 1
    assert_eq!(state.current_toc_index(), Some(0));

    // in chapter 2 the chapter's own entry is current, not its nested section
    let spine = SpineIndex::new(1);
    assert!(state.jump_to(spine, None));
    let (chapter, page_index) = future::block_on(session.load_chapter_at(spine, None))
        .unwrap()
        .unwrap();
    let target = JumpTarget::Chapter {
        spine,
        anchor: None,
    };
    assert!(state.apply_jump(&path, target, chapter, page_index));
    assert_eq!(state.current_toc_index(), Some(1));
}

#[test]
fn turning_back_while_adjacent_chapter_is_pending_does_not_clobber_or_deadlock() {
    const BYTES: &[u8] = include_bytes!("../../../../fixtures/navigation-anchors.epub");
    let path = String::from("/Fixtures/navigation-anchors.epub");

    let mut session =
        future::block_on(ReaderSession::open(path.clone(), SliceSource::new(BYTES))).unwrap();
    let mut document = future::block_on(session.load_document()).unwrap();
    let spine = SpineIndex::new(1);
    let (chapter, _) = future::block_on(session.load_chapter_at(spine, None))
        .unwrap()
        .unwrap();
    document.replace_chapter(chapter);

    let last_page = document.page_count().saturating_sub(1);
    assert!(last_page > 0, "chapter 2 fixture should span multiple pages");

    let mut state = ReaderState::default();
    state.open(path.clone(), String::from("navigation-anchors"));
    assert!(state.apply_document(document));
    let _ = state.take_request();

    while state.page_index() < last_page {
        assert!(state.next_page());
        let _ = state.take_request();
    }

    // Request next chapter at the end of the current one.
    assert!(!state.next_page());

    // Pressing previous_page before the chapter request is serviced must be
    // rejected while chapter navigation is in flight, rather than overwriting
    // the pending chapter request with UpdateProgress.
    assert!(!state.previous_page());
    assert_eq!(state.page_index(), last_page);

    assert_eq!(
        state.take_request(),
        Some(ReaderRequest::LoadAdjacentChapter {
            path: path.clone(),
            from: spine,
            direction: ReaderChapterDirection::Next,
        }),
    );

    // Completing the chapter request unlocks navigation again.
    assert!(state.finish_chapter_request(&path, spine, ReaderChapterDirection::Next));
    assert!(state.previous_page());
    assert_eq!(state.page_index(), last_page - 1);
}

#[test]
fn toc_request_and_progress_or_navigation_requests_do_not_clobber_each_other() {
    const BYTES: &[u8] = include_bytes!("../../../../fixtures/navigation-anchors.epub");
    let path = String::from("/Fixtures/navigation-anchors.epub");

    let mut session =
        future::block_on(ReaderSession::open(path.clone(), SliceSource::new(BYTES))).unwrap();
    let document = future::block_on(session.load_document()).unwrap();

    let mut state = ReaderState::default();
    state.open(path.clone(), String::from("navigation-anchors"));
    assert!(state.apply_document(document));

    // Request TOC before draining the initial UpdateProgress from apply_document,
    // and also queue a JumpTo while TOC is still pending.
    assert!(state.request_table_of_contents());
    assert_eq!(state.table_of_contents(), &TableOfContents::Loading);
    assert!(state.jump_to(SpineIndex::new(1), None));

    // All three independent requests drain in order without clobbering.
    assert!(matches!(
        state.take_request(),
        Some(ReaderRequest::UpdateProgress(_))
    ));
    assert_eq!(
        state.take_request(),
        Some(ReaderRequest::JumpTo {
            path: path.clone(),
            target: JumpTarget::Chapter {
                spine: SpineIndex::new(1),
                anchor: None,
            },
        }),
    );
    assert_eq!(
        state.take_request(),
        Some(ReaderRequest::LoadTableOfContents { path: path.clone() }),
    );
    assert_eq!(state.take_request(), None);
}

#[test]
fn chapter_images_are_decoded_lazily_per_page() {
    // Valid 1x1 white grayscale PNG (67 bytes).
    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00, 0x00, 0x00, 0x3a,
        0x7e, 0x9b, 0x55, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8,
        0x0f, 0x00, 0x01, 0x01, 0x01, 0x00, 0x1b, 0xb6, 0xee, 0x56, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    fn build_stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut archive = Vec::new();
        let mut central = Vec::new();
        for &(name, data) in entries {
            let local_offset = u32::try_from(archive.len()).unwrap();
            let name_bytes = name.as_bytes();
            let size = u32::try_from(data.len()).unwrap();
            archive.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            archive.extend_from_slice(&20u16.to_le_bytes());
            archive.extend_from_slice(&0u16.to_le_bytes());
            archive.extend_from_slice(&0u16.to_le_bytes());
            archive.extend_from_slice(&0u16.to_le_bytes());
            archive.extend_from_slice(&0u16.to_le_bytes());
            archive.extend_from_slice(&0u32.to_le_bytes());
            archive.extend_from_slice(&size.to_le_bytes());
            archive.extend_from_slice(&size.to_le_bytes());
            archive.extend_from_slice(&u16::try_from(name_bytes.len()).unwrap().to_le_bytes());
            archive.extend_from_slice(&0u16.to_le_bytes());
            archive.extend_from_slice(name_bytes);
            archive.extend_from_slice(data);

            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&u16::try_from(name_bytes.len()).unwrap().to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&local_offset.to_le_bytes());
            central.extend_from_slice(name_bytes);
        }
        let central_offset = u32::try_from(archive.len()).unwrap();
        let central_size = u32::try_from(central.len()).unwrap();
        let count = u16::try_from(entries.len()).unwrap();
        archive.extend_from_slice(&central);
        archive.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&count.to_le_bytes());
        archive.extend_from_slice(&count.to_le_bytes());
        archive.extend_from_slice(&central_size.to_le_bytes());
        archive.extend_from_slice(&central_offset.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive
    }

    const CONTAINER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OPS/package.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;

    const PACKAGE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Lazy Images</dc:title></metadata>
  <manifest>
    <item id="ch1" href="Text/ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="img1" href="Images/first.png" media-type="image/png"/>
    <item id="img2" href="Images/second.png" media-type="image/png"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;

    let mut body = String::from(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p><img src="../Images/first.png" alt="First"/></p>"#,
    );
    for i in 0..45 {
        body.push_str(&format!(
            "<p>Paragraph {i} with enough words to fill a full reader page before the second illustration appears.</p>"
        ));
    }
    body.push_str(r#"<p><img src="../Images/second.png" alt="Second"/></p></body></html>"#);

    let zip = build_stored_zip(&[
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OPS/package.opf", PACKAGE.as_bytes()),
        ("OPS/Text/ch1.xhtml", body.as_bytes()),
        ("OPS/Images/first.png", TINY_PNG),
        ("OPS/Images/second.png", TINY_PNG),
    ]);

    let first_path = inkpaper_epub::ArchivePath::new("OPS/Images/first.png").unwrap();
    let second_path = inkpaper_epub::ArchivePath::new("OPS/Images/second.png").unwrap();
    let book_path = String::from("/Fixtures/lazy-images.epub");

    let mut session =
        future::block_on(ReaderSession::open(book_path.clone(), SliceSource::new(&zip))).unwrap();
    let document = future::block_on(session.load_document()).unwrap();

    assert!(document.page_count() >= 2);
    // Opening page 0 decoded only the first image, not the image on the last page.
    assert!(document.chapter().has_decoded_image(&first_path));
    assert!(!document.chapter().has_decoded_image(&second_path));

    let last_page = document.page_count() - 1;
    let spine = document.spine();

    let mut state = ReaderState::default();
    state.open(book_path.clone(), String::from("Lazy Images"));
    assert!(state.apply_document(document));
    let _ = state.take_request(); // initial UpdateProgress

    let mut requested_images = None;
    while state.page_index() < last_page {
        assert!(state.next_page());
        while let Some(request) = state.take_request() {
            if let ReaderRequest::LoadPageImages {
                path,
                spine: req_spine,
                page_index,
                images,
            } = request
            {
                requested_images = Some((path, req_spine, page_index, images));
            }
        }
    }

    let (req_path, req_spine, req_page, images) =
        requested_images.expect("turning to the last page must request its image lazily");
    assert_eq!(req_path, book_path);
    assert_eq!(req_spine, spine);
    assert_eq!(req_page, last_page);
    assert_eq!(images, vec![second_path.clone()]);

    let decoded = future::block_on(session.load_page_images(&images));
    assert!(state.apply_page_images(&req_path, req_spine, req_page, decoded));
    assert!(
        state
            .document()
            .unwrap()
            .chapter()
            .has_decoded_image(&second_path)
    );
}
