use crate::{Pixels, TextMaxLines, TextOverflow, TextWrap, px};

fn next_scalar_boundary(text: &str, from: usize) -> Option<usize> {
    if from >= text.len() || !text.is_char_boundary(from) {
        return None;
    }

    let character = text[from..].chars().next()?;

    Some(from.saturating_add(character.len_utf8()))
}

fn next_text_boundary<B>(text: &str, from: usize, next_boundary: &B) -> Option<usize>
where
    B: Fn(&str, usize) -> Option<usize>,
{
    if from >= text.len() {
        return None;
    }

    let candidate = next_boundary(text, from);
    debug_assert!(
        candidate.is_some(),
        "text boundary provider must make progress before end of input",
    );

    let boundary = candidate.or_else(|| next_scalar_boundary(text, from))?;

    if boundary <= from || boundary > text.len() || !text.is_char_boundary(boundary) {
        debug_assert!(
            false,
            "text boundary provider returned an invalid UTF-8 boundary",
        );

        return next_scalar_boundary(text, from);
    }

    Some(boundary)
}

fn is_wrap_whitespace(character: char) -> bool {
    matches!(character, ' ' | '\t')
}

fn skip_wrap_whitespace(text: &str, mut offset: usize) -> usize {
    while offset < text.len() {
        let Some(character) = text[offset..].chars().next() else {
            break;
        };
        if !is_wrap_whitespace(character) {
            break;
        }

        offset += character.len_utf8();
    }

    offset
}

fn next_word(text: &str, from: usize) -> Option<(usize, usize)> {
    let start = skip_wrap_whitespace(text, from);
    if start >= text.len() {
        return None;
    }

    let mut end = text.len();
    for (relative, character) in text[start..].char_indices() {
        if is_wrap_whitespace(character) {
            end = start + relative;
            break;
        }
    }

    Some((start, end))
}

fn emit_oversized_word<'a, B, M, V>(
    word: &'a str,
    max_width: Pixels,
    next_boundary: &B,
    measure: &mut M,
    visit: &mut V,
) where
    B: Fn(&str, usize) -> Option<usize>,
    M: FnMut(&str) -> Pixels,
    V: FnMut(&'a str, Pixels),
{
    let mut start = 0usize;

    while start < word.len() {
        let first_end = next_text_boundary(word, start, next_boundary)
            .expect("non-empty word must contain a text boundary");

        let mut end = first_end;
        let mut last_fit = None;

        loop {
            let candidate = &word[start..end];
            let width = measure(candidate);

            if width <= max_width {
                last_fit = Some((end, width));
            } else {
                break;
            }
            if end == word.len() {
                break;
            }

            let Some(next_end) = next_text_boundary(word, end, next_boundary) else {
                break;
            };

            end = next_end;
        }

        let (end, width) = match last_fit {
            Some(result) => result,
            None => (first_end, measure(&word[start..first_end])),
        };

        visit(&word[start..end], width);
        start = end;
    }
}

fn wrap_paragraph<'a, B, M, V>(
    paragraph: &'a str,
    max_width: Pixels,
    next_boundary: &B,
    measure: &mut M,
    visit: &mut V,
) where
    B: Fn(&str, usize) -> Option<usize>,
    M: FnMut(&str) -> Pixels,
    V: FnMut(&'a str, Pixels),
{
    if paragraph.is_empty() {
        visit("", Pixels::ZERO);
        return;
    }

    if max_width.is_non_positive() {
        visit(paragraph, measure(paragraph));
        return;
    }

    let mut cursor = 0usize;
    let mut line_start: Option<usize> = None;
    let mut line_end = 0usize;
    let mut emitted = false;

    while let Some((word_start, word_end)) = next_word(paragraph, cursor) {
        if let Some(start) = line_start {
            let candidate = &paragraph[start..word_end];
            if measure(candidate) <= max_width {
                line_end = word_end;
                cursor = word_end;
                continue;
            }

            let line = &paragraph[start..line_end];

            visit(line, measure(line));

            emitted = true;
            line_start = None;
            cursor = word_start;

            continue;
        }

        let word = &paragraph[word_start..word_end];
        let width = measure(word);

        if width <= max_width {
            line_start = Some(word_start);
            line_end = word_end;
            cursor = word_end;
        } else {
            emit_oversized_word(word, max_width, next_boundary, measure, visit);

            emitted = true;
            cursor = word_end;
        }
    }

    if let Some(start) = line_start {
        let line = &paragraph[start..line_end];
        visit(line, measure(line));
        emitted = true;
    }

    if !emitted {
        visit("", Pixels::ZERO);
    }
}

pub(crate) fn for_each_text_line_with_boundaries<'a, B, M, V>(
    text: &'a str,
    wrap: TextWrap,
    max_width: Pixels,
    next_boundary: B,
    mut measure: M,
    mut visit: V,
) where
    B: Fn(&str, usize) -> Option<usize>,
    M: FnMut(&str) -> Pixels,
    V: FnMut(&'a str, Pixels),
{
    if text.is_empty() {
        return;
    }

    let mut paragraph_start = 0usize;

    loop {
        let remaining = &text[paragraph_start..];

        let (paragraph_end, has_newline) = match remaining.find('\n') {
            Some(relative) => (paragraph_start.saturating_add(relative), true),
            None => (text.len(), false),
        };

        let paragraph = &text[paragraph_start..paragraph_end];

        match wrap {
            TextWrap::NoWrap => visit(paragraph, measure(paragraph)),
            TextWrap::Word => {
                wrap_paragraph(
                    paragraph,
                    max_width,
                    &next_boundary,
                    &mut measure,
                    &mut visit,
                );
            }
        }

        if !has_newline {
            break;
        }

        paragraph_start = paragraph_end.saturating_add(1);
        if paragraph_start == text.len() {
            visit("", Pixels::ZERO);
            break;
        }
    }
}

pub(crate) fn for_each_text_line<'a, M, V>(
    text: &'a str,
    wrap: TextWrap,
    max_width: Pixels,
    measure: M,
    visit: V,
) where
    M: FnMut(&str) -> Pixels,
    V: FnMut(&'a str, Pixels),
{
    for_each_text_line_with_boundaries(text, wrap, max_width, next_scalar_boundary, measure, visit);
}

pub(crate) const ELLIPSIS: &str = "...";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisibleTextLine<'a> {
    pub(crate) text: &'a str,
    pub(crate) width: Pixels,
    pub(crate) ellipsis: bool,
}

