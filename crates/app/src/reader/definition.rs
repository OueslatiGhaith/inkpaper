//! A dictionary definition laid out in pages like a book's chapter, the way
//! crosspoint lays out HTML definitions through its EPUB engine. Markup
//! keeps its bold, italics and lists; the rest is kept for its text.

use alloc::{format, string::String, vec::Vec};

use inkpaper_dictionary::{Definition, FieldKind};
use inkpaper_epub::{ArchivePath, Chapter, ChapterStyles, SpineIndex};
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

    // markup can leave text that isn't XHTML, like a bare `&`; escaped
    // whole, it always is
    [false, true]
        .into_iter()
        .find_map(|escape_all| paginate(&definition_xhtml(definition, escape_all), text, card))
        .unwrap_or_default()
}

fn paginate(
    xhtml: &str,
    text: TextSettings,
    card: Option<&CardFont>,
) -> Option<Vec<Page<'static>>> {
    let path = ArchivePath::new("definition.xhtml").ok()?;
    let chapter = Chapter::parse(xhtml, path).ok()?;
    let styles = ChapterStyles::defaults(&chapter);

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

/// A chapter of `definition`'s fields, their markup's bold, italics and
/// lists kept, pronunciations in italics. Markup keeps its entities unless
/// `escape_all`.
fn definition_xhtml(definition: &Definition, escape_all: bool) -> String {
    let mut body = Body::new(escape_all);

    for field in definition.fields() {
        match field.kind() {
            FieldKind::Plain => body.plain(field.text()),
            FieldKind::Phonetic => {
                body.open(Inline::Italic);
                body.plain(field.text());
            }
            // in HTML, a newline is a space like any other
            FieldKind::Html => body.markup(field.text(), false),
            _ => body.markup(field.text(), true),
        }

        body.end_field();
    }

    format!(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>{}</body></html>"#,
        body.xhtml
    )
}

/// The bold and italics a definition's text can keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Inline {
    Bold,
    Italic,
}

impl Inline {
    const fn name(self) -> &'static str {
        match self {
            Self::Bold => "b",
            Self::Italic => "i",
        }
    }
}

/// What a tag of a definition's markup does to its text.
enum Tag {
    Inline(Inline),
    /// a line of its own, like a paragraph
    Block,
    /// a line of its own in bold, like a heading
    Heading,
    /// a list, numbered or not
    List {
        numbered: bool,
    },
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
            "b" | "strong" => Self::Inline(Inline::Bold),
            "i" | "em" | "cite" | "dfn" | "var" => Self::Inline(Inline::Italic),
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

/// The XHTML body of a definition, built a paragraph at a time from markup
/// that needn't be well-formed: only paragraphs, bold, italics and line
/// breaks are written, each closed before its paragraph ends.
struct Body {
    xhtml: String,
    escape_all: bool,
    /// whether a paragraph is open
    paragraph: bool,
    /// the bold and italics open, innermost last, reopened in each paragraph
    inline: Vec<Inline>,
    /// the lists open, innermost last, with the last number of a numbered one
    lists: Vec<Option<u32>>,
    /// a list item's bullet or number, written before its first text
    marker: Option<String>,
}

impl Body {
    const fn new(escape_all: bool) -> Self {
        Self {
            xhtml: String::new(),
            escape_all,
            paragraph: false,
            inline: Vec::new(),
            lists: Vec::new(),
            marker: None,
        }
    }

    /// Plain text, a paragraph a line.
    fn plain(&mut self, text: &str) {
        self.lines(text, true, true);
    }

