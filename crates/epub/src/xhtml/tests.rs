use crate::XhtmlError;

use super::*;

fn visible_text(block: &ChapterBlock) -> String {
    let mut output = String::new();

    for inline in block.inlines() {
        match inline {
            Inline::Text(text) => output.push_str(text.text()),
            Inline::Image(image) => {
                if let Some(alt) = image.alt() {
                    output.push_str(alt);
                }
            }
            Inline::Break => output.push('\n'),
            Inline::Anchor(_) => {}
        }
    }

    output
}

#[test]
fn xhtml_normalizes_blocks_text_styles_links_and_anchors() {
    const XHTML: &str = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
    <head>
        <title>This must not become chapter text</title>
    </head>

    <body>
        <h1 id="chapter">
            Chapter <em>One</em>
        </h1>

        <p id="intro">
            Hello
            <strong>
                bold
                <em>and italic</em>
            </strong>
            world<br />
            next
            <a href="../Notes/notes.xhtml#n1">
                note
            </a>.
        </p>

        <div id="loose">
            Loose <b>text</b>
        </div>

        <script>
            ignored script text
        </script>

        <style>
            ignored style text
        </style>
    </body>
</html>
"#;

    let chapter = parse_xhtml(XHTML, ArchivePath::new("OPS/Text/chapter.xhtml").unwrap()).unwrap();

    assert_eq!(chapter.path().as_str(), "OPS/Text/chapter.xhtml");
    assert_eq!(chapter.blocks().len(), 3);

    let heading = &chapter.blocks()[0];

    assert_eq!(heading.kind(), BlockKind::Heading(1));
    assert_eq!(visible_text(heading), "Chapter One");
    assert_eq!(
        heading.inlines()[0],
        Inline::Anchor(String::from("chapter")),
    );

    let heading_text = heading
        .inlines()
        .iter()
        .filter_map(|inline| match inline {
            Inline::Text(text) => Some(text),

            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(heading_text.len(), 2);
    assert_eq!(heading_text[0].text(), "Chapter ");
    assert!(!heading_text[0].style().italic());
    assert_eq!(heading_text[1].text(), "One");
    assert!(heading_text[1].style().italic());

    let paragraph = &chapter.blocks()[1];

    assert_eq!(paragraph.kind(), BlockKind::Paragraph);
    assert_eq!(
        visible_text(paragraph),
        "Hello bold and italic world\nnext note.",
    );
    assert_eq!(
        paragraph.inlines()[0],
        Inline::Anchor(String::from("intro")),
    );

    let bold = paragraph
        .inlines()
        .iter()
        .find_map(|inline| {
            let Inline::Text(text) = inline else {
                return None;
            };

            if text.text() == "bold " {
                Some(text)
            } else {
                None
            }
        })
        .unwrap();

    assert!(bold.style().bold());
    assert!(!bold.style().italic());

    let bold_italic = paragraph
        .inlines()
        .iter()
        .find_map(|inline| {
            let Inline::Text(text) = inline else {
                return None;
            };

            if text.text() == "and italic" {
                Some(text)
            } else {
                None
            }
        })
        .unwrap();

    assert!(bold_italic.style().bold());
    assert!(bold_italic.style().italic());

    let link = paragraph
        .inlines()
        .iter()
        .find_map(|inline| {
            let Inline::Text(text) = inline else {
                return None;
            };

            if text.text() == "note" {
                Some(text)
            } else {
                None
            }
        })
        .unwrap();

    let target = link.link().unwrap();

    assert_eq!(target.path().unwrap().as_str(), "OPS/Notes/notes.xhtml");
    assert_eq!(target.fragment(), Some("n1"));

    let loose = &chapter.blocks()[2];

    assert_eq!(loose.kind(), BlockKind::Paragraph);
    assert_eq!(loose.inlines()[0], Inline::Anchor(String::from("loose")));
    assert_eq!(visible_text(loose), "Loose text");
}

#[test]
fn xhtml_preserves_named_anchors_and_external_links() {
    const XHTML: &str = r#"
<html xmlns="http://www.w3.org/1999/xhtml">
    <body>
        <p>
            <a name="legacy-anchor"></a>
            See
            <a href="https://example.com/books">
                website
            </a>
        </p>
    </body>
</html>
"#;

    let chapter = parse_xhtml(XHTML, ArchivePath::new("OPS/Text/chapter.xhtml").unwrap()).unwrap();

    assert_eq!(chapter.blocks().len(), 1);

    let block = &chapter.blocks()[0];

    assert!(
        block
            .inlines()
            .iter()
            .any(|inline| { inline == &Inline::Anchor(String::from("legacy-anchor")) }),
    );

    let website = block
        .inlines()
        .iter()
        .find_map(|inline| {
            let Inline::Text(text) = inline else {
                return None;
            };

            if text.text() == "website" {
                Some(text)
            } else {
                None
            }
        })
        .unwrap();

    assert_eq!(
        website.link().unwrap().external(),
        Some("https://example.com/books"),
    );
}

#[test]
fn xhtml_creates_list_item_blocks() {
    const XHTML: &str = r#"
<html xmlns="http://www.w3.org/1999/xhtml">
    <body>
        <ul>
            <li>First</li>
            <li>Second</li>
        </ul>
    </body>
</html>
"#;

    let chapter = parse_xhtml(XHTML, ArchivePath::new("chapter.xhtml").unwrap()).unwrap();

    assert_eq!(chapter.blocks().len(), 2);
    assert_eq!(chapter.blocks()[0].kind(), BlockKind::ListItem);
    assert_eq!(visible_text(&chapter.blocks()[0]), "First");
    assert_eq!(chapter.blocks()[1].kind(), BlockKind::ListItem);
    assert_eq!(visible_text(&chapter.blocks()[1]), "Second");
}

#[test]
fn xhtml_requires_a_body() {
    let error = parse_xhtml(
        "<html><head /></html>",
        ArchivePath::new("chapter.xhtml").unwrap(),
    )
    .unwrap_err();

    assert!(matches!(error, XhtmlError::MissingBody));
}

#[test]
fn xhtml_collapses_whitespace_without_inserting_space_before_punctuation() {
    const XHTML: &str = r##"
<html xmlns="http://www.w3.org/1999/xhtml">
    <body>
        <p>
            Read
            <a href="#note">
                this note
            </a>.
            Then <em>continue</em>.
        </p>
    </body>
</html>
"##;

    let chapter = parse_xhtml(XHTML, ArchivePath::new("chapter.xhtml").unwrap()).unwrap();

    assert_eq!(chapter.blocks().len(), 1);
    assert_eq!(
        visible_text(&chapter.blocks()[0]),
        "Read this note. Then continue.",
    );
}

#[test]
fn xhtml_preserves_stylesheets_and_element_style_context() {
    const XHTML: &str = r#"
<html xmlns="http://www.w3.org/1999/xhtml">
    <head>
        <link
            rel="stylesheet"
            href="../Styles/book.css"
        />

        <style>
            p.lead {
                text-align: center;
            }

            .accent {
                font-style: italic;
            }
        </style>
    </head>

    <body class="book">
        <p
            id="intro"
            class="lead centered"
            style="margin-top: 1em"
        >
            Hello
            <span
                class="accent"
                style="font-weight: bold"
            >world</span>
        </p>
    </body>
</html>
"#;

    let chapter = parse_xhtml(XHTML, ArchivePath::new("OPS/Text/chapter.xhtml").unwrap()).unwrap();

    assert_eq!(chapter.stylesheets().len(), 2);

    assert_eq!(
        chapter.stylesheets()[0].external_path().unwrap().as_str(),
        "OPS/Styles/book.css",
    );

    let embedded = chapter.stylesheets()[1].embedded_css().unwrap();

    assert!(embedded.contains("p.lead"));
    assert!(embedded.contains(".accent"));

    assert_eq!(chapter.blocks().len(), 1);

    let block = &chapter.blocks()[0];
    let paragraph = chapter.style_node(block.style_node()).unwrap();

    assert_eq!(paragraph.element(), "p");
    assert_eq!(paragraph.id(), Some("intro"));
    assert!(paragraph.has_class("lead"));
    assert!(paragraph.has_class("centered"));
    assert_eq!(paragraph.inline_style(), Some("margin-top: 1em"));

    let body = chapter
        .style_node(
            paragraph
                .parent()
                .expect("paragraph should have the body as its parent"),
        )
        .unwrap();

    assert_eq!(body.element(), "body");
    assert!(body.has_class("book"));

    let world = block
        .inlines()
        .iter()
        .find_map(|inline| {
            let Inline::Text(text) = inline else {
                return None;
            };

            if text.text() == "world" {
                Some(text)
            } else {
                None
            }
        })
        .unwrap();

    let span = chapter.style_node(world.style_node()).unwrap();

    assert_eq!(span.element(), "span");
    assert!(span.has_class("accent"));
    assert_eq!(span.inline_style(), Some("font-weight: bold"));
    assert_eq!(span.parent(), Some(block.style_node()));
}

#[test]
fn xhtml_preserves_lazy_images_links_alt_text_and_style_context() {
    const XHTML: &str = r##"
<html xmlns="http://www.w3.org/1999/xhtml">
    <body>
        <p>
            Before
            <a href="#full-size">
                <img
                    id="cover"
                    class="cover"
                    src="../Images/cover.jpg"
                    alt="Cover art"
                />
            </a>
            after
        </p>

        <p>
            <img
                src="https://example.com/remote.jpg"
                alt="Remote image"
            />
        </p>
    </body>
</html>
"##;

    let chapter = parse_xhtml(XHTML, ArchivePath::new("OPS/Text/chapter.xhtml").unwrap()).unwrap();

    assert_eq!(chapter.blocks().len(), 2);

    let image = chapter.blocks()[0]
        .inlines()
        .iter()
        .find_map(|inline| {
            let Inline::Image(image) = inline else {
                return None;
            };

            Some(image)
        })
        .unwrap();

    assert_eq!(image.path().as_str(), "OPS/Images/cover.jpg");
    assert_eq!(image.alt(), Some("Cover art"));

    let link = image.link().unwrap();

    assert_eq!(link.path().unwrap().as_str(), "OPS/Text/chapter.xhtml");
    assert_eq!(link.fragment(), Some("full-size"));

    let image_node = chapter.style_node(image.style_node()).unwrap();

    assert_eq!(image_node.element(), "img");
    assert_eq!(image_node.id(), Some("cover"));
    assert!(image_node.has_class("cover"));

    let parent = chapter.style_node(image_node.parent().unwrap()).unwrap();

    assert_eq!(parent.element(), "a");
    assert_eq!(visible_text(&chapter.blocks()[1]), "Remote image");
    assert!(
        chapter.blocks()[1]
            .inlines()
            .iter()
            .all(|inline| { !matches!(inline, Inline::Image(_)) }),
    );
}

#[test]
fn xhtml_content_offsets_survive_markup_images_and_breaks() {
    const XHTML: &str = r#"
<html xmlns="http://www.w3.org/1999/xhtml"><body><p>é🙂<a id="middle"></a><strong>漢</strong><br/><img src="../Images/cover.jpg" alt="Cover"/>z</p></body></html>
"#;

    let chapter = parse_xhtml(XHTML, ArchivePath::new("OPS/Text/chapter.xhtml").unwrap()).unwrap();

    assert_eq!(chapter.anchor_offset("middle"), Some(ContentOffset::new(2)));
    assert_eq!(chapter.content_len(), ContentOffset::new(4));
    assert!(chapter.contains_offset(ContentOffset::ZERO));
    assert!(chapter.contains_offset(ContentOffset::new(4)));
    assert!(!chapter.contains_offset(ContentOffset::new(5)));
}
