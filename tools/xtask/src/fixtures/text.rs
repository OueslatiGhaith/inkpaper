use anyhow::Result;
use indoc::{formatdoc, indoc};

use super::book::{Book, Epub};

pub fn basic_text() -> Result<Epub> {
    Ok(Book::new("basic-text", "Basic Text")
        .chapter(
            "chapter.xhtml",
            "Basic Text",
            indoc! {r#"
                <h1>Basic Text</h1>
                <p>This is the smallest known-good InkPaper EPUB fixture.</p>
                <p>
                    It verifies opening, pagination, rendering and reading progress
                    without unusual content.
                </p>
            "#},
        )
        .build())
}

pub fn long_text() -> Result<Epub> {
    let mut body = String::from("<h1>Long Text</h1>\n");

    for index in 1..=32 {
        body.push_str(&formatdoc! {r#"
            <p>
                Paragraph {index}. InkPaper should paginate this text
                deterministically. This sentence deliberately contains enough
                words to wrap over multiple lines on the X4 Pro viewport.
                Moving forward and backward should preserve page boundaries.
            </p>
        "#});
    }

    Ok(Book::new("long-text", "Long Text")
        .chapter("chapter.xhtml", "Long Text", body)
        .build())
}

pub fn chapters() -> Result<Epub> {
    let mut book = Book::new("chapters", "Chapters");

    for chapter_index in 1..=4 {
        let mut body = format!("<h1>Chapter {chapter_index}</h1>\n");

        for paragraph_index in 1..=8 {
            body.push_str(&formatdoc! {r#"
                <p>
                    Chapter {chapter_index}, paragraph {paragraph_index}.
                    This content exercises crossing spine boundaries in both
                    directions and preserving whole-book progress.
                </p>
            "#});
        }

        book = book.chapter(
            format!("chapter-{chapter_index}.xhtml"),
            format!("Chapter {chapter_index}"),
            body,
        );
    }

    Ok(book.build())
}
