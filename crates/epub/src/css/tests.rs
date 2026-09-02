use crate::{ArchivePath, Inline, StylesheetSource, xhtml::parse_xhtml};

use super::*;

fn resolve_embedded(chapter: &Chapter) -> ChapterStyles {
    let mut stylesheet = Stylesheet::default();

    for source in chapter.stylesheets() {
        if let StylesheetSource::Embedded(css) = source {
            stylesheet.push(css);
        }
    }

    resolve_chapter_styles(chapter, &stylesheet)
}

fn text_style(chapter: &Chapter, styles: &ChapterStyles, text: &str) -> ComputedStyle {
    let run = chapter
        .blocks()
        .iter()
        .flat_map(|block| block.inlines())
        .find_map(|inline| {
            let Inline::Text(run) = inline else {
                return None;
            };

            if run.text() == text { Some(run) } else { None }
        })
        .unwrap();

    styles.style(run.style_node()).unwrap()
}

#[test]
fn css_resolves_specificity_inheritance_inline_style_and_important() {
    const XHTML: &str = r#"
<html xmlns="http://www.w3.org/1999/xhtml">
    <head>
        <style>
            p {
                font-weight: normal;
                text-align: center;
            }

            .lead {
                font-weight: normal;
            }

            p.lead {
                font-style: italic;
            }

            #intro {
                font-weight: normal;
                text-align: right;
            }

            .lead {
                font-weight: bold !important;
            }

            span.accent {
                font-style: italic;
            }
        </style>
    </head>

    <body>
        <p
            id="intro"
            class="lead"
        >
            Hello
            <span
                class="accent"
                style="font-style: normal; font-weight: normal"
            >world</span>
            <strong>strong</strong>
        </p>
    </body>
</html>
"#;

    let chapter = parse_xhtml(XHTML, ArchivePath::new("OPS/Text/chapter.xhtml").unwrap()).unwrap();

    let styles = resolve_embedded(&chapter);

    let paragraph = styles.style(chapter.blocks()[0].style_node()).unwrap();

    // .lead !important beats the normal #intro declaration.
    assert_eq!(paragraph.font_weight(), FontWeight::Bold);
    // p.lead beats inherited/default style.
    assert_eq!(paragraph.font_style(), FontStyle::Italic);
    // #intro beats p.
    assert_eq!(paragraph.text_align(), TextAlign::Right);

    let world = text_style(&chapter, &styles, "world");

    // inline declarations beat ordinary author rules and inherited values.
    assert_eq!(world.font_weight(), FontWeight::Normal);
    assert_eq!(world.font_style(), FontStyle::Normal);
    assert_eq!(world.text_align(), TextAlign::Right);

    let strong = text_style(&chapter, &styles, "strong");

    assert_eq!(strong.font_weight(), FontWeight::Bold);
    assert_eq!(strong.font_style(), FontStyle::Italic);
    assert_eq!(strong.text_align(), TextAlign::Right);
}

#[test]
fn css_display_none_hides_descendants() {
    const XHTML: &str = r#"
<html xmlns="http://www.w3.org/1999/xhtml">
    <head>
        <style>
            .hidden {
                display: none;
            }
        </style>
    </head>

    <body>
        <div class="hidden">
            <p>secret</p>
        </div>

        <p>visible</p>
    </body>
</html>
"#;

    let chapter = parse_xhtml(XHTML, ArchivePath::new("chapter.xhtml").unwrap()).unwrap();

    let styles = resolve_embedded(&chapter);

    assert!(text_style(&chapter, &styles, "secret").hidden());
    assert!(!text_style(&chapter, &styles, "visible").hidden());
}

#[test]
fn css_later_equal_specificity_wins() {
    const XHTML: &str = r#"
<html xmlns="http://www.w3.org/1999/xhtml">
    <head>
        <style>
            .lead {
                text-align: left;
            }

            .lead {
                text-align: justify;
            }
        </style>
    </head>

    <body>
        <p class="lead">text</p>
    </body>
</html>
"#;

    let chapter = parse_xhtml(XHTML, ArchivePath::new("chapter.xhtml").unwrap()).unwrap();

    let styles = resolve_embedded(&chapter);

    assert_eq!(
        text_style(&chapter, &styles, "text").text_align(),
        TextAlign::Justify,
    );
}
