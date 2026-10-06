use anyhow::Result;
use indoc::{formatdoc, indoc};

use super::book::{Book, Epub, TocEntry};

pub fn navigation_anchors() -> Result<Epub> {
    let mut first = String::from("<h1>Chapter 1</h1>\n");
    let mut second = String::from("<h1>Chapter 2</h1>\n");

    for index in 1..=4 {
        first.push_str(&format!("<p>Chapter 1, paragraph {index}.</p>\n"));
    }

    // enough text before the anchor to push it past the first page
    for index in 1..=24 {
        second.push_str(&formatdoc! {r#"
            <p>
                Chapter 2, paragraph {index}. Navigation entries can point into
                the middle of a chapter, so this text pushes the anchored
                heading below onto a later page.
            </p>
        "#});
    }

    second.push_str(indoc! {r#"
        <h2 id="later-section">Later Section</h2>
        <p>The table of contents should open the reader on this heading.</p>
    "#});

    Ok(Book::new("navigation-anchors", "Navigation Anchors")
        .chapter("chapter-1.xhtml", "Chapter 1", first)
        .chapter("chapter-2.xhtml", "Chapter 2", second)
        .toc(vec![
            TocEntry::new("chapter-1.xhtml", "Chapter 1"),
            TocEntry::new("chapter-2.xhtml", "Chapter 2").child(TocEntry::new(
                "chapter-2.xhtml#later-section",
                "Later Section",
            )),
        ])
        .build())
}

pub fn book_boundaries() -> Result<Epub> {
    Ok(Book::new("book-boundaries", "Book Boundaries")
        .chapter(
            "first.xhtml",
            "First",
            indoc! {r#"
                <p>
                    Press Previous for Beginning of book. Press Next to reach the
                    last chapter, then Next again for End of book.
                </p>
            "#},
        )
        .chapter(
            "last.xhtml",
            "Last",
            indoc! {r#"
                <p>
                    Last chapter. Press Next for End of book. Press Previous to
                    return and clear the notice.
                </p>
            "#},
        )
        .build())
}

/// the second chapter is in the spine but missing from the archive
pub fn broken_chapter() -> Result<Epub> {
    Ok(Book::new("broken-chapter", "Broken Chapter")
        .chapter(
            "first.xhtml",
            "First",
            indoc! {r#"
                <p>
                    Press Previous for Beginning of book. Press Next to try
                    loading the deliberately missing chapter.
                </p>
            "#},
        )
        .chapter("missing.xhtml", "Missing", "")
        .omit("OEBPS/missing.xhtml")
        .build())
}
