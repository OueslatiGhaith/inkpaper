use crate::XhtmlError;

use super::*;

fn visible_text(block: &ChapterBlock) -> String {
    let mut output = String::new();

    for inline in block.inlines() {
        match inline {
            Inline::Text(text) => output.push_str(text.text()),
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