fn ellipsize_line<'a, B, M>(
    line: &'a str,
    max_width: Pixels,
    next_boundary: &B,
    measured_ellipsized: &mut M,
) -> VisibleTextLine<'a>
where
    B: Fn(&str, usize) -> Option<usize>,
    M: FnMut(&str) -> Pixels,
{
    if max_width.is_non_positive() {
        return VisibleTextLine {
            text: "",
            width: px(0),
            ellipsis: false,
        };
    }

    let ellipsis_only_width = measured_ellipsized("");
    if ellipsis_only_width > max_width {
        return VisibleTextLine {
            text: "",
            width: ellipsis_only_width,
            ellipsis: true,
        };
    }

    let mut best_end = 0usize;
    let mut best_width = ellipsis_only_width;
    let mut cursor = 0usize;

    while let Some(end) = next_text_boundary(line, cursor, next_boundary) {
        let width = measured_ellipsized(&line[..end]);
        if width > max_width {
            break;
        }

        best_end = end;
        best_width = width;

        if end == line.len() {
            break;
        }

        cursor = end;
    }

    VisibleTextLine {
        text: &line[..best_end],
        width: best_width,
        ellipsis: true,
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_visible_line<'a, B, M, V>(
    line: &'a str,
    width: Pixels,
    max_width: Pixels,
    overflow: TextOverflow,
    force_ellipsis: bool,
    next_boundary: &B,
    measured_ellipsized: &mut M,
    visit: &mut V,
) where
    B: Fn(&str, usize) -> Option<usize>,
    M: FnMut(&str) -> Pixels,
    V: FnMut(VisibleTextLine<'a>),
{
    if overflow == TextOverflow::Ellipsis && (force_ellipsis || width > max_width) {
        visit(ellipsize_line(
            line,
            max_width,
            next_boundary,
            measured_ellipsized,
        ));

        return;
    }

    visit(VisibleTextLine {
        text: line,
        width,
        ellipsis: false,
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn for_each_visible_text_line_with_boundaries<'a, B, M, E, V>(
    text: &'a str,
    wrap: TextWrap,
    max_width: Pixels,
    max_lines: TextMaxLines,
    overflow: TextOverflow,
    next_boundary: B,
    mut measure: M,
    mut measured_ellipsized: E,
    mut visit: V,
) where
    B: Fn(&str, usize) -> Option<usize>,
    M: FnMut(&str) -> Pixels,
    E: FnMut(&str) -> Pixels,
    V: FnMut(VisibleTextLine<'a>),
{
    let limit = max_lines.limit();

    let mut seen = 0usize;
    let mut pending = None;
    let mut truncated = false;

    for_each_text_line_with_boundaries(
        text,
        wrap,
        max_width,
        &next_boundary,
        |line| measure(line),
        |line, width| match limit {
            None => emit_visible_line(
                line,
                width,
                max_width,
                overflow,
                false,
                &next_boundary,
                &mut measured_ellipsized,
                &mut visit,
            ),
            Some(limit) => {
                if seen >= limit {
                    truncated = true;
                    return;
                }

                seen = seen.saturating_add(1);

                if seen == limit {
                    pending = Some((line, width));
                } else {
                    emit_visible_line(
                        line,
                        width,
                        max_width,
                        overflow,
                        false,
                        &next_boundary,
                        &mut measured_ellipsized,
                        &mut visit,
                    );
                }
            }
        },
    );

    if let Some((line, width)) = pending {
        emit_visible_line(
            line,
            width,
            max_width,
            overflow,
            truncated,
            &next_boundary,
            &mut measured_ellipsized,
            &mut visit,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn for_each_visible_text_line<'a, M, E, V>(
    text: &'a str,
    wrap: TextWrap,
    max_width: Pixels,
    max_lines: TextMaxLines,
    overflow: TextOverflow,
    measure: M,
    measured_ellipsized: E,
    visit: V,
) where
    M: FnMut(&str) -> Pixels,
    E: FnMut(&str) -> Pixels,
    V: FnMut(VisibleTextLine<'a>),
{
    for_each_visible_text_line_with_boundaries(
        text,
        wrap,
        max_width,
        max_lines,
        overflow,
        next_scalar_boundary,
        measure,
        measured_ellipsized,
        visit,
    );
}

#[cfg(test)]
mod tests {
    use std::{vec, vec::Vec};

    use crate::{
        text_layout::{
            for_each_text_line, for_each_text_line_with_boundaries, for_each_visible_text_line,
            for_each_visible_text_line_with_boundaries,
        },
        *,
    };

    fn measure(text: &str) -> Pixels {
        let count = i32::try_from(text.chars().count()).unwrap_or(i32::MAX);
        px(count)
    }

    fn combining_mark_cluster_boundary(text: &str, from: usize) -> Option<usize> {
        if from >= text.len() || !text.is_char_boundary(from) {
            return None;
        }

        let first = text[from..].chars().next()?;
        let mut end = from.saturating_add(first.len_utf8());

        while end < text.len() {
            let character = text[end..].chars().next()?;
            if character != '\u{0301}' {
                break;
            }

            end = end.saturating_add(character.len_utf8());
        }

        Some(end)
    }

    #[test]
    fn no_wrap_only_breaks_at_explicit_newlines() {
        let mut lines = Vec::new();

        for_each_text_line(
            "hello world\nsecond",
            TextWrap::NoWrap,
            px(5),
            measure,
            |line, width| {
                lines.push((line, width));
            },
        );

        assert_eq!(lines, vec![("hello world", px(11)), ("second", px(6)),]);
    }

    #[test]
    fn word_wrap_breaks_at_spaces() {
        let mut lines = Vec::new();

        for_each_text_line(
            "hello world again",
            TextWrap::Word,
            px(5),
            measure,
            |line, width| {
                lines.push((line, width));
            },
        );

        assert_eq!(
            lines,
            vec![("hello", px(5)), ("world", px(5)), ("again", px(5)),]
        );
    }

    #[test]
    fn word_wrap_breaks_oversized_words_at_character_boundaries() {
        let mut lines = Vec::new();

        for_each_text_line("abcdef", TextWrap::Word, px(3), measure, |line, width| {
            lines.push((line, width));
        });

        assert_eq!(lines, vec![("abc", px(3)), ("def", px(3)),]);
    }

    #[test]
    fn explicit_blank_lines_are_preserved() {
        let mut lines = Vec::new();

        for_each_text_line(
            "first\n\nthird",
            TextWrap::Word,
            px(20),
            measure,
            |line, width| {
                lines.push((line, width));
            },
        );

        assert_eq!(
            lines,
            vec![("first", px(5)), ("", px(0)), ("third", px(5)),]
        );
    }

    #[test]
    fn trailing_newline_creates_empty_final_line() {
        let mut lines = Vec::new();

        for_each_text_line(
            "hello\n",
            TextWrap::NoWrap,
            px(20),
            measure,
            |line, width| {
                lines.push((line, width));
            },
        );

        assert_eq!(lines, vec![("hello", px(5)), ("", px(0)),]);
    }

    #[test]
    fn max_lines_limits_visible_lines() {
        let mut lines = Vec::new();

        for_each_visible_text_line(
            "one two three",
            TextWrap::Word,
            px(5),
            TextMaxLines::Limited(2),
            TextOverflow::Clip,
            measure,
            |text| measure(text) + px(1),
            |line| {
                lines.push((line.text, line.width, line.ellipsis));
            },
        );

        assert_eq!(lines, vec![("one", px(3), false), ("two", px(3), false),]);
    }

    #[test]
    fn line_limit_adds_ellipsis_to_last_visible_line() {
        let mut lines = Vec::new();

        for_each_visible_text_line(
            "hello world again",
            TextWrap::Word,
            px(5),
            TextMaxLines::Limited(2),
            TextOverflow::Ellipsis,
            measure,
            |text| {
                let characters = i32::try_from(text.chars().count()).unwrap_or(i32::MAX);

                px(characters.saturating_add(1))
            },
            |line| {
                lines.push((line.text, line.width, line.ellipsis));
            },
        );

        assert_eq!(lines, vec![("hello", px(5), false), ("worl", px(5), true),]);
    }

    #[test]
    fn nowrap_text_can_be_ellipsized_to_available_width() {
        let mut lines = Vec::new();

        for_each_visible_text_line(
            "abcdef",
            TextWrap::NoWrap,
            px(4),
            TextMaxLines::Unlimited,
            TextOverflow::Ellipsis,
            measure,
            |text| {
                let characters = i32::try_from(text.chars().count()).unwrap_or(i32::MAX);

                px(characters.saturating_add(1))
            },
            |line| {
                lines.push((line.text, line.width, line.ellipsis));
            },
        );

        assert_eq!(lines, vec![("abc", px(4), true),]);
    }

    #[test]
    fn ellipsis_truncation_preserves_utf8_boundaries() {
        let mut lines = Vec::new();

        for_each_visible_text_line(
            "éééé",
            TextWrap::NoWrap,
            px(3),
            TextMaxLines::Unlimited,
            TextOverflow::Ellipsis,
            measure,
            |text| {
                let characters = i32::try_from(text.chars().count()).unwrap_or(i32::MAX);

                px(characters.saturating_add(1))
            },
            |line| {
                lines.push((line.text, line.width, line.ellipsis));
            },
        );

        assert_eq!(lines, vec![("éé", px(3), true),]);
    }

    #[test]
    fn oversized_words_break_only_at_supplied_cluster_boundaries() {
        let mut lines = Vec::new();

        for_each_text_line_with_boundaries(
            "a\u{0301}b",
            TextWrap::Word,
            px(1),
            combining_mark_cluster_boundary,
            measure,
            |line, width| {
                lines.push((line, width));
            },
        );

        assert_eq!(lines, vec![("a\u{0301}", px(2)), ("b", px(1)),],);
    }

    #[test]
    fn ellipsis_does_not_split_a_supplied_cluster() {
        let mut lines = Vec::new();

        for_each_visible_text_line_with_boundaries(
            "a\u{0301}bc",
            TextWrap::NoWrap,
            px(2),
            TextMaxLines::Unlimited,
            TextOverflow::Ellipsis,
            combining_mark_cluster_boundary,
            measure,
            |text| measure(text) + px(1),
            |line| {
                lines.push((line.text, line.width, line.ellipsis));
            },
        );

        // "a + combining acute" is one cluster and would require three units including
        // the ellipsis. We therefore render only the ellipsis instead of illegally
        // keeping just "a".
        assert_eq!(lines, vec![("", px(1), true)],);
    }
}
