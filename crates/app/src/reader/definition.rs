//! A dictionary definition laid out in pages like a book's chapter. Markup
//! keeps its bold, italics and lists; the rest is kept for its text, built
//! directly into chapter blocks without round-tripping through XHTML.

use alloc::{borrow::Cow, format, string::String, vec::Vec};

use inkpaper_dictionary::{Definition, FieldKind};
use inkpaper_epub::{
    ArchivePath, BlockKind, Chapter, ChapterBuilder, ChapterStyles, FontStyle, FontWeight,
    SpineIndex, decode_xml_text,
};
use inkpaper_reader::{Page, Viewport, paginate_chapter};

use super::{
    ParagraphAlignment, TextSettings,
    measurer::{ReaderFonts, ReaderMeasurer},
};
use crate::fonts::{CardFont, card_for};

/// The area of the definition card a page of a definition fills.
pub(crate) const DEFINITION_WIDTH: u32 = 436;
pub(crate) const DEFINITION_HEIGHT: u32 = 260;

/// What an item of a list inside another's item starts with, a level in:
/// no-break spaces, as a paragraph's own spaces collapse.
const NESTED_INDENT: &str = "\u{a0}\u{a0}\u{a0}\u{a0}";

/// `definition`'s pages, in the reader's font, size and line spacing, its
/// paragraphs flush left. `card` is the card font, when it's loaded.
pub(crate) fn layout_definition(
    definition: &Definition,
    text: TextSettings,
    card: Option<&CardFont>,
) -> Vec<Page<'static>> {
    let text = text
        .with_alignment(ParagraphAlignment::Left)
        .with_paragraph_spacing(true);
    let text = text.with_paragraph_indent(0).unwrap_or(text);

    paginate(definition, text, card).unwrap_or_default()
}

fn paginate(
    definition: &Definition,
    text: TextSettings,
    card: Option<&CardFont>,
) -> Option<Vec<Page<'static>>> {
    let path = ArchivePath::new("definition.xhtml").ok()?;
    let (chapter, styles) = build_definition_chapter(definition, path);

    let fonts = ReaderFonts::new(card_for(text.font(), card)).ok()?;
    let mut measurer = ReaderMeasurer::new(&fonts, Default::default());

    let pagination = paginate_chapter(
        &chapter,
        &styles,
        SpineIndex::ZERO,
        Viewport::new(DEFINITION_WIDTH, DEFINITION_HEIGHT)?,
        text.reader_settings(None),
        &mut measurer,
    )
    .ok()?;

    Some(pagination.into_owned().pages().to_vec())
}

/// Builds a chapter and its styles directly from `definition`'s fields:
/// markup keeps bold, italics, headings, and lists; pronunciations are
/// italicized.
fn build_definition_chapter(
    definition: &Definition,
    path: ArchivePath,
) -> (Chapter, ChapterStyles) {
    let mut body = Body::new(path);

    for field in definition.fields() {
        match field.kind() {
            FieldKind::Plain => body.plain(field.text()),
            FieldKind::Phonetic => {
                body.open(InlineStyle::Italic);
                body.plain(field.text());
            }
            // in HTML, a newline is a space like any other
            FieldKind::Html => body.markup(field.text(), false),
            _ => body.markup(field.text(), true),
        }

        body.end_field();
    }

    body.finish()
}

/// The bold and italics a definition's text can keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineStyle {
    Bold,
    Italic,
}

/// What a tag of a definition's markup does to its text.
enum Tag {
    Inline(InlineStyle),
    /// a line of its own, like a paragraph
    Block,
    /// a line of its own in bold, like a heading
    Heading,
    /// a list, numbered or not
    List { numbered: bool },
    Item,
    /// a line ends there
    Break,
    /// its text isn't part of the definition
    Skipped,
    /// kept for its text only
    Other,
}

