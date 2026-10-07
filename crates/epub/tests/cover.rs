use futures_lite::future;
use inkpaper_epub::{ArchivePath, Epub, SliceSource};

fn cover_image(bytes: &[u8]) -> Option<ArchivePath> {
    let mut epub = future::block_on(Epub::open(SliceSource::new(bytes))).unwrap();

    let cover = future::block_on(epub.cover_image()).unwrap();

    // the cover can be read like any manifest resource
    if let Some(path) = &cover {
        let bytes = future::block_on(epub.read_resource(path)).unwrap().unwrap();
        assert!(bytes.starts_with(b"\x89PNG"));
    }

    cover
}

#[test]
fn finds_the_epub3_cover_image() {
    assert_eq!(
        cover_image(include_bytes!("../../../fixtures/cover.epub")),
        ArchivePath::new("OEBPS/cover.png").ok(),
    );
}

#[test]
fn finds_the_image_of_an_epub2_cover_page() {
    assert_eq!(
        cover_image(include_bytes!("../../../fixtures/cover-epub2.epub")),
        ArchivePath::new("OEBPS/cover.png").ok(),
    );
}

#[test]
fn finds_the_image_of_a_guide_cover_page() {
    assert_eq!(
        cover_image(include_bytes!("../../../fixtures/cover-guide.epub")),
        ArchivePath::new("OEBPS/cover.png").ok(),
    );
}

#[test]
fn books_without_a_declared_cover_have_none() {
    // its chapter has an image, but crosspoint does not guess
    assert_eq!(
        cover_image(include_bytes!("../../../fixtures/mixed-content.epub")),
        None,
    );
}
