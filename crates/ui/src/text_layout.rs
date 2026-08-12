use crate::{Pixels, TextMaxLines, TextOverflow, TextWrap, px};

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

fn emit_oversized_word<'a, M, V>(word: &'a str, max_width: Pixels, measure: &mut M, visit: &mut V)
where
    M: FnMut(&str) -> Pixels,
    V: FnMut(&'a str, Pixels),
{
    let mut start = 0;
    while start < word.len() {
        let mut first_end = None;
        let mut last_fit = None;

        for (relative, character) in word[start..].char_indices() {
            let end = start + relative + character.len_utf8();
            first_end.get_or_insert(end);

            let candidate = &word[start..end];
            let width = measure(candidate);
            if width <= max_width {
                last_fit = Some((end, width));
            } else {
                break;
            }
        }

        let (end, width) = match last_fit {
            Some(result) => result,
            None => {
                let end = first_end.expect("non-empty word must contain a character");
                (end, measure(&word[start..end]))
            }
        };

        visit(&word[start..end], width);
        start = end;
    }
}

fn wrap_paragraph<'a, M, V>(paragraph: &'a str, max_width: Pixels, measure: &mut M, visit: &mut V)
where
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

    let mut cursor = 0;
    let mut line_start: Option<usize> = None;
    let mut line_end = 0;
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
            emit_oversized_word(word, max_width, measure, visit);

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

pub(crate) fn for_each_text_line<'a, M, V>(
    text: &'a str,
    wrap: TextWrap,
    max_width: Pixels,
    mut measure: M,
    mut visit: V,
) where
    M: FnMut(&str) -> Pixels,
    V: FnMut(&'a str, Pixels),
{
    if text.is_empty() {
        return;
    }

    let mut paragraph_start = 0;

    loop {
        let remaining = &text[paragraph_start..];
        let (paragraph_end, has_newline) = match remaining.find('\n') {
            Some(relative) => (paragraph_start + relative, true),
            None => (text.len(), false),
        };

        let paragraph = &text[paragraph_start..paragraph_end];

        match wrap {
            TextWrap::NoWrap => visit(paragraph, measure(paragraph)),
            TextWrap::Word => wrap_paragraph(paragraph, max_width, &mut measure, &mut visit),
        }

        if !has_newline {
            break;
        }

        paragraph_start = paragraph_end + 1;
        if paragraph_start == text.len() {
            visit("", Pixels::ZERO);
            break;
        }
    }
}

pub(crate) const ELLIPSIS: &str = "...";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisibleTextLine<'a> {
    pub(crate) text: &'a str,
    pub(crate) width: Pixels,
    pub(crate) ellipsis: bool,
}

fn eliipsize_line<'a, M>(
    line: &'a str,
    max_width: Pixels,
    measured_ellipsized: &mut M,
) -> VisibleTextLine<'a>
where
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

    let mut best_end = 0;
    let mut best_width = ellipsis_only_width;

    for (offset, character) in line.char_indices() {
        let end = offset + character.len_utf8();
        let width = measured_ellipsized(&line[..end]);
        if width > max_width {
            break;
        }
        best_end = end;
        best_width = width;
    }

    VisibleTextLine {
        text: &line[..best_end],
        width: best_width,
        ellipsis: true,
    }
}

fn emit_visible_line<'a, M, V>(
    line: &'a str,
    width: Pixels,
    max_width: Pixels,
    overflow: TextOverflow,
    force_ellipsis: bool,
    measured_ellipsized: &mut M,
    visit: &mut V,
) where
    M: FnMut(&str) -> Pixels,
    V: FnMut(VisibleTextLine<'a>),
{
    if overflow == TextOverflow::Ellipsis && (force_ellipsis || width > max_width) {
        visit(eliipsize_line(line, max_width, measured_ellipsized));
        return;
    }

    visit(VisibleTextLine {
        text: line,
        width,
        ellipsis: false,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn for_each_visible_text_line<'a, M, E, V>(
    text: &'a str,
    wrap: TextWrap,
    max_width: Pixels,
    max_lines: TextMaxLines,
    overflow: TextOverflow,
    mut measure: M,
    mut measured_ellipsized: E,
    mut visit: V,
) where
    M: FnMut(&str) -> Pixels,
    E: FnMut(&str) -> Pixels,
    V: FnMut(VisibleTextLine<'a>),
{
    let limit = max_lines.limit();

    let mut seen = 0;
    let mut pending = None;
    let mut truncated = false;

    for_each_text_line(
        text,
        wrap,
        max_width,
        |line| measure(line),
        |line, width| match limit {
            None => emit_visible_line(
                line,
                width,
                max_width,
                overflow,
                false,
                &mut measured_ellipsized,
                &mut visit,
            ),
            Some(limit) => {
                if seen >= limit {
                    truncated = true;
                    return;
                }

                seen += 1;
                if seen == limit {
                    pending = Some((line, width));
                } else {
                    emit_visible_line(
                        line,
                        width,
                        max_width,
                        overflow,
                        false,
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
            &mut measured_ellipsized,
            &mut visit,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{vec, vec::Vec};

    use crate::{
        text_layout::{for_each_text_line, for_each_visible_text_line},
        *,
    };

    fn measure(text: &str) -> Pixels {
        let count = i32::try_from(text.chars().count()).unwrap_or(i32::MAX);
        px(count)
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
}