    /// Markup's text and tags. Newlines end lines when `newlines`, like in
    /// Pango and XDXF markup.
    fn markup(&mut self, markup: &str, newlines: bool) {
        let mut rest = markup;

        while let Some(start) = rest.find('<') {
            self.lines(&rest[..start], false, newlines);

            let Some(end) = rest[start..].find('>') else {
                return;
            };

            let tag = &rest[start + 1..start + end];
            rest = &rest[start + end + 1..];

            let closing = tag.starts_with('/');
            let self_closing = tag.ends_with('/');
            let name = tag
                .trim_start_matches('/')
                .split(|character: char| character.is_whitespace() || character == '/')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();

            match Tag::from_name(&name) {
                // a comment, a doctype, or a tag naming nothing
                _ if name.is_empty() || name.starts_with('!') => {}
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
                        self.close(Inline::Bold);
                    } else {
                        self.open(Inline::Bold);
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

        self.lines(rest, false, newlines);
    }

    /// `text`, escaped whole when `plain`, its newlines ending lines when
    /// `newlines`.
    fn lines(&mut self, text: &str, plain: bool, newlines: bool) {
        if !newlines {
            self.text(text, plain);
            return;
        }

        for (index, line) in text.split('\n').enumerate() {
            if index > 0 {
                self.end_paragraph();
            }

            self.text(line, plain);
        }
    }

    fn text(&mut self, text: &str, plain: bool) {
        // whitespace alone doesn't start a paragraph
        if text.trim().is_empty() {
            if self.paragraph && !text.is_empty() {
                self.xhtml.push(' ');
            }

            return;
        }

        if !self.paragraph {
            self.paragraph = true;
            self.xhtml.push_str("<p>");

            if let Some(marker) = self.marker.take() {
                self.xhtml.push_str(&marker);
            }

            for index in 0..self.inline.len() {
                self.start_tag(self.inline[index]);
            }
        }

        self.xhtml.push_str(&escape(text, plain || self.escape_all));
    }

    fn open(&mut self, inline: Inline) {
        self.inline.push(inline);

        if self.paragraph {
            self.start_tag(inline);
        }
    }

    /// Closes the innermost `inline` open, and with it the ones inside it,
    /// which open again after it. A tag closing nothing is left out.
    fn close(&mut self, inline: Inline) {
        let Some(index) = self.inline.iter().rposition(|open| *open == inline) else {
            return;
        };

        if self.paragraph {
            for open in self.inline[index..].iter().rev() {
                self.xhtml.push_str(&format!("</{}>", open.name()));
            }

            for open in &self.inline[index + 1..] {
                self.xhtml.push_str(&format!("<{}>", open.name()));
            }
        }

        self.inline.remove(index);
    }

    fn start_tag(&mut self, inline: Inline) {
        self.xhtml.push_str(&format!("<{}>", inline.name()));
    }

    fn end_paragraph(&mut self) {
        if !self.paragraph {
            return;
        }

        for open in self.inline.iter().rev() {
            self.xhtml.push_str(&format!("</{}>", open.name()));
        }

        self.xhtml.push_str("</p>");
        self.paragraph = false;
    }

    /// Ends a field, and whatever its markup left open.
    fn end_field(&mut self) {
        self.end_paragraph();
        self.inline.clear();
        self.lists.clear();
        self.marker = None;
    }
}

/// `rest` after the end of the element named `name` it's inside, or nothing
/// when it doesn't end.
fn skip_element<'a>(rest: &'a str, name: &str) -> &'a str {
    let lowercase = rest.to_ascii_lowercase();
    let closing = format!("</{name}");

    lowercase
        .find(&closing)
        .and_then(|start| rest[start..].find('>').map(|end| &rest[start + end + 1..]))
        .unwrap_or_default()
}

/// `text` safe in XHTML. Unless `ampersands`, the entities in it, like
/// `&nbsp;`, are kept, and only a bare `&` escaped.
fn escape(text: &str, ampersands: bool) -> String {
    let mut escaped = String::with_capacity(text.len());

    for (offset, character) in text.char_indices() {
        match character {
            '&' if ampersands || !starts_entity(&text[offset + 1..]) => {
                escaped.push_str("&amp;");
            }
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            character => escaped.push(character),
        }
    }

    escaped
}

/// Whether `rest`, after an `&`, is an entity's name or number and its `;`.
fn starts_entity(rest: &str) -> bool {
    let Some((name, _)) = rest.split_once(';') else {
        return false;
    };

    let number = name
        .strip_prefix("#x")
        .or_else(|| name.strip_prefix("#X"))
        .map(|hex| hex.chars().all(|digit| digit.is_ascii_hexdigit()))
        .or_else(|| {
            name.strip_prefix('#')
                .map(|decimal| decimal.chars().all(|digit| digit.is_ascii_digit()))
        });

    !name.is_empty()
        && name.len() <= 32
        && number.unwrap_or_else(|| {
            name.chars()
                .all(|character| character.is_ascii_alphanumeric())
        })
}
