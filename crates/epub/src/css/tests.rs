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

#[test]
fn css_resolves_block_spacing_indentation_and_line_height() {
    const XHTML: &str = r#"
<html xmlns="http://www.w3.org/1999/xhtml">
    <head>
        <style>
            body {
                text-indent: 10%;
                line-height: 1.5;
            }

            p {
                margin: 2em auto 12px;
            }
        </style>
    </head>

    <body>
        <p><span>text</span></p>
    </body>
</html>
"#;

    let chapter = parse_xhtml(XHTML, ArchivePath::new("chapter.xhtml").unwrap()).unwrap();

    let styles = resolve_embedded(&chapter);

    let paragraph = styles.style(chapter.blocks()[0].style_node()).unwrap();

    assert_eq!(paragraph.margin_top().unwrap().resolve(20, 400), 40);

    assert_eq!(paragraph.margin_bottom().unwrap().resolve(20, 400), 12);

    // text-indent is inherited from body.
    assert_eq!(paragraph.text_indent().resolve(20, 400), 40);

    // line-height is inherited from body.
    assert_eq!(paragraph.line_height().resolve(20), Some(30));

    let text = text_style(&chapter, &styles, "text");

    // inherited properties continue through the span.
    assert_eq!(text.text_indent().resolve(20, 400), 40);

    assert_eq!(text.line_height().resolve(20), Some(30));

    // margins are not inherited by the span.
    assert_eq!(text.margin_top(), None);
    assert_eq!(text.margin_bottom(), None);
}

#[test]
fn css_matches_descendant_and_child_combinators() {
    const XHTML: &str = r#"
<html xmlns="http://www.w3.org/1999/xhtml">
    <head>
        <style>
            .book p { font-style: italic; }
            div > p { text-align: center; }
            .book  >  .note  em { font-weight: bold; }
        </style>
    </head>
    <body class="book">
        <p>direct</p>
        <div><p>child of div</p></div>
        <div><blockquote><p>grandchild of div</p></blockquote></div>
        <div class="note"><span><em>deep</em></span></div>
    </body>
</html>
"#;

    let chapter = parse_xhtml(XHTML, ArchivePath::new("chapter.xhtml").unwrap()).unwrap();
    let styles = resolve_embedded(&chapter);

    // descendant: every p under body.book
    assert_eq!(
        text_style(&chapter, &styles, "direct").font_style(),
        FontStyle::Italic
    );
    assert_eq!(
        text_style(&chapter, &styles, "grandchild of div").font_style(),
        FontStyle::Italic
    );

    // child: only the p directly inside div
    assert_eq!(
        text_style(&chapter, &styles, "child of div").text_align(),
        TextAlign::Center
    );
    assert_ne!(
        text_style(&chapter, &styles, "grandchild of div").text_align(),
        TextAlign::Center
    );

    // mixed chain with extra whitespace around the combinator
    assert_eq!(
        text_style(&chapter, &styles, "deep").font_weight(),
        FontWeight::Bold
    );
}

#[test]
fn css_combinator_specificity_adds_across_compounds() {
    const XHTML: &str = r#"
<html xmlns="http://www.w3.org/1999/xhtml">
    <head>
        <style>
            .book p { text-align: center; }
            p { text-align: right; }
        </style>
    </head>
    <body class="book"><p>text</p></body>
</html>
"#;

    let chapter = parse_xhtml(XHTML, ArchivePath::new("chapter.xhtml").unwrap()).unwrap();
    let styles = resolve_embedded(&chapter);

    // (0,1,1) beats (0,0,1) even though `p` comes later
    assert_eq!(
        text_style(&chapter, &styles, "text").text_align(),
        TextAlign::Center
    );
}

#[test]
fn css_ignores_unsupported_selectors_without_dropping_the_rest_of_the_list() {
    const XHTML: &str = r#"
<html xmlns="http://www.w3.org/1999/xhtml">
    <head>
        <style>
            h1 + p, p:first-child, h1 ~ p, p[lang], .book p { font-style: italic; }
            h1 + p { font-weight: bold; }
        </style>
    </head>
    <body class="book"><h1>title</h1><p>text</p></body>
</html>
"#;

    let chapter = parse_xhtml(XHTML, ArchivePath::new("chapter.xhtml").unwrap()).unwrap();
    let styles = resolve_embedded(&chapter);

    assert_eq!(
        text_style(&chapter, &styles, "text").font_style(),
        FontStyle::Italic
    );
    assert_ne!(
        text_style(&chapter, &styles, "text").font_weight(),
        FontWeight::Bold
    );
}
