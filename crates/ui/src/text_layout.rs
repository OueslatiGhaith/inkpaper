use core::ops::ControlFlow;

use unicode_linebreak::{BreakOpportunity, linebreaks};

use crate::{Pixels, ShapeError, TextMaxLines, TextOverflow, TextWrap, px};

/// a line's width, or why the shaper couldn't shape it
pub(crate) type LineWidth = Result<Pixels, ShapeError>;

/// whether a line fits in `max_width`. A line the shaper can't shape never fits, so
/// wrapping and ellipsis shorten it until it can
fn fits(width: LineWidth, max_width: Pixels) -> bool {
    width.is_ok_and(|width| width <= max_width)
}

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

// characters that hang past the end of a line instead of counting toward its width
fn is_hanging_whitespace(character: char) -> bool {
    matches!(
        character,
        ' ' | '\t' | '\r' | '\u{000B}' | '\u{000C}' | '\u{0085}' | '\u{2028}' | '\u{2029}'
    )
}

fn skip_hanging_whitespace(text: &str, mut offset: usize) -> usize {
    while offset < text.len() {
        let Some(character) = text[offset..].chars().next() else {
            break;
        };
        if !is_hanging_whitespace(character) {
            break;
        }

        offset += character.len_utf8();
    }

    offset
}

fn trim_hanging_whitespace(text: &str, start: usize, mut end: usize) -> usize {
    while end > start {
        let Some(character) = text[start..end].chars().next_back() else {
            break;
        };
        if !is_hanging_whitespace(character) {
            break;
        }

        end -= character.len_utf8();
    }

    end
}

fn advance_boundaries<B>(
    text: &str,
    mut pos: usize,
    max_steps: usize,
    next_boundary: &B,
) -> (usize, usize)
where
    B: Fn(&str, usize) -> Option<usize>,
{
    let mut taken = 0usize;

    while taken < max_steps && pos < text.len() {
        let Some(next) = next_text_boundary(text, pos, next_boundary) else {
            break;
        };
        pos = next;
        taken += 1;
    }

    (taken, pos)
}

/// Finds the longest prefix of `text` ending at a cluster boundary `>= start_end`
/// that fits within `max_width`, using exponential (galloping) + binary search.
///
/// Makes `O(log k)` calls to `measure` and `O(k)` boundary steps with zero allocation.
/// If the very first boundary after `start_end` fails to fit and `initial` is `None`,
/// returns `Err((first_end, first_width))` so callers that must make progress (like
/// oversized-word splitting) do not re-measure it.
fn find_longest_fitting_cluster_prefix<B, M>(
    text: &str,
    start_end: usize,
    initial: Option<(usize, LineWidth)>,
    max_width: Pixels,
    next_boundary: &B,
    measure: &mut M,
) -> Result<Option<(usize, LineWidth)>, (usize, LineWidth)>
where
    B: Fn(&str, usize) -> Option<usize>,
    M: FnMut(&str) -> LineWidth,
{
    let mut lo_steps = 0usize;
    let mut lo_byte = start_end;
    let mut lo_result = initial;
    let mut step = 1usize;
    let mut hi_steps = None;

    while lo_byte < text.len() {
        let (taken, probe_byte) = advance_boundaries(text, lo_byte, step, next_boundary);
        if taken == 0 {
            break;
        }

        let width = measure(&text[..probe_byte]);
        if fits(width, max_width) {
            lo_steps += taken;
            lo_byte = probe_byte;
            lo_result = Some((probe_byte, width));

            if probe_byte == text.len() {
                return Ok(lo_result);
            }

            step = step.saturating_mul(2);
        } else {
            if lo_result.is_none() && lo_steps == 0 && taken == 1 {
                return Err((probe_byte, width));
            }
            hi_steps = Some(lo_steps + taken);
            break;
        }
    }

    if let Some(mut hi) = hi_steps {
        while lo_steps + 1 < hi {
            let mid_delta = (hi - lo_steps) / 2;
            let (taken, mid_byte) = advance_boundaries(text, lo_byte, mid_delta, next_boundary);
            if taken == 0 {
                break;
            }

            let width = measure(&text[..mid_byte]);
            if fits(width, max_width) {
                lo_steps += taken;
                lo_byte = mid_byte;
                lo_result = Some((mid_byte, width));
            } else {
                hi = lo_steps + taken;
            }
        }
    }

    Ok(lo_result)
}

