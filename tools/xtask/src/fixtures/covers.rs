use anyhow::Result;
use indoc::indoc;

use super::{
    book::{Book, Cover, Epub},
    pixels::cover_png,
};

const TEXT: &str = indoc! {r#"
    <h1>Covers</h1>

    <p>
        Once this book is open, the home screen shows its cover in place of the
        placeholder. The cover names how the book declares it.
    </p>
"#};

/// EPUB 3: the cover image's manifest item has `properties="cover-image"`
pub fn cover() -> Result<Epub> {
    Ok(Book::new("cover", "Cover")
        .chapter("chapter.xhtml", "Covers", TEXT)
        .image("cover.png", cover_png(600, 900, "COVER", "EPUB 3")?)
        .cover(Cover::Property("cover.png"))
        .build())
}

/// EPUB 2: `<meta name="cover">` names a cover page, which wraps its image in
/// SVG like many converted books
pub fn cover_epub2() -> Result<Epub> {
    Ok(Book::new("cover-epub2", "EPUB 2 Cover")
        .chapter(
            "cover.xhtml",
            "Cover",
            indoc! {r#"
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    xmlns:xlink="http://www.w3.org/1999/xlink"
                    viewBox="0 0 600 900"
                >
                    <image width="600" height="900" xlink:href="cover.png"/>
                </svg>
            "#},
        )
        .chapter("chapter.xhtml", "Covers", TEXT)
        .image("cover.png", cover_png(600, 900, "COVER", "EPUB 2")?)
        .cover(Cover::Meta("cover.xhtml"))
        .build())
}

/// only a `<guide>` reference points at the cover page
pub fn cover_guide() -> Result<Epub> {
    Ok(Book::new("cover-guide", "Guide Cover")
        .chapter(
            "cover.xhtml",
            "Cover",
            r#"<p><img src="cover.png" alt="Cover"/></p>"#,
        )
        .chapter("chapter.xhtml", "Covers", TEXT)
        .image("cover.png", cover_png(600, 900, "COVER", "GUIDE")?)
        .cover(Cover::Guide("cover.xhtml"))
        .build())
}
