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

/// text for the reader's typography settings. Later settings add chapters here
pub fn typography() -> Result<Epub> {
    let mut plain = String::from("<h1>Plain Paragraphs</h1>\n");

    for index in 1..=12 {
        plain.push_str(&formatdoc! {r#"
            <p>
                Paragraph {index} has no styles of its own. Line spacing, margins
                and font size come only from the reader settings, so every page
                of this chapter changes as they do.
            </p>
        "#});
    }

    Ok(Book::new("typography", "Typography")
        .stylesheet(indoc! {r#"
            .tight {
                line-height: 1;
            }

            .loose {
                line-height: 2.5;
            }

            .book p {
                margin: 0;
                text-indent: 3em;
            }

            .book p.centered {
                text-align: center;
            }
        "#})
        .chapter("chapter-1.xhtml", "Plain Paragraphs", plain)
        .chapter(
            "chapter-2.xhtml",
            "Book Line Height",
            indoc! {r#"
                <h1>Book Line Height</h1>

                <p class="tight">
                    This paragraph asks for a line height of 1 in the book's CSS.
                    The reader's line spacing setting replaces it, so its lines
                    are spaced like every other paragraph.
                </p>

                <p class="loose">
                    This paragraph asks for a line height of 2.5. It too follows
                    the reader's setting instead, with the same spacing as the
                    paragraphs around it.
                </p>

                <p>
                    This paragraph has no line height of its own, for comparison
                    with the two above.
                </p>
            "#},
        )
        .chapter(
            "chapter-3.xhtml",
            "Book Paragraphs",
            indoc! {r#"
                <h1>Book Paragraphs</h1>

                <div class="book">
                    <p>
                        Like many books, this chapter's CSS indents each paragraph
                        by 3 em and removes the space between them. The paragraph
                        indentation setting replaces that indent with its own, and
                        extra paragraph spacing adds half a line after each one.
                    </p>
                    <p>
                        A second paragraph shows both settings at once: its first
                        line starts at the reader's indent, and the gap above it is
                        the extra spacing alone, since the book asks for none.
                    </p>
                    <p class="centered">A centered paragraph is never indented.</p>
                    <p>
                        A last paragraph closes the chapter, so the spacing after
                        the centered one can be compared with the rest.
                    </p>
                </div>
            "#},
        )
        .chapter(
            "chapter-4.xhtml",
            "Dialogue",
            indoc! {r#"
                <h1>Dialogue</h1>

                <p>“Is it raining?”</p>
                <p>“Not yet.”</p>
                <p>“Then we walk.”</p>
                <p>“And if it starts?”</p>
                <p>“Then we walk faster.”</p>
                <p>
                    Short lines like these show the spacing between lines and
                    paragraphs most clearly, since each paragraph is a single line.
                </p>
            "#},
        )
        .build())
}