/// Visits the pieces of a word wider than the line, each as wide as fits, except the
/// last. Returns `(tail_start, tail_width)` so later words can share its line without
/// re-measuring it.
fn split_oversized_word<'a, B, M, V>(
    word: &'a str,
    max_width: Pixels,
    next_boundary: &B,
    measure: &mut M,
    visit: &mut V,
) -> ControlFlow<(), (usize, LineWidth)>
where
    B: Fn(&str, usize) -> Option<usize>,
    M: FnMut(&str) -> LineWidth,
    V: FnMut(&'a str, LineWidth) -> ControlFlow<()>,
{
    let mut start = 0usize;

    loop {
        let rest = &word[start..];
        let (rel_end, width) = match find_longest_fitting_cluster_prefix(
            rest,
            0,
            None,
            max_width,
            next_boundary,
            measure,
        ) {
            Ok(Some(found)) => found,
            Err(first) => first,
            Ok(None) => {
                let first_end = next_text_boundary(rest, 0, next_boundary)
                    .expect("non-empty word must contain a text boundary");
                (first_end, measure(&rest[..first_end]))
            }
        };

        let end = start + rel_end;
        if end == word.len() {
            return ControlFlow::Continue((start, width));
        }

        visit(&word[start..end], width)?;
        start = end;
    }
}

/// Advances up to `max_steps` non-empty word-break segments inside `chunk` starting
/// from `pos`, returning `(steps_taken, content_end, segment_end)`.
fn advance_word_breaks(chunk: &str, pos: usize, max_steps: usize) -> (usize, usize, usize) {
    let mut taken = 0usize;
    let mut seg_start = pos;
    let mut last_content_end = pos;
    let mut last_seg_end = pos;

    for (rel_end, _) in linebreaks(&chunk[pos..]) {
        let seg_end = pos + rel_end;
        let content_end = trim_hanging_whitespace(chunk, seg_start, seg_end);
        seg_start = seg_end;

        if content_end <= last_content_end {
            last_seg_end = seg_end;
            continue;
        }

        last_content_end = content_end;
        last_seg_end = seg_end;
        taken += 1;

        if taken == max_steps {
            break;
        }
    }

    (taken, last_content_end, last_seg_end)
}

/// Wraps one mandatory-break-free chunk of a paragraph in `TextWrap::Word` mode,
/// galloping + binary-searching over word breaks per line (`O(log W)` measurements).
fn wrap_word_chunk<'a, B, M, V>(
    chunk: &'a str,
    max_width: Pixels,
    next_boundary: &B,
    measure: &mut M,
    visit: &mut V,
) -> ControlFlow<()>
where
    B: Fn(&str, usize) -> Option<usize>,
    M: FnMut(&str) -> LineWidth,
    V: FnMut(&'a str, LineWidth) -> ControlFlow<()>,
{
    let mut cursor = skip_hanging_whitespace(chunk, 0);
    if cursor >= chunk.len() {
        return visit("", Ok(Pixels::ZERO));
    }

    // Track an oversized word's trailing piece so subsequent words can join its line.
    let mut carried: Option<(usize, usize, LineWidth)> = None;

    while cursor < chunk.len() || carried.is_some() {
        let (line_start, mut lo_steps, mut lo_content_end, mut lo_seg_end, mut lo_width) =
            if let Some((start, seg_end, width)) = carried.take() {
                let content_end = trim_hanging_whitespace(chunk, start, seg_end);
                (start, 0usize, content_end, seg_end, Some(width))
            } else {
                let start = skip_hanging_whitespace(chunk, cursor);
                if start >= chunk.len() {
                    break;
                }
                (start, 0usize, start, start, None)
            };

        let mut step = 1usize;
        let mut hi_steps = None;

        while lo_seg_end < chunk.len() {
            let (taken, probe_content_end, probe_seg_end) =
                advance_word_breaks(chunk, lo_seg_end, step);
            if taken == 0 {
                lo_seg_end = chunk.len();
                break;
            }

            let width = measure(&chunk[line_start..probe_content_end]);
            if fits(width, max_width) {
                lo_steps += taken;
                lo_content_end = probe_content_end;
                lo_seg_end = probe_seg_end;
                lo_width = Some(width);

                if probe_seg_end >= chunk.len() {
                    break;
                }

                step = step.saturating_mul(2);
            } else {
                hi_steps = Some(lo_steps + taken);
                break;
            }
        }

        if let Some(mut hi) = hi_steps {
            while lo_steps + 1 < hi {
                let mid_delta = (hi - lo_steps) / 2;
                let (taken, mid_content_end, mid_seg_end) =
                    advance_word_breaks(chunk, lo_seg_end, mid_delta);
                if taken == 0 {
                    break;
                }

                let width = measure(&chunk[line_start..mid_content_end]);
                if fits(width, max_width) {
                    lo_steps += taken;
                    lo_content_end = mid_content_end;
                    lo_seg_end = mid_seg_end;
                    lo_width = Some(width);
                } else {
                    hi = lo_steps + taken;
                }
            }
        }

        if let Some(width) = lo_width {
            visit(&chunk[line_start..lo_content_end], width)?;
            cursor = lo_seg_end;
        } else {
            // The very first word on this line didn't fit — split it by cluster.
            let (_, word_content_end, word_seg_end) = advance_word_breaks(chunk, line_start, 1);
            let word = &chunk[line_start..word_content_end];
            let (tail_rel, tail_width) =
                split_oversized_word(word, max_width, next_boundary, measure, visit)?;
            let tail_start = line_start + tail_rel;

            cursor = word_seg_end;
            if word_seg_end < chunk.len() {
                carried = Some((tail_start, word_seg_end, tail_width));
            } else {
                visit(&chunk[tail_start..word_content_end], tail_width)?;
            }
        }
    }

    ControlFlow::Continue(())
}

/// Wraps one mandatory-break-free chunk of a paragraph in `TextWrap::Anywhere` mode,
/// measuring whole words when they fit and galloping over clusters only on the word
/// that crosses `max_width`.
fn wrap_anywhere_chunk<'a, B, M, V>(
    chunk: &'a str,
    max_width: Pixels,
    next_boundary: &B,
    measure: &mut M,
    visit: &mut V,
) -> ControlFlow<()>
where
    B: Fn(&str, usize) -> Option<usize>,
    M: FnMut(&str) -> LineWidth,
    V: FnMut(&'a str, LineWidth) -> ControlFlow<()>,
{
    let mut line_start: Option<usize> = None;
    let mut line_end = 0usize;
    let mut line_width = Ok(Pixels::ZERO);
    let mut emitted = false;
    let mut segment_start = 0usize;

    for (segment_end, _) in linebreaks(chunk) {
        let start = segment_start;
        segment_start = segment_end;

        let content_end = trim_hanging_whitespace(chunk, start, segment_end);
        let mut cursor = match line_start {
            Some(_) => start,
            None => skip_hanging_whitespace(chunk, start).min(content_end),
        };

        while cursor < content_end {
            if let Some(line) = line_start {
                let candidate = &chunk[line..content_end];
                let width = measure(candidate);
                if fits(width, max_width) {
                    line_end = content_end;
                    line_width = width;
                    break;
                }

                // Try extending `chunk[line..]` past `cursor` (starting at `cursor - line`).
                match find_longest_fitting_cluster_prefix(
                    candidate,
                    cursor - line,
                    None,
                    max_width,
                    next_boundary,
                    measure,
                ) {
                    Ok(Some((rel_end, width))) => {
                        let fit_end = line + rel_end;
                        visit(&chunk[line..fit_end], width)?;
                        emitted = true;
                        line_start = None;
                        cursor = skip_hanging_whitespace(chunk, fit_end).min(content_end);
                    }
                    Ok(None) | Err(_) => {
                        // Not even the first cluster at `cursor` fit; flush the prior line.
                        visit(&chunk[line..line_end], line_width)?;
                        emitted = true;
                        line_start = None;
                        cursor = skip_hanging_whitespace(chunk, cursor).min(content_end);
                    }
                }
            } else {
                let word = &chunk[cursor..content_end];
                let width = measure(word);
                if fits(width, max_width) {
                    line_start = Some(cursor);
                    line_end = content_end;
                    line_width = width;
                    break;
                }

                let (tail_rel, tail_width) =
                    split_oversized_word(word, max_width, next_boundary, measure, visit)?;
                if tail_rel > 0 {
                    emitted = true;
                }

                line_start = Some(cursor + tail_rel);
                line_end = content_end;
                line_width = tail_width;
                break;
            }
        }
    }

    if let Some(start) = line_start {
        visit(&chunk[start..line_end], line_width)?;
        emitted = true;
    }

    if !emitted {
        visit("", Ok(Pixels::ZERO))?;
    }

    ControlFlow::Continue(())
}

/// wraps between words, or with `anywhere` between any two clusters
fn wrap_paragraph<'a, B, M, V>(
    paragraph: &'a str,
    anywhere: bool,
    max_width: Pixels,
    next_boundary: &B,
    measure: &mut M,
    visit: &mut V,
) -> ControlFlow<()>
where
    B: Fn(&str, usize) -> Option<usize>,
    M: FnMut(&str) -> LineWidth,
    V: FnMut(&'a str, LineWidth) -> ControlFlow<()>,
{
    if paragraph.is_empty() {
        return visit("", Ok(Pixels::ZERO));
    }

    if max_width.is_non_positive() {
        return visit(paragraph, measure(paragraph));
    }

    let mut chunk_start = 0usize;

    for (segment_end, opportunity) in linebreaks(paragraph) {
        if opportunity != BreakOpportunity::Mandatory {
            continue;
        }

        let chunk = &paragraph[chunk_start..segment_end];
        chunk_start = segment_end;

        if anywhere {
            wrap_anywhere_chunk(chunk, max_width, next_boundary, measure, visit)?;
        } else {
            wrap_word_chunk(chunk, max_width, next_boundary, measure, visit)?;
        }
    }

    ControlFlow::Continue(())
}

fn try_for_each_text_line_with_boundaries<'a, B, M, V>(
    text: &'a str,
    wrap: TextWrap,
    max_width: Pixels,
    next_boundary: B,
    mut measure: M,
    mut visit: V,
) -> ControlFlow<()>
where
    B: Fn(&str, usize) -> Option<usize>,
    M: FnMut(&str) -> LineWidth,
    V: FnMut(&'a str, LineWidth) -> ControlFlow<()>,
{
    if text.is_empty() {
        return ControlFlow::Continue(());
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
            TextWrap::NoWrap => visit(paragraph, measure(paragraph))?,
            TextWrap::Word | TextWrap::Anywhere => {
                wrap_paragraph(
                    paragraph,
                    wrap == TextWrap::Anywhere,
                    max_width,
                    &next_boundary,
                    &mut measure,
                    &mut visit,
                )?;
            }
        }

        if !has_newline {
            break;
        }

        paragraph_start = paragraph_end.saturating_add(1);
        if paragraph_start == text.len() {
            visit("", Ok(Pixels::ZERO))?;
            break;
        }
    }

    ControlFlow::Continue(())
}