impl Tag {
    fn from_name(name: &str) -> Self {
        match name {
            "b" | "strong" => Self::Inline(InlineStyle::Bold),
            "i" | "em" | "cite" | "dfn" | "var" => Self::Inline(InlineStyle::Italic),
            "p" | "div" | "blockquote" | "dl" | "dd" | "table" | "tr" | "hr" | "section"
            | "pre" => Self::Block,
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "dt" => Self::Heading,
            "ul" => Self::List { numbered: false },
            "ol" => Self::List { numbered: true },
            "li" => Self::Item,
            "br" => Self::Break,
            "script" | "style" | "head" => Self::Skipped,
            _ => Self::Other,
        }
    }
}

/// Builds a definition's chapter blocks directly from markup that needn't be
/// well-formed, tracking open bold/italic nesting and lists in place.
struct Body {
    builder: ChapterBuilder,
    /// whether a paragraph has received non-whitespace content
    paragraph: bool,
    /// nesting depth of bold tags
    bold: usize,
    /// nesting depth of italic tags
    italic: usize,
    /// the lists open, innermost last, with the last number of a numbered one
    lists: Vec<Option<u32>>,
    /// a list item's bullet or number, written before its first text
    marker: Option<String>,
}

impl Body {
    const fn new(path: ArchivePath) -> Self {
        Self {
            builder: Chapter::builder(path),
            paragraph: false,
            bold: 0,
            italic: 0,
            lists: Vec::new(),
            marker: None,
        }
    }

    fn finish(self) -> (Chapter, ChapterStyles) {
        self.builder.finish()
    }

    /// Plain text, a paragraph a line.
    fn plain(&mut self, text: &str) {
        self.lines(text, false, true);
    }

    /// Markup's text and tags. Newlines end lines when `newlines`, like in
    /// Pango and XDXF markup.
    fn markup(&mut self, markup: &str, newlines: bool) {
        let mut rest = markup;

        while let Some(start) = rest.find('<') {
            self.lines(&rest[..start], true, newlines);

            let after_lt = &rest[start + 1..];

            // A bare `<` not starting a tag (e.g. `x < 5`) or without a
            // closing `>` is literal text rather than markup.
            if !starts_tag(after_lt) {
                self.text("<", false);
                rest = after_lt;
                continue;
            }

            let Some(end) = after_lt.find('>') else {
                self.lines(&rest[start..], true, newlines);
                return;
            };

            let tag = &after_lt[..end];
            rest = &after_lt[end + 1..];

            let closing = tag.starts_with('/');
            let self_closing = tag.ends_with('/');
            let name = tag
                .trim_start_matches('/')
                .split(|character: char| character.is_whitespace() || character == '/')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();

            match Tag::from_name(&name) {
                // a comment, a doctype, a processing instruction, or an empty tag
                _ if name.is_empty() || name.starts_with('!') || name.starts_with('?') => {}
                Tag::Skipped if !closing && !self_closing => {
                    rest = skip_element(rest, &name);
                }
                Tag::Break => self.end_paragraph(),
                _ if self_closing => {}
                Tag::Inline(inline) if closing => self.close(inline),
                Tag::Inline(inline) => self.open(inline),
                Tag::Block => self.end_paragraph(),
                Tag::Heading => {
                    self.end_paragraph();

                    if closing {
                        self.close(InlineStyle::Bold);
                    } else {
                        self.open(InlineStyle::Bold);
                    }
                }
                Tag::List { numbered } => {
                    self.end_paragraph();

                    if closing {
                        self.lists.pop();
                    } else {
                        self.lists.push(numbered.then_some(0));
                    }
                }
                Tag::Item => {
                    self.end_paragraph();

                    if !closing {
                        // a list in another's item is indented under it
                        let indent = NESTED_INDENT.repeat(self.lists.len().saturating_sub(1));

                        self.marker = Some(match self.lists.last_mut() {
                            Some(Some(number)) => {
                                *number += 1;
                                format!("{indent}{number}. ")
                            }
                            _ => format!("{indent}\u{2022} "),
                        });
                    }
                }
                Tag::Skipped | Tag::Other => {}
            }
        }

        self.lines(rest, true, newlines);
    }

