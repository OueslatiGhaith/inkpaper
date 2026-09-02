use alloc::{string::String, vec::Vec};

use crate::StyleNode;

use super::{FontStyle, FontWeight, TextAlign};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DisplayValue {
    Visible,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Property {
    FontWeight(FontWeight),
    FontStyle(FontStyle),
    TextAlign(TextAlign),
    Display(DisplayValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Declaration {
    pub(super) property: Property,
    pub(super) important: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Specificity {
    ids: u16,
    classes: u16,
    elements: u16,
}

impl Specificity {
    pub(super) const INLINE: Self = Self {
        ids: u16::MAX,
        classes: u16::MAX,
        elements: u16::MAX,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Selector {
    element: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    specificity: Specificity,
}

impl Selector {
    fn matches(&self, node: &StyleNode) -> bool {
        if let Some(element) = &self.element
            && !element.eq_ignore_ascii_case(node.element())
        {
            return false;
        }

        if let Some(id) = &self.id
            && node.id() != Some(id.as_str())
        {
            return false;
        }

        self.classes.iter().all(|class| node.has_class(class))
    }
}

#[derive(Debug, Clone)]
pub(super) struct Rule {
    selector: Selector,
    pub(super) declarations: Vec<Declaration>,
    pub(super) order: usize,
}

impl Rule {
    pub(super) fn matches(&self, node: &StyleNode) -> bool {
        self.selector.matches(node)
    }

    pub(super) const fn specificity(&self) -> Specificity {
        self.selector.specificity
    }
}

pub(super) fn parse_stylesheet(css: &str, rules: &mut Vec<Rule>, next_order: &mut usize) {
    let css = strip_comments(css);

    let mut cursor = 0;

    while cursor < css.len() {
        let Some(relative_open) = css[cursor..].find('{') else {
            break;
        };

        let open = cursor + relative_open;

        let Some(close) = find_matching_brace(&css, open) else {
            break;
        };

        let prelude = css[cursor..open].trim();

        // this also strips top-level statement at-rules such as:
        // @charset "UTF-8";
        // @import "...";
        let selector_text = prelude.rsplit(';').next().unwrap_or("").trim();

        if !selector_text.is_empty() && !selector_text.starts_with('@') {
            let declarations = parse_declarations(&css[open + 1..close]);

            if !declarations.is_empty() {
                let order = *next_order;
                *next_order = next_order.saturating_add(1);

                for raw_selector in selector_text.split(',') {
                    let Some(selector) = parse_selector(raw_selector) else {
                        continue;
                    };

                    rules.push(Rule {
                        selector,
                        declarations: declarations.clone(),
                        order,
                    });
                }
            }
        }

        cursor = close.saturating_add(1);
    }
}

pub(super) fn parse_inline_declarations(css: &str) -> Vec<Declaration> {
    parse_declarations(&strip_comments(css))
}

fn parse_selector(selector: &str) -> Option<Selector> {
    let selector = selector.trim();

    if selector.is_empty() || selector.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return None;
    }

    let bytes = selector.as_bytes();

    let mut index = 0;
    let mut element = None;
    let mut id = None;
    let mut classes = Vec::new();

    let mut ids = 0u16;
    let mut class_count = 0u16;
    let mut elements = 0u16;

    if bytes[index] == b'*' {
        index += 1;
    } else if is_identifier_start(bytes[index]) {
        let (name, end) = parse_identifier(selector, index)?;

        element = Some(name);
        elements = 1;
        index = end;
    } else if !matches!(bytes[index], b'.' | b'#') {
        return None;
    }

    while index < bytes.len() {
        match bytes[index] {
            b'.' => {
                index += 1;

                let (class, end) = parse_identifier(selector, index)?;

                classes.push(class);

                class_count = class_count.saturating_add(1);
                index = end;
            }

            b'#' => {
                if id.is_some() {
                    return None;
                }

                index += 1;

                let (identifier, end) = parse_identifier(selector, index)?;

                id = Some(identifier);

                ids = ids.saturating_add(1);
                index = end;
            }

            _ => {
                // combinators, pseudo classes/elements, attribute selectors, namespaces
                // and escaped selectors are not supported yet
                return None;
            }
        }
    }

    Some(Selector {
        element,
        id,
        classes,
        specificity: Specificity {
            ids,
            classes: class_count,
            elements,
        },
    })
}

fn parse_identifier(input: &str, start: usize) -> Option<(String, usize)> {
    let bytes = input.as_bytes();

    if start >= bytes.len() || !is_identifier_start(bytes[start]) {
        return None;
    }

    let mut end = start + 1;

    while end < bytes.len() && is_identifier_continue(bytes[end]) {
        end += 1;
    }

    Some((String::from(&input[start..end]), end))
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'-') || byte >= 0x80
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn parse_declarations(css: &str) -> Vec<Declaration> {
    let mut declarations = Vec::new();

    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut parentheses = 0usize;

    for (index, character) in css.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        if character == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }

        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            }

            continue;
        }

        match character {
            '"' | '\'' => {
                quote = Some(character);
            }

            '(' => {
                parentheses = parentheses.saturating_add(1);
            }

            ')' => {
                parentheses = parentheses.saturating_sub(1);
            }

            ';' if parentheses == 0 => {
                parse_declaration(&css[start..index], &mut declarations);

                start = index + character.len_utf8();
            }

            _ => {}
        }
    }

    parse_declaration(&css[start..], &mut declarations);

    declarations
}

fn parse_declaration(declaration: &str, output: &mut Vec<Declaration>) {
    let Some((name, value)) = declaration.split_once(':') else {
        return;
    };

    let name = name.trim();

    if name.is_empty() {
        return;
    }

    let (value, important) = strip_important(value);

    let property = if name.eq_ignore_ascii_case("font-weight") {
        parse_font_weight(value).map(Property::FontWeight)
    } else if name.eq_ignore_ascii_case("font-style") {
        parse_font_style(value).map(Property::FontStyle)
    } else if name.eq_ignore_ascii_case("text-align") {
        parse_text_align(value).map(Property::TextAlign)
    } else if name.eq_ignore_ascii_case("display") {
        parse_display(value).map(Property::Display)
    } else {
        None
    };

    let Some(property) = property else {
        return;
    };

    output.push(Declaration {
        property,
        important,
    });
}

fn strip_important(value: &str) -> (&str, bool) {
    let value = value.trim();
    const IMPORTANT: &str = "!important";

    if value.len() >= IMPORTANT.len() {
        let start = value.len() - IMPORTANT.len();

        if value[start..].eq_ignore_ascii_case(IMPORTANT) {
            return (value[..start].trim_end(), true);
        }
    }

    (value, false)
}

fn parse_font_weight(value: &str) -> Option<FontWeight> {
    if value.eq_ignore_ascii_case("normal") || value.eq_ignore_ascii_case("initial") {
        return Some(FontWeight::Normal);
    }

    if value.eq_ignore_ascii_case("bold") {
        return Some(FontWeight::Bold);
    }

    let numeric = value.parse::<u16>().ok()?;

    if !(100..=900).contains(&numeric) || numeric % 100 != 0 {
        return None;
    }

    Some(if numeric >= 600 {
        FontWeight::Bold
    } else {
        FontWeight::Normal
    })
}

fn parse_font_style(value: &str) -> Option<FontStyle> {
    if value.eq_ignore_ascii_case("normal") || value.eq_ignore_ascii_case("initial") {
        return Some(FontStyle::Normal);
    }

    if value.eq_ignore_ascii_case("italic") || starts_with_ignore_ascii_case(value, "oblique") {
        return Some(FontStyle::Italic);
    }

    None
}

fn starts_with_ignore_ascii_case(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
}

fn parse_text_align(value: &str) -> Option<TextAlign> {
    if value.eq_ignore_ascii_case("start") || value.eq_ignore_ascii_case("initial") {
        Some(TextAlign::Start)
    } else if value.eq_ignore_ascii_case("end") {
        Some(TextAlign::End)
    } else if value.eq_ignore_ascii_case("left") {
        Some(TextAlign::Left)
    } else if value.eq_ignore_ascii_case("right") {
        Some(TextAlign::Right)
    } else if value.eq_ignore_ascii_case("center") {
        Some(TextAlign::Center)
    } else if value.eq_ignore_ascii_case("justify") {
        Some(TextAlign::Justify)
    } else {
        None
    }
}

fn parse_display(value: &str) -> Option<DisplayValue> {
    if value.eq_ignore_ascii_case("none") {
        return Some(DisplayValue::None);
    }

    if [
        "block",
        "inline",
        "inline-block",
        "list-item",
        "table",
        "table-row",
        "table-cell",
        "contents",
        "flex",
        "grid",
        "initial",
    ]
    .iter()
    .any(|candidate| value.eq_ignore_ascii_case(candidate))
    {
        return Some(DisplayValue::Visible);
    }

    None
}

fn strip_comments(css: &str) -> String {
    let mut output = String::with_capacity(css.len());

    let mut remaining = css;

    loop {
        let Some(start) = remaining.find("/*") else {
            output.push_str(remaining);
            break;
        };

        output.push_str(&remaining[..start]);

        let after_start = &remaining[start + 2..];

        let Some(end) = after_start.find("*/") else {
            break;
        };

        remaining = &after_start[end + 2..];
    }

    output
}

fn find_matching_brace(css: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;

    for (relative_index, character) in css[open..].char_indices() {
        let index = open + relative_index;

        if escaped {
            escaped = false;
            continue;
        }

        if character == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }

        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            }

            continue;
        }

        match character {
            '"' | '\'' => {
                quote = Some(character);
            }

            '{' => {
                depth = depth.saturating_add(1);
            }

            '}' => {
                depth = depth.saturating_sub(1);

                if depth == 0 {
                    return Some(index);
                }
            }

            _ => {}
        }
    }

    None
}
