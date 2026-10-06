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

pub fn italics() -> Result<Epub> {
    let mut long = String::from("<h1>Long Italic Paragraph</h1>\n<p><em>\n");

    // one paragraph long enough to cross a page break
    for index in 1..=24 {
        long.push_str(&formatdoc! {r#"
            Sentence {index} stays italic from the first word to the last,
            including where the paragraph wraps onto the next page.
        "#});
    }

    long.push_str("</em></p>\n<p>This paragraph after it is upright again.</p>\n");

    Ok(Book::new("italics", "Italics")
        .stylesheet(indoc! {r#"
            .italic {
                font-style: italic;
            }

            .oblique {
                font-style: oblique 10deg;
            }

            .upright {
                font-style: normal;
            }

            h2.italic-heading {
                font-style: italic;
            }
        "#})
        .chapter(
            "chapter-1.xhtml",
            "Italic Text",
            indoc! {r#"
                <h1>Italic Text</h1>

                <p>Upright text, then <em>em text</em>, then <i>i text</i>.</p>

                <p>
                    Bold and italic together: <b><i>b around i</i></b>,
                    <strong><em>strong around em</em></strong> and
                    <em><strong>em around strong</strong></em>. Each should use
                    the bold italic face.
                </p>

                <p class="italic">
                    This whole paragraph is italic through a CSS class.
                </p>

                <p class="oblique">
                    This paragraph asks for oblique, which renders as italic.
                </p>

                <p class="italic">
                    Italic through a class, with
                    <span class="upright">an upright span</span> that turns it
                    off, and <em>em inside the italic</em> that stays italic.
                </p>

                <h2 class="italic-heading">An Italic Heading</h2>

                <p>
                    Italic inside one word: un<em>believ</em>able, and
                    <em>italic</em>, punctuated: <em>“quoted,”</em> <em>(parenthetical)</em>.
                </p>

                <p>
                    Scripts Libron lacks fall back to Inter, which has no italic:
                    <em>Greek αβγ δέλτα and Cyrillic Привет мир</em>.
                </p>
            "#},
        )
        .chapter("chapter-2.xhtml", "Long Italic Paragraph", long)
        .build())
}