    /// `text`, decoding entities when `decode_entities`, with newlines ending
    /// paragraphs when `newlines`.
    fn lines(&mut self, text: &str, decode_entities: bool, newlines: bool) {
        if !newlines {
            self.text(text, decode_entities);
            return;
        }

        for (index, line) in text.split('\n').enumerate() {
            if index > 0 {
                self.end_paragraph();
            }

            self.text(line, decode_entities);
        }
    }

    fn text(&mut self, text: &str, decode_entities: bool) {
        if text.is_empty() {
            return;
        }

        let decoded = if decode_entities {
            decode_xml_text(text)
        } else {
            Cow::Borrowed(text)
        };

        // Collapsible whitespace alone doesn't start a paragraph, though
        // non-breaking spaces (e.g. `&nbsp;`) are kept as real text.
        if is_only_collapsible_whitespace(&decoded) {
            if self.paragraph {
                let (weight, style) = self.current_font();
                self.builder.push_text(" ", weight, style);
            }

            return;
        }

        if !self.paragraph {
            self.paragraph = true;
            self.builder.start_block(BlockKind::Paragraph);

            if let Some(marker) = self.marker.take() {
                self.builder
                    .push_text(&marker, FontWeight::Normal, FontStyle::Normal);
            }
        }

        let (weight, style) = self.current_font();
        self.builder.push_text(&decoded, weight, style);
    }

    const fn current_font(&self) -> (FontWeight, FontStyle) {
        let weight = if self.bold > 0 {
            FontWeight::Bold
        } else {
            FontWeight::Normal
        };
        let style = if self.italic > 0 {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        };

        (weight, style)
    }

    fn open(&mut self, inline: InlineStyle) {
        match inline {
            InlineStyle::Bold => self.bold += 1,
            InlineStyle::Italic => self.italic += 1,
        }
    }

    fn close(&mut self, inline: InlineStyle) {
        match inline {
            InlineStyle::Bold => self.bold = self.bold.saturating_sub(1),
            InlineStyle::Italic => self.italic = self.italic.saturating_sub(1),
        }
    }

    fn end_paragraph(&mut self) {
        if !self.paragraph {
            return;
        }

        self.builder.end_block();
        self.paragraph = false;
    }

    /// Ends a field, and whatever its markup left open.
    fn end_field(&mut self) {
        self.end_paragraph();
        self.bold = 0;
        self.italic = 0;
        self.lists.clear();
        self.marker = None;
    }
}

/// Whether the text immediately after `<` can begin a markup tag, comment,
/// doctype, or processing instruction.
fn starts_tag(after_lt: &str) -> bool {
    matches!(
        after_lt.as_bytes().first(),
        Some(b'a'..=b'z' | b'A'..=b'Z' | b'/' | b'!' | b'?')
    )
}

