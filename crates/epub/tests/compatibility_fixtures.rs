use futures_lite::future;
use inkpaper_epub::{Chapter, Epub, FontStyle, Inline, SliceSource, TextAlign};

const ENCODED_CONTAINER_PATH: &[u8] =
    include_bytes!("../../../fixtures/compat-encoded-container-path.epub");

const EPUB2_ENTITIES: &[u8] = include_bytes!("../../../fixtures/compat-epub2-entities.epub");

const MANIFEST_FALLBACK: &[u8] = include_bytes!("../../../fixtures/compat-manifest-fallback.epub");

const CSS_DESCENDANT_SELECTOR: &[u8] =
    include_bytes!("../../../fixtures/compat-css-descendant-selector.epub");

fn chapter_text(chapter: &Chapter) -> String {
    let mut output = String::new();

    for block in chapter.blocks() {
        for inline in block.inlines() {
            if let Inline::Text(text) = inline {
                output.push_str(text.text());
            }
        }
    }

    output
}

#[test]
fn opens_percent_encoded_container_rootfile() {
    let mut epub = future::block_on(Epub::open(SliceSource::new(ENCODED_CONTAINER_PATH))).unwrap();

    assert_eq!(
        epub.metadata().title(),
        Some("Compatibility — Encoded Container Path"),
    );

    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();

    assert_eq!(chapter.path().as_str(), "OEBPS/My Book/chapter.xhtml",);
}

#[test]
fn decodes_epub2_xhtml_named_entities() {
    let mut epub = future::block_on(Epub::open(SliceSource::new(EPUB2_ENTITIES))).unwrap();

    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();

    assert_eq!(
        chapter_text(&chapter),
        "EPUB 2 EntitiesFish\u{00A0}Chips © 2026 — EPUB 2",
    );
}

#[test]
fn follows_manifest_fallback_for_unsupported_spine_resource() {
    let mut epub = future::block_on(Epub::open(SliceSource::new(MANIFEST_FALLBACK))).unwrap();

    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();

    assert_eq!(chapter.path().as_str(), "OEBPS/chapter.xhtml",);

    assert!(chapter_text(&chapter).contains("InkPaper should render this XHTML document",),);
}

#[test]
#[ignore = "known gap: CSS descendant selectors are unsupported"]
fn applies_css_descendant_selector() {
    let mut epub = future::block_on(Epub::open(SliceSource::new(CSS_DESCENDANT_SELECTOR))).unwrap();

    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();

    let styles = future::block_on(epub.load_chapter_styles(&chapter)).unwrap();

    let text = chapter
        .blocks()
        .iter()
        .flat_map(|block| block.inlines())
        .find_map(|inline| {
            let Inline::Text(text) = inline else {
                return None;
            };

            (text.text() == "THIS PARAGRAPH SHOULD BE CENTERED AND ITALIC.").then_some(text)
        })
        .unwrap();

    let style = styles.style(text.style_node()).unwrap();

    assert_eq!(style.font_style(), FontStyle::Italic);
    assert_eq!(style.text_align(), TextAlign::Center);
}