#[cfg(test)]
pub(crate) fn for_each_text_line_with_boundaries<'a, B, M, V>(
    text: &'a str,
    wrap: TextWrap,
    max_width: Pixels,
    next_boundary: B,
    measure: M,
    mut visit: V,
) where
    B: Fn(&str, usize) -> Option<usize>,
    M: FnMut(&str) -> LineWidth,
    V: FnMut(&'a str, LineWidth),
{
    let _ = try_for_each_text_line_with_boundaries(
        text,
        wrap,
        max_width,
        next_boundary,
        measure,
        |line, width| {
            visit(line, width);
            ControlFlow::Continue(())
        },
    );
}

#[cfg(test)]
pub(crate) fn for_each_text_line<'a, M, V>(
    text: &'a str,
    wrap: TextWrap,
    max_width: Pixels,
    measure: M,
    visit: V,
) where
    M: FnMut(&str) -> LineWidth,
    V: FnMut(&'a str, LineWidth),
{
    for_each_text_line_with_boundaries(text, wrap, max_width, next_scalar_boundary, measure, visit);
}

pub(crate) const ELLIPSIS: &str = "...";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisibleTextLine<'a> {
    pub(crate) text: &'a str,
    pub(crate) width: LineWidth,
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
    M: FnMut(&str) -> LineWidth,
{
    if max_width.is_non_positive() {
        return VisibleTextLine {
            text: "",
            width: Ok(px(0)),
            ellipsis: false,
        };
    }

    let ellipsis_only_width = measured_ellipsized("");
    if !fits(ellipsis_only_width, max_width) {
        return VisibleTextLine {
            text: "",
            width: ellipsis_only_width,
            ellipsis: true,
        };
    }

    let (best_end, best_width) = match find_longest_fitting_cluster_prefix(
        line,
        0,
        Some((0, ellipsis_only_width)),
        max_width,
        next_boundary,
        measured_ellipsized,
    ) {
        Ok(Some(found)) => found,
        _ => (0, ellipsis_only_width),
    };

    VisibleTextLine {
        text: &line[..best_end],
        width: best_width,
        ellipsis: true,
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_visible_line<'a, B, M, V>(
    line: &'a str,
    width: LineWidth,
    max_width: Pixels,
    overflow: TextOverflow,
    force_ellipsis: bool,
    next_boundary: &B,
    measured_ellipsized: &mut M,
    visit: &mut V,
) where
    B: Fn(&str, usize) -> Option<usize>,
    M: FnMut(&str) -> LineWidth,
    V: FnMut(VisibleTextLine<'a>),
{
    if overflow == TextOverflow::Ellipsis && (force_ellipsis || !fits(width, max_width)) {
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
    M: FnMut(&str) -> LineWidth,
    E: FnMut(&str) -> LineWidth,
    V: FnMut(VisibleTextLine<'a>),
{
    let limit = max_lines.limit();

    let mut seen = 0usize;
    let mut pending = None;
    let mut truncated = false;

    let _ = try_for_each_text_line_with_boundaries(
        text,
        wrap,
        max_width,
        &next_boundary,
        |line| measure(line),
        |line, width| match limit {
            None => {
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
                ControlFlow::Continue(())
            }
            Some(limit) => {
                if seen >= limit {
                    truncated = true;
                    return ControlFlow::Break(());
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

                ControlFlow::Continue(())
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

#[cfg(test)]
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
    M: FnMut(&str) -> LineWidth,
    E: FnMut(&str) -> LineWidth,
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
            LineWidth, for_each_text_line, for_each_text_line_with_boundaries,
            for_each_visible_text_line, for_each_visible_text_line_with_boundaries,
        },
        *,
    };

    fn measure(text: &str) -> LineWidth {
        let count = i32::try_from(text.chars().count()).unwrap_or(i32::MAX);
        Ok(px(count))
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
                lines.push((line, width.unwrap()));
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
                lines.push((line, width.unwrap()));
            },
        );

        assert_eq!(
            lines,
            vec![("hello", px(5)), ("world", px(5)), ("again", px(5)),]
        );
    }

    fn wrap(text: &str, max_width: i32) -> Vec<&str> {
        let mut lines = Vec::new();

        for_each_text_line(text, TextWrap::Word, px(max_width), measure, |line, _| {
            lines.push(line);
        });

        lines
    }

    #[test]
    fn word_wrap_breaks_after_hyphens_and_slashes() {
        assert_eq!(wrap("well-known", 6), vec!["well-", "known"]);
        assert_eq!(wrap("and/or", 4), vec!["and/", "or"]);
    }

    #[test]
    fn word_wrap_breaks_around_em_dashes() {
        assert_eq!(wrap("wait\u{2014}what", 5), vec!["wait\u{2014}", "what"]);
        assert_eq!(wrap("abc\u{2014}d", 3), vec!["abc", "\u{2014}d"]);
    }

    #[test]
    fn word_wrap_breaks_between_ideographs_but_keeps_closing_punctuation() {
        assert_eq!(
            wrap("\u{6211}\u{559C}\u{6B22}\u{4E66}\u{3002}", 2),
            vec!["\u{6211}\u{559C}", "\u{6B22}", "\u{4E66}\u{3002}"],
        );
    }

    #[test]
    fn word_wrap_does_not_break_at_no_break_spaces() {
        assert_eq!(wrap("10\u{00A0}km away", 6), vec!["10\u{00A0}km", "away"]);
    }

    #[test]
    fn word_wrap_hangs_tabs_and_leading_spaces() {
        assert_eq!(wrap("  one\ttwo", 3), vec!["one", "two"]);
    }

    #[test]
    fn word_wrap_forces_mandatory_breaks() {
        assert_eq!(wrap("one\u{2028}two", 20), vec!["one", "two"]);
        assert_eq!(wrap("one\u{2028}\u{2028}two", 20), vec!["one", "", "two"]);
        assert_eq!(wrap("one\r\ntwo", 20), vec!["one", "two"]);
    }

    #[test]
    fn word_wrap_breaks_oversized_words_at_character_boundaries() {
        let mut lines = Vec::new();

        for_each_text_line("abcdef", TextWrap::Word, px(3), measure, |line, width| {
            lines.push((line, width.unwrap()));
        });

        assert_eq!(lines, vec![("abc", px(3)), ("def", px(3)),]);
    }

    #[test]
    fn word_wrap_starts_the_next_line_with_an_oversized_words_tail() {
        assert_eq!(wrap("abcdefgh ij", 6), vec!["abcdef", "gh ij"]);
        assert_eq!(
            wrap("abcdefghijklm no", 6),
            vec!["abcdef", "ghijkl", "m no"]
        );
        // a word that fits a line of its own still moves to one before splitting
        assert_eq!(wrap("x abcdefgh", 6), vec!["x", "abcdef", "gh"]);
    }

    #[test]
    fn an_oversized_words_tail_shares_a_limited_line() {
        let mut lines = Vec::new();

        for_each_visible_text_line(
            "abcdefgh ij",
            TextWrap::Word,
            px(6),
            TextMaxLines::Limited(2),
            TextOverflow::Ellipsis,
            measure,
            measure,
            |line| lines.push((line.text, line.ellipsis)),
        );

        assert_eq!(lines, vec![("abcdef", false), ("gh ij", false)]);
    }

    fn wrap_anywhere(text: &str, max_width: i32) -> Vec<&str> {
        let mut lines = Vec::new();

        for_each_text_line(
            text,
            TextWrap::Anywhere,
            px(max_width),
            measure,
            |line, _| {
                lines.push(line);
            },
        );

        lines
    }

    #[test]
    fn anywhere_wrap_breaks_between_any_characters() {
        assert_eq!(wrap_anywhere("x abcdefgh", 6), vec!["x abcd", "efgh"]);
        assert_eq!(wrap_anywhere("hello world", 7), vec!["hello w", "orld"]);
        assert_eq!(wrap_anywhere("abcdef", 3), vec!["abc", "def"]);
    }

    #[test]
    fn anywhere_wrap_hangs_whitespace_and_forces_mandatory_breaks() {
        assert_eq!(wrap_anywhere("hello world", 5), vec!["hello", "world"]);
        assert_eq!(wrap_anywhere("  ab cd", 3), vec!["ab", "cd"]);
        assert_eq!(wrap_anywhere("ab\u{2028}cd", 20), vec!["ab", "cd"]);
        assert_eq!(
            wrap_anywhere("ab\u{2028}\u{2028}cd", 20),
            vec!["ab", "", "cd"]
        );
        assert_eq!(wrap_anywhere("ab\ncd", 1), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn anywhere_wrap_breaks_only_at_supplied_cluster_boundaries() {
        let mut lines = Vec::new();

        for_each_text_line_with_boundaries(
            "a\u{0301}bc",
            TextWrap::Anywhere,
            px(2),
            combining_mark_cluster_boundary,
            measure,
            |line, _| lines.push(line),
        );

        assert_eq!(lines, vec!["a\u{0301}", "bc"]);
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
                lines.push((line, width.unwrap()));
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
                lines.push((line, width.unwrap()));
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
            |text| measure(text).map(|width| width + px(1)),
            |line| {
                lines.push((line.text, line.width.unwrap(), line.ellipsis));
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

                Ok(px(characters.saturating_add(1)))
            },
            |line| {
                lines.push((line.text, line.width.unwrap(), line.ellipsis));
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

                Ok(px(characters.saturating_add(1)))
            },
            |line| {
                lines.push((line.text, line.width.unwrap(), line.ellipsis));
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

                Ok(px(characters.saturating_add(1)))
            },
            |line| {
                lines.push((line.text, line.width.unwrap(), line.ellipsis));
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
                lines.push((line, width.unwrap()));
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
            |text| measure(text).map(|width| width + px(1)),
            |line| {
                lines.push((line.text, line.width.unwrap(), line.ellipsis));
            },
        );

        // "a + combining acute" is one cluster and would require three units including
        // the ellipsis. We therefore render only the ellipsis instead of illegally
        // keeping just "a".
        assert_eq!(lines, vec![("", px(1), true)],);
    }

    /// measures like `measure`, but can't shape more than six characters at once
    fn measure_up_to_six(text: &str) -> LineWidth {
        if text.chars().count() > 6 {
            return Err(ShapeError::TooManyDirectionalRuns);
        }

        measure(text)
    }

    #[test]
    fn lines_the_shaper_cannot_shape_wrap_like_overlong_ones() {
        let mut lines = Vec::new();

        for_each_text_line(
            "aa bb cc dd",
            TextWrap::Word,
            px(100),
            measure_up_to_six,
            |line, width| {
                lines.push((line, width.unwrap()));
            },
        );

        assert_eq!(lines, vec![("aa bb", px(5)), ("cc dd", px(5))]);
    }

    #[test]
    fn lines_the_shaper_cannot_shape_are_ellipsized() {
        let mut lines = Vec::new();

        for_each_visible_text_line(
            "abcdefgh",
            TextWrap::NoWrap,
            px(100),
            TextMaxLines::Unlimited,
            TextOverflow::Ellipsis,
            measure_up_to_six,
            |text| measure_up_to_six(text).map(|width| width + px(1)),
            |line| {
                lines.push((line.text, line.width.unwrap(), line.ellipsis));
            },
        );

        assert_eq!(lines, vec![("abcdef", px(7), true)]);
    }

    #[test]
    fn max_lines_stops_measuring_after_truncation_is_known() {
        let text = "word ".repeat(200);
        let mut measure_calls = 0usize;
        let mut lines = Vec::new();

        for_each_visible_text_line(
            &text,
            TextWrap::Word,
            px(10),
            TextMaxLines::Limited(2),
            TextOverflow::Ellipsis,
            |slice| {
                measure_calls += 1;
                measure(slice)
            },
            |slice| measure(slice).map(|width| width + px(1)),
            |line| lines.push((line.text, line.ellipsis)),
        );

        assert_eq!(lines, vec![("word word", false), ("word word", true)]);
        // 2 visible lines + 1 overflow line to detect truncation; never walks all 200 words.
        assert!(
            measure_calls < 20,
            "expected early exit after max_lines, got {measure_calls} measure calls"
        );
    }

    #[test]
    fn long_single_line_wraps_in_logarithmic_measurements() {
        let text = "a ".repeat(128);
        let mut measure_calls = 0usize;
        let mut lines = Vec::new();

        for_each_text_line(
            &text,
            TextWrap::Word,
            px(10_000),
            |slice| {
                measure_calls += 1;
                measure(slice)
            },
            |line, _| lines.push(line),
        );

        assert_eq!(lines.len(), 1);
        assert!(
            measure_calls <= 16,
            "expected O(log N) measure calls, got {measure_calls}"
        );
    }

    #[test]
    fn ellipsizing_long_line_uses_logarithmic_measurements() {
        let text = "a".repeat(200);
        let mut ellipsis_calls = 0usize;
        let mut lines = Vec::new();

        for_each_visible_text_line(
            &text,
            TextWrap::NoWrap,
            px(100),
            TextMaxLines::Unlimited,
            TextOverflow::Ellipsis,
            measure,
            |slice| {
                ellipsis_calls += 1;
                measure(slice).map(|width| width + px(3))
            },
            |line| lines.push((line.text.len(), line.ellipsis)),
        );

        assert_eq!(lines, vec![(97, true)]);
        assert!(
            ellipsis_calls <= 20,
            "expected O(log N) ellipsis measurements, got {ellipsis_calls}"
        );
    }
}