fn is_only_collapsible_whitespace(text: &str) -> bool {
    text.chars()
        .all(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{FEFF}'))
}

/// `rest` after the end of the element named `name` it's inside, or nothing
/// when it doesn't end.
fn skip_element<'a>(rest: &'a str, name: &str) -> &'a str {
    let bytes = rest.as_bytes();
    let name_bytes = name.as_bytes();

    for start in 0..bytes.len() {
        if bytes[start] == b'<'
            && bytes.get(start + 1) == Some(&b'/')
            && bytes
                .get(start + 2..start + 2 + name_bytes.len())
                .is_some_and(|slice| slice.eq_ignore_ascii_case(name_bytes))
            && bytes
                .get(start + 2 + name_bytes.len())
                .is_some_and(|&next| next == b'>' || next.is_ascii_whitespace())
            && let Some(end) = rest[start..].find('>')
        {
            return &rest[start + end + 1..];
        }
    }

    ""
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use inkpaper_epub::Inline;

    fn styled_paragraphs(build: impl FnOnce(&mut Body)) -> Vec<Vec<(String, FontWeight, FontStyle)>> {
        let path = ArchivePath::new("definition.xhtml").unwrap();
        let mut body = Body::new(path);
        build(&mut body);
        let (chapter, styles) = body.finish();

        chapter
            .blocks()
            .iter()
            .map(|block| {
                block
                    .inlines()
                    .iter()
                    .map(|inline| match inline {
                        Inline::Text(run) => {
                            let style = styles.style(run.style_node()).unwrap();
                            (String::from(run.text()), style.font_weight(), style.font_style())
                        }
                        _ => panic!("unexpected inline in definition"),
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn formats_markup_styles_headings_lists_and_skips_scripts() {
        let blocks = styled_paragraphs(|body| {
            body.open(InlineStyle::Italic);
            body.plain("liːd");
            body.end_field();

            body.markup(
                "<h3>Verb</h3><style>.x { color: red; }</style>\
                 <p>To <b>guide <i>or</i></b> direct.</p>\
                 <ol><li>First<ul><li>Nested</li></ul></li><li>Second</li></ol>\
                 Line one<br/>Line two",
                false,
            );
            body.end_field();
        });

        assert_eq!(
            blocks,
            vec![
                vec![(String::from("liːd"), FontWeight::Normal, FontStyle::Italic)],
                vec![(String::from("Verb"), FontWeight::Bold, FontStyle::Normal)],
                vec![
                    (String::from("To "), FontWeight::Normal, FontStyle::Normal),
                    (String::from("guide "), FontWeight::Bold, FontStyle::Normal),
                    (String::from("or"), FontWeight::Bold, FontStyle::Italic),
                    (
                        String::from(" direct."),
                        FontWeight::Normal,
                        FontStyle::Normal,
                    ),
                ],
                vec![(
                    String::from("1. First"),
                    FontWeight::Normal,
                    FontStyle::Normal,
                )],
                vec![(
                    format!("{NESTED_INDENT}\u{2022} Nested"),
                    FontWeight::Normal,
                    FontStyle::Normal,
                )],
                vec![(
                    String::from("2. Second"),
                    FontWeight::Normal,
                    FontStyle::Normal,
                )],
                vec![(
                    String::from("Line one"),
                    FontWeight::Normal,
                    FontStyle::Normal,
                )],
                vec![(
                    String::from("Line two"),
                    FontWeight::Normal,
                    FontStyle::Normal,
                )],
            ],
        );
    }

    #[test]
    fn decodes_entities_and_preserves_bare_ampersands_and_less_than() {
        let blocks = styled_paragraphs(|body| {
            body.markup(
                "Fish &amp; Chips &mdash; R&D &amp; x < 5 and y <z",
                false,
            );
            body.end_field();

            body.plain("plain &amp; literal <tag>\nsecond line");
            body.end_field();
        });

        assert_eq!(
            blocks,
            vec![
                vec![(
                    String::from("Fish & Chips — R&D & x < 5 and y <z"),
                    FontWeight::Normal,
                    FontStyle::Normal,
                )],
                vec![(
                    String::from("plain &amp; literal <tag>"),
                    FontWeight::Normal,
                    FontStyle::Normal,
                )],
                vec![(
                    String::from("second line"),
                    FontWeight::Normal,
                    FontStyle::Normal,
                )],
            ],
        );
    }

    #[test]
    fn keeps_nbsp_between_styled_runs() {
        let blocks = styled_paragraphs(|body| {
            body.markup("<b>one</b>&nbsp;<i>two</i>", false);
            body.end_field();
        });

        assert_eq!(
            blocks,
            vec![vec![
                (String::from("one"), FontWeight::Bold, FontStyle::Normal),
                (
                    String::from("\u{00a0}"),
                    FontWeight::Normal,
                    FontStyle::Normal,
                ),
                (String::from("two"), FontWeight::Normal, FontStyle::Italic),
            ]],
        );
    }
}
