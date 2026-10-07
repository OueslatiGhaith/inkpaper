use futures_lite::future;
use inkpaper_epub::{ArchivePath, Epub, Inline, LinkTarget, SliceSource};

const LINKS: &[u8] = include_bytes!("../../../fixtures/links.epub");

/// each linked run's text and target, in reading order
fn links(spine: usize) -> Vec<(String, LinkTarget)> {
    let mut epub = future::block_on(Epub::open(SliceSource::new(LINKS))).unwrap();
    let chapter = future::block_on(epub.load_spine_chapter(spine))
        .unwrap()
        .unwrap();

    chapter
        .blocks()
        .iter()
        .flat_map(|block| block.inlines())
        .filter_map(|inline| match inline {
            Inline::Text(run) => run
                .link()
                .map(|link| (String::from(run.text().trim()), link.clone())),
            _ => None,
        })
        .collect()
}

fn internal(path: &str, fragment: Option<&str>) -> LinkTarget {
    LinkTarget::Internal {
        path: ArchivePath::new(path).unwrap(),
        fragment: fragment.map(String::from),
    }
}

#[test]
fn the_links_fixture_has_notes_chapter_links_and_an_external_link() {
    assert_eq!(
        links(0)[..7],
        [
            (
                String::from("[1]"),
                internal("OEBPS/links.xhtml", Some("note-1"))
            ),
            (
                String::from("[2]"),
                internal("OEBPS/links.xhtml", Some("note-2"))
            ),
            (
                String::from("[3]"),
                internal("OEBPS/endnotes.xhtml", Some("note-3"))
            ),
            (
                String::from("the next chapter"),
                internal("OEBPS/destinations.xhtml", None)
            ),
            (
                String::from("the middle of it"),
                internal("OEBPS/destinations.xhtml", Some("middle"))
            ),
            (
                String::from("example.com"),
                LinkTarget::External(String::from("https://example.com"))
            ),
            (
                String::from("1."),
                internal("OEBPS/links.xhtml", Some("ref-1"))
            ),
        ],
    );

    assert_eq!(
        links(2),
        [(
            String::from("3."),
            internal("OEBPS/links.xhtml", Some("ref-3"))
        )],
    );
}
