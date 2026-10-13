//! A dictionary definition laid out in pages like a book's chapter, the way
//! crosspoint lays out HTML definitions through its EPUB engine. Markup is
//! stripped to its text for now.

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

/// Markup tags that end a line of a definition's text.
const LINE_TAGS: [&str; 7] = ["br", "p", "div", "li", "tr", "h1", "h2"];

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

/// A chapter of `definition`'s fields: a paragraph a line, pronunciations in
/// italics. Markup keeps its entities unless `escape_all`.
fn definition_xhtml(definition: &Definition, escape_all: bool) -> String {
    let mut body = String::new();

    for field in definition.fields() {
        let text = match field.kind() {
            FieldKind::Plain | FieldKind::Phonetic => escape(field.text(), true),
            _ => escape(&markup_text(field.text()), escape_all),
        };

        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            if field.kind() == FieldKind::Phonetic {
                body.push_str(&format!("<p><i>{line}</i></p>"));
            } else {
                body.push_str(&format!("<p>{line}</p>"));
            }
        }
    }

    format!(r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>{body}</body></html>"#)
}

/// The text of a field's markup, its tags dropped, the ones ending a line
/// turned into line breaks.
fn markup_text(markup: &str) -> String {
    let mut text = String::new();
    let mut rest = markup;

    while let Some(start) = rest.find('<') {
        text.push_str(&rest[..start]);

        let Some(end) = rest[start..].find('>') else {
            rest = "";
            break;
        };

        let tag = &rest[start + 1..start + end];
        let name = tag
            .trim_start_matches('/')
            .split(|character: char| character.is_whitespace() || character == '/')
            .next()
            .unwrap_or_default();

        if LINE_TAGS
            .iter()
            .any(|line_tag| name.eq_ignore_ascii_case(line_tag))
        {
            text.push('\n');
        }

        rest = &rest[start + end + 1..];
    }

    text.push_str(rest);

    text
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
