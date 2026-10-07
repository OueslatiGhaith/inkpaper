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

/// footnotes at the end of a chapter and in a chapter of their own, links to
/// another chapter and to an anchor inside it, and an external link
pub fn links() -> Result<Epub> {
    let mut first = String::from(indoc! {r##"
        <h1>Links</h1>

        <p>
            Internal links are underlined. This sentence has a footnote<a
            id="ref-1" href="#note-1">[1]</a>, and so does this one<a id="ref-2"
            href="#note-2">[2]</a>. A third note lives in the endnotes
            chapter<a id="ref-3" href="endnotes.xhtml#note-3">[3]</a>.
        </p>

        <p>
            Links can open <a href="destinations.xhtml">the next chapter</a>
            or <a href="destinations.xhtml#middle">the middle of it</a>. An
            external link to <a href="https://example.com">example.com</a> is
            not underlined, since the reader can't open it.
        </p>
    "##});

    // push the notes onto a later page, as at the end of a real chapter
    for index in 1..=10 {
        first.push_str(&formatdoc! {r##"
            <p>
                Filler paragraph {index}. Following a footnote and coming back
                should return to the page with its marker, not to the start of
                the chapter.
            </p>
        "##});
    }

    first.push_str(indoc! {r##"
        <h2>Notes</h2>

        <p id="note-1">
            <a href="#ref-1">1.</a> The first note, at the end of its chapter.
            Its number links back to the marker.
        </p>

        <p id="note-2">
            <a href="#ref-2">2.</a> The second note.
        </p>
    "##});

    let mut destinations = String::from("<h1>Destinations</h1>\n");

    for index in 1..=12 {
        destinations.push_str(&formatdoc! {r##"
            <p>
                Destinations paragraph {index}. A link to this chapter opens
                at its start; the anchor below is a few pages in.
            </p>
        "##});
    }

    destinations.push_str(indoc! {r##"
        <h2 id="middle">The Middle</h2>
        <p>The link to the middle of this chapter lands on this heading.</p>
    "##});

    Ok(Book::new("links", "Links")
        .chapter("links.xhtml", "Links", first)
        .chapter("destinations.xhtml", "Destinations", destinations)
        .chapter(
            "endnotes.xhtml",
            "Endnotes",
            indoc! {r##"
                <h1>Endnotes</h1>

                <p id="note-3">
                    <a href="links.xhtml#ref-3">3.</a> The third note, in a
                    chapter of its own. Its number links back across chapters.
                </p>
            "##},
        )
        .build())
}
