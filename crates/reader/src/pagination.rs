use alloc::{borrow::Cow, string::String, vec::Vec};
use core::mem;

use inkpaper_epub::{
    BlockKind, BookLocation, Chapter, ChapterBlock, ChapterImage, ChapterStyles, ComputedStyle,
    ContentOffset, CssLength, FontStyle, FontWeight, ImageDimensions, Inline, LineHeight,
    LinkTarget, SpineIndex, StyleNodeId, TextAlign, TextRun,
};
use unicode_linebreak::linebreaks;

use crate::{
    ImageFragment, ImageMeasurer, Page, PageItem, ReaderSettings, ReadingPosition, Rect,
    TextFragment, TextMeasurer, TextStyle, Viewport,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pagination<'a> {
    pages: Vec<Page<'a>>,
}

impl<'a> Pagination<'a> {
    pub fn pages(&self) -> &[Page<'a>] {
        &self.pages
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// detatches this chapter's pages from their borrowed EPUB content.
    ///
    /// allocation happens here, before the pages enter the rendering path
    pub fn into_owned(self) -> Pagination<'static> {
        Pagination {
            pages: self.pages.into_iter().map(Page::into_owned).collect(),
        }
    }

    /// finds the page containing a position from the same book and chapter.
    ///
    /// repagination may move the position within a page. Chapter ends are exclusive.
    /// Positions outside this chapter return `None`
    pub fn page_at_position(&self, position: ReadingPosition) -> Option<usize> {
        let index = self
            .pages
            .partition_point(|page| page.position() <= position)
            .checked_sub(1)?;
        let page = &self.pages[index];

        // empty chapters still have a single page that can be reopened
        (position < page.end_position()
            || (self.pages.len() == 1
                && position == page.position()
                && page.position() == page.end_position()))
        .then_some(index)
    }
}

pub fn paginate_chapter<'a, M>(
    chapter: &'a Chapter,
    styles: &ChapterStyles,
    spine: SpineIndex,
    viewport: Viewport,
    settings: ReaderSettings,
    measurer: &mut M,
) -> Result<Pagination<'a>, M::Error>
where
    M: TextMeasurer + ImageMeasurer,
{
    Paginator::new(chapter, styles, spine, viewport, settings, measurer).paginate()
}

struct PendingText<'a> {
    source: &'a str,
    start: usize,
    end: usize,
    style: TextStyle,
    link: Option<&'a LinkTarget>,
    x: u32,
    width: u32,
    /// the extra pixels after each U+0020 space, or `None` without spaces
    word_spacing: Option<u32>,
    /// the start of a word broken at the end of its line, drawn with a hyphen
    hyphen: bool,
}

impl<'a> PendingText<'a> {
    fn text(&self) -> &'a str {
        self.source
            .get(self.start..self.end)
            .expect("pending text range must stay on UTF-8 boundaries")
    }

    fn can_merge_with(&self, next: &Self) -> bool {
        core::ptr::eq(self.source, next.source)
            && self.end == next.start
            && self.style == next.style
            && self.link == next.link
            && self.x.saturating_add(self.width) == next.x
            && !self.hyphen
            && (self.word_spacing.is_none()
                || next.word_spacing.is_none()
                || self.word_spacing == next.word_spacing)
    }

    /// a single collapsed space between words, the gap justification stretches
    fn is_gap(&self) -> bool {
        self.text() == " "
    }

    fn is_whitespace(&self) -> bool {
        self.text().chars().all(is_break_whitespace)
    }

    fn merge(&mut self, next: Self) {
        debug_assert!(self.can_merge_with(&next));

        self.end = next.end;
        self.width = self.width.saturating_add(next.width);
        self.word_spacing = self.word_spacing.or(next.word_spacing);
        self.hyphen = next.hyphen;
    }
}

/// why a line ended. Only wrapped lines are justified, so the last line of a
/// paragraph, or one before a `<br>` or an image, keeps its natural spacing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineEnd {
    Wrapped,
    Last,
}

struct RunSegment<'a> {
    block_start: usize,
    block_end: usize,
    source: &'a str,
    style: TextStyle,
    link: Option<&'a LinkTarget>,
}

struct WordSlice<'a> {
    source: &'a str,
    start: usize,
    end: usize,
    style: TextStyle,
    link: Option<&'a LinkTarget>,
    block_start: usize,
}

const SOFT_HYPHEN: char = '\u{00AD}';
const LIST_BULLET: &str = "• ";

#[derive(Debug, Clone, Copy)]
struct ListMarker {
    style: TextStyle,
    width: u32,
}

struct Paginator<'chapter, 'context, M> {
    chapter: &'chapter Chapter,
    styles: &'context ChapterStyles,
    viewport: Viewport,
    settings: ReaderSettings,
    measurer: &'context mut M,

    pages: Vec<Page<'chapter>>,
    page_items: Vec<PageItem<'chapter>>,

    page_start: ReadingPosition,
    cursor: ReadingPosition,
    used_height: u32,

    line_active: bool,
    line_start: ReadingPosition,
    line_width: u32,
    line_height: u32,
    /// the height of the last line laid out, which sizes extra block spacing
    last_line_height: u32,
    /// how far below the line's top the text starts: half the space the line
    /// adds around the font's own height, like CSS half-leading
    line_text_offset: u32,
    line_text_height: u32,
    line_align: TextAlign,
    line_indent: u32,
    line_list_marker: Option<ListMarker>,
    line_items: Vec<PendingText<'chapter>>,
    /// reused to measure a word's start with its hyphen
    hyphen_scratch: String,
    /// reused scratch buffers for cross-run line breaking within a block
    block_text: String,
    run_segments: Vec<RunSegment<'chapter>>,
    hidden_runs: Vec<(usize, &'chapter str)>,

    block_laid_out: bool,
    block_align: TextAlign,
    block_text_indent: u32,
    block_hanging_indent: u32,
    block_list_marker: Option<ListMarker>,
    block_first_line: bool,
    block_line_height: LineHeight,
}

impl<'chapter, 'context, M> Paginator<'chapter, 'context, M>
where
    M: TextMeasurer + ImageMeasurer,
{
    fn new(
        chapter: &'chapter Chapter,
        styles: &'context ChapterStyles,
        spine: SpineIndex,
        viewport: Viewport,
        settings: ReaderSettings,
        measurer: &'context mut M,
    ) -> Self {
        let start = ReadingPosition::new(BookLocation::new(spine, ContentOffset::ZERO), 0);

        Self {
            chapter,
            styles,
            viewport,
            settings,
            measurer,
            pages: Vec::new(),
            page_items: Vec::new(),
            page_start: start,
            cursor: start,
            used_height: 0,
            line_active: false,
            line_start: start,
            line_width: 0,
            line_height: 0,
            last_line_height: 0,
            line_text_offset: 0,
            line_text_height: 0,
            line_align: TextAlign::Start,
            line_indent: 0,
            line_list_marker: None,
            line_items: Vec::new(),
            hyphen_scratch: String::new(),
            block_text: String::new(),
            run_segments: Vec::new(),
            hidden_runs: Vec::new(),
            block_laid_out: false,
            block_align: TextAlign::Start,
            block_text_indent: 0,
            block_hanging_indent: 0,
            block_list_marker: None,
            block_first_line: false,
            block_line_height: LineHeight::NORMAL,
        }
    }

    fn paginate(mut self) -> Result<Pagination<'chapter>, M::Error> {
        for (index, block) in self.chapter.blocks().iter().enumerate() {
            self.layout_block(block)?;

            if let Some(next) = self.chapter.blocks().get(index + 1) {
                self.add_block_spacing(block, next);
            }
        }

        self.flush_line(LineEnd::Last);

        let end = self.cursor;

        debug_assert_eq!(end.location().offset(), self.chapter.content_len(),);

        if self.used_height > 0 {
            self.push_page(end);
        } else if self.pages.is_empty() {
            self.pages.push(Page::new(
                self.page_start,
                end,
                mem::take(&mut self.page_items),
            ));
        } else if self.page_start < end
            && let Some(last) = self.pages.last_mut()
        {
            last.set_end(end);
        }

        Ok(Pagination { pages: self.pages })
    }

    fn layout_block(&mut self, block: &'chapter ChapterBlock) -> Result<(), M::Error> {
        self.block_laid_out = false;

        let block_style = self.computed_style(block.style_node());

        self.block_align = self.resolve_block_align(block.kind(), block_style);
        self.block_text_indent = self.resolve_text_indent(block.kind(), block_style)?;
        self.block_list_marker = self.resolve_list_marker(block.kind(), block_style)?;
        self.block_hanging_indent = self.block_list_marker.map_or(0, |marker| marker.width);
        self.block_first_line = true;
        self.block_line_height = block_style.line_height();

        self.block_text.clear();
        self.run_segments.clear();
        self.hidden_runs.clear();

        for inline in block.inlines() {
            match inline {
                Inline::Text(text) => {
                    self.push_block_text_run(block.kind(), text);
                }
                Inline::Image(image) => {
                    self.flush_block_text_runs()?;
                    self.layout_image(image);
                    self.cursor = self.cursor.advance_non_text();
                }
                Inline::Break => {
                    self.flush_block_text_runs()?;
                    if !block_style.hidden() {
                        self.layout_break(block.kind(), block_style)?;
                    }
                    self.cursor = self.cursor.advance_non_text();
                }
                Inline::Anchor(_) => {}
            }
        }

        self.flush_block_text_runs()?;
        self.flush_line(LineEnd::Last);

        self.block_align = TextAlign::Start;
        self.block_text_indent = 0;
        self.block_hanging_indent = 0;
        self.block_list_marker = None;
        self.block_first_line = false;
        self.block_line_height = LineHeight::NORMAL;

        Ok(())
    }

    fn push_block_text_run(&mut self, block_kind: BlockKind, run: &'chapter TextRun) {
        let computed = self.computed_style(run.style_node());

        if computed.hidden() {
            self.hidden_runs.push((self.block_text.len(), run.text()));
            return;
        }

        if run.text().is_empty() {
            return;
        }

        let style = TextStyle::new(
            self.settings.font_size(),
            block_kind,
            computed.font_weight(),
            computed.font_style(),
        );

        let block_start = self.block_text.len();
        self.block_text.push_str(run.text());
        let block_end = self.block_text.len();

        self.run_segments.push(RunSegment {
            block_start,
            block_end,
            source: run.text(),
            style,
            link: run.link(),
        });
    }

    fn flush_block_text_runs(&mut self) -> Result<(), M::Error> {
        if self.block_text.is_empty() && self.hidden_runs.is_empty() {
            return Ok(());
        }

        let block_text = mem::take(&mut self.block_text);
        let run_segments = mem::take(&mut self.run_segments);
        let hidden_runs = mem::take(&mut self.hidden_runs);

        let result = self.layout_text_runs(&block_text, &run_segments, &hidden_runs);

        self.block_text = block_text;
        self.block_text.clear();
        self.run_segments = run_segments;
        self.run_segments.clear();
        self.hidden_runs = hidden_runs;
        self.hidden_runs.clear();

        result
    }

    fn layout_text_runs(
        &mut self,
        block_text: &str,
        segments: &[RunSegment<'chapter>],
        hidden_runs: &[(usize, &'chapter str)],
    ) -> Result<(), M::Error> {
        let align = self.block_align;
        let mut hidden_cursor = 0usize;
        let mut seg_cursor = 0usize;
        let mut start = 0usize;
        let mut cluster: Vec<WordSlice<'chapter>> = Vec::new();

        // newlines in a run are collapsible whitespace, so mandatory breaks are
        // treated like any other break opportunity.
        // Coalesce segments split by U+00AD soft hyphen so the whole word is
        // measured without soft hyphens and only breaks with a visible '-'
        // when it does not fit on the current line.
        for (end, _) in linebreaks(block_text) {
            if end < block_text.len() && block_text[start..end].ends_with(SOFT_HYPHEN) {
                continue;
            }

            let content_start = skip_break_whitespace(block_text, start, end);
            let content_end = trim_break_whitespace(block_text, content_start, end);

            if start < content_start {
                self.layout_whitespace_range(
                    start,
                    content_start,
                    segments,
                    &mut seg_cursor,
                    hidden_runs,
                    &mut hidden_cursor,
                    align,
                )?;
            }

            if content_start < content_end {
                collect_word_slices(
                    content_start,
                    content_end,
                    segments,
                    &mut seg_cursor,
                    &mut cluster,
                );
                self.layout_word_cluster(&cluster, hidden_runs, &mut hidden_cursor, align)?;
            }

            if content_end < end {
                self.layout_whitespace_range(
                    content_end,
                    end,
                    segments,
                    &mut seg_cursor,
                    hidden_runs,
                    &mut hidden_cursor,
                    align,
                )?;
            }

            start = end;
        }

        self.advance_hidden_up_to(block_text.len(), hidden_runs, &mut hidden_cursor);

        Ok(())
    }

    fn advance_hidden_up_to(
        &mut self,
        block_pos: usize,
        hidden_runs: &[(usize, &'chapter str)],
        hidden_cursor: &mut usize,
    ) {
        while let Some(&(offset, text)) = hidden_runs.get(*hidden_cursor) {
            if offset > block_pos {
                break;
            }

            self.cursor = self.cursor.advance_text(text);
            *hidden_cursor += 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_whitespace_range(
        &mut self,
        mut start: usize,
        end: usize,
        segments: &[RunSegment<'chapter>],
        seg_cursor: &mut usize,
        hidden_runs: &[(usize, &'chapter str)],
        hidden_cursor: &mut usize,
        align: TextAlign,
    ) -> Result<(), M::Error> {
        while start < end {
            while *seg_cursor < segments.len() && segments[*seg_cursor].block_end <= start {
                *seg_cursor += 1;
            }

            let seg = &segments[*seg_cursor];
            let piece_end = end.min(seg.block_end);
            let local_start = start - seg.block_start;
            let local_end = piece_end - seg.block_start;

            self.advance_hidden_up_to(start, hidden_runs, hidden_cursor);
            self.layout_whitespace(seg.source, local_start, local_end, seg.style, seg.link, align)?;

            start = piece_end;
        }

        Ok(())
    }

    fn layout_word_cluster(
        &mut self,
        cluster: &[WordSlice<'chapter>],
        hidden_runs: &[(usize, &'chapter str)],
        hidden_cursor: &mut usize,
        align: TextAlign,
    ) -> Result<(), M::Error> {
        if let [only] = cluster {
            self.advance_hidden_up_to(only.block_start, hidden_runs, hidden_cursor);
            return self.layout_word(
                only.source,
                only.start,
                only.end,
                only.style,
                only.link,
                align,
            );
        }

        let mut start_idx = 0usize;
        let mut start_offset = cluster[0].start;

        loop {
            let mut total_width = 0u32;
            for (i, slice) in cluster.iter().enumerate().skip(start_idx) {
                let s = if i == start_idx { start_offset } else { slice.start };
                let piece = &slice.source[s..slice.end];
                total_width =
                    total_width.saturating_add(self.measure_visible_text(piece, slice.style)?);
            }

            let used = if self.line_active {
                self.line_indent.saturating_add(self.line_width)
            } else {
                self.prospective_line_indent()
            };

            let available = self.viewport.width().saturating_sub(used);

            if total_width <= available {
                for (i, slice) in cluster.iter().enumerate().skip(start_idx) {
                    let s = if i == start_idx { start_offset } else { slice.start };
                    let piece = &slice.source[s..slice.end];
                    let width = self.measure_visible_text(piece, slice.style)?;
                    self.advance_hidden_up_to(
                        slice.block_start + (s - slice.start),
                        hidden_runs,
                        hidden_cursor,
                    );
                    self.add_text_piece(
                        slice.source,
                        s,
                        slice.end,
                        width,
                        slice.style,
                        slice.link,
                        align,
                    )?;
                }

                return Ok(());
            }

            if let Some((alpha_idx, split, split_width)) =
                self.cluster_hyphenation_break(cluster, start_idx, start_offset, available)?
            {
                for (i, slice) in cluster[..alpha_idx].iter().enumerate().skip(start_idx) {
                    let s = if i == start_idx { start_offset } else { slice.start };
                    let piece = &slice.source[s..slice.end];
                    let width = self.measure_visible_text(piece, slice.style)?;
                    self.advance_hidden_up_to(
                        slice.block_start + (s - slice.start),
                        hidden_runs,
                        hidden_cursor,
                    );
                    self.add_text_piece(
                        slice.source,
                        s,
                        slice.end,
                        width,
                        slice.style,
                        slice.link,
                        align,
                    )?;
                }

                let alpha = &cluster[alpha_idx];
                let alpha_start = if alpha_idx == start_idx {
                    start_offset
                } else {
                    alpha.start
                };
                self.advance_hidden_up_to(
                    alpha.block_start + (alpha_start - alpha.start),
                    hidden_runs,
                    hidden_cursor,
                );
                self.add_text_piece(
                    alpha.source,
                    alpha_start,
                    split,
                    split_width,
                    alpha.style,
                    alpha.link,
                    align,
                )?;

                if let Some(piece) = self.line_items.last_mut() {
                    piece.hyphen = true;
                }

                self.flush_line(LineEnd::Wrapped);

                if split < alpha.end {
                    start_idx = alpha_idx;
                    start_offset = split;
                } else if alpha_idx + 1 < cluster.len() {
                    start_idx = alpha_idx + 1;
                    start_offset = cluster[start_idx].start;
                } else {
                    return Ok(());
                }
                continue;
            }

            if !self.line_active {
                for (i, slice) in cluster.iter().enumerate().skip(start_idx) {
                    let s = if i == start_idx { start_offset } else { slice.start };
                    self.advance_hidden_up_to(
                        slice.block_start + (s - slice.start),
                        hidden_runs,
                        hidden_cursor,
                    );
                    self.layout_oversized_word(
                        slice.source,
                        s,
                        slice.end,
                        slice.style,
                        slice.link,
                        align,
                    )?;
                }

                return Ok(());
            }

            self.flush_line(LineEnd::Wrapped);
        }
    }

    /// Hyphenates a multi-run cluster (such as `(<em>extraordinary</em>).`)
    /// at soft hyphens or when all alphabetic characters belong to a single
    /// styled run.
    fn cluster_hyphenation_break(
        &mut self,
        cluster: &[WordSlice<'chapter>],
        start_idx: usize,
        start_offset: usize,
        available: u32,
    ) -> Result<Option<(usize, usize, u32)>, M::Error> {
        let mut leading_width = 0u32;
        let mut best_soft: Option<(usize, usize, u32)> = None;

        for (i, slice) in cluster.iter().enumerate().skip(start_idx) {
            let s = if i == start_idx { start_offset } else { slice.start };
            let piece = &slice.source[s..slice.end];

            if leading_width < available && piece.contains(SOFT_HYPHEN) {
                let remaining = available - leading_width;
                if let Some((split, width)) =
                    self.soft_hyphen_break(piece, remaining, slice.style)?
                {
                    best_soft = Some((i, s.saturating_add(split), width));
                }
            }

            leading_width =
                leading_width.saturating_add(self.measure_visible_text(piece, slice.style)?);
            if leading_width >= available {
                break;
            }
        }

        if best_soft.is_some() {
            return Ok(best_soft);
        }

        if self.settings.hyphenation().is_none() {
            return Ok(None);
        }

        let mut alpha_idx = None;
        for (i, slice) in cluster.iter().enumerate().skip(start_idx) {
            let s = if i == start_idx { start_offset } else { slice.start };
            let text = &slice.source[s..slice.end];
            if text.chars().any(char::is_alphabetic) {
                if alpha_idx.is_some() {
                    return Ok(None);
                }
                alpha_idx = Some(i);
            }
        }

        let Some(alpha_idx) = alpha_idx else {
            return Ok(None);
        };

        let mut leading_width = 0u32;
        for (i, slice) in cluster[..alpha_idx].iter().enumerate().skip(start_idx) {
            let s = if i == start_idx { start_offset } else { slice.start };
            let piece = &slice.source[s..slice.end];
            leading_width =
                leading_width.saturating_add(self.measure_visible_text(piece, slice.style)?);
        }

        if leading_width >= available {
            return Ok(None);
        }

        let alpha = &cluster[alpha_idx];
        let alpha_start = if alpha_idx == start_idx {
            start_offset
        } else {
            alpha.start
        };
        let word = &alpha.source[alpha_start..alpha.end];
        let remaining = available - leading_width;

        let Some((split, width)) = self.hyphenation_break(word, remaining, alpha.style)? else {
            return Ok(None);
        };

        Ok(Some((alpha_idx, alpha_start.saturating_add(split), width)))
    }

    fn layout_image(&mut self, image: &'chapter ChapterImage) {
        let computed = self.computed_style(image.style_node());

        if computed.hidden() {
            return;
        }

        self.flush_line(LineEnd::Last);

        let Some(intrinsic) = self.measurer.image_dimensions(image) else {
            return;
        };
        let Some(layout) = fit_image_dimensions(intrinsic, self.viewport) else {
            return;
        };

        let height = layout.height();

        if self.used_height > 0 && self.used_height.saturating_add(height) > self.viewport.height()
        {
            // images consume no ContentOffset. This may intentionally produce a zero-length
            // PageRange for an image-only page.
            self.push_page(self.cursor);
        }

        let x = alignment_offset(computed.text_align(), self.viewport.width(), layout.width());

        let bounds = Rect::new(x, self.used_height, layout.width(), height);

        self.page_items
            .push(PageItem::Image(ImageFragment::new(image, bounds)));
        self.used_height = self.used_height.saturating_add(height);
        self.block_laid_out = true;
    }

    fn layout_whitespace(
        &mut self,
        source: &'chapter str,
        start: usize,
        end: usize,
        style: TextStyle,
        link: Option<&'chapter LinkTarget>,
        align: TextAlign,
    ) -> Result<(), M::Error> {
        let whitespace = &source[start..end];

        if !self.line_active {
            self.cursor = self.cursor.advance_text(whitespace);

            return Ok(());
        }

        let width = self.measurer.measure_text(whitespace, style)?;

        let occupied = self
            .line_indent
            .saturating_add(self.line_width)
            .saturating_add(width);

        if occupied > self.viewport.width() {
            self.flush_line(LineEnd::Wrapped);

            self.cursor = self.cursor.advance_text(whitespace);

            return Ok(());
        }

        self.add_text_piece(source, start, end, width, style, link, align)
    }

    fn layout_word(
        &mut self,
        source: &'chapter str,
        start: usize,
        end: usize,
        style: TextStyle,
        link: Option<&'chapter LinkTarget>,
        align: TextAlign,
    ) -> Result<(), M::Error> {
        let mut start = start;

        loop {
            let word = &source[start..end];

            let width = self.measure_visible_text(word, style)?;

            let used = if self.line_active {
                self.line_indent.saturating_add(self.line_width)
            } else {
                self.prospective_line_indent()
            };

            let available = self.viewport.width().saturating_sub(used);

            if width <= available {
                return self.add_text_piece(source, start, end, width, style, link, align);
            }

            // prefer explicit U+00AD soft hyphens, then algorithmic hyphenation
            let break_point = if word.contains(SOFT_HYPHEN) {
                self.soft_hyphen_break(word, available, style)?
            } else {
                None
            };
            let break_point = match break_point {
                Some(found) => Some(found),
                None => self.hyphenation_break(word, available, style)?,
            };

            // fill the line with the word's start and a hyphen, and carry the
            // rest over to the next line
            if let Some((split, width)) = break_point {
                let split = start.saturating_add(split);

                self.add_text_piece(source, start, split, width, style, link, align)?;

                if let Some(piece) = self.line_items.last_mut() {
                    piece.hyphen = true;
                }

                self.flush_line(LineEnd::Wrapped);

                start = split;
                if start >= end {
                    return Ok(());
                }

                continue;
            }

            if !self.line_active {
                return self.layout_oversized_word(source, start, end, style, link, align);
            }

            self.flush_line(LineEnd::Wrapped);
        }
    }

    fn measure_visible_text(&mut self, text: &str, style: TextStyle) -> Result<u32, M::Error> {
        if !text.contains(SOFT_HYPHEN) {
            return self.measurer.measure_text(text, style);
        }

        let mut scratch = mem::take(&mut self.hyphen_scratch);
        scratch.clear();
        for ch in text.chars() {
            if ch != SOFT_HYPHEN {
                scratch.push(ch);
            }
        }

        let result = self.measurer.measure_text(&scratch, style);
        self.hyphen_scratch = scratch;
        result
    }

    /// The longest prefix of `word` ending at a `U+00AD` soft hyphen that fits
    /// in `available` with a visible `-`.
    fn soft_hyphen_break(
        &mut self,
        word: &str,
        available: u32,
        style: TextStyle,
    ) -> Result<Option<(usize, u32)>, M::Error> {
        let mut scratch = mem::take(&mut self.hyphen_scratch);
        let mut best = None;

        for (index, ch) in word.char_indices() {
            if ch != SOFT_HYPHEN || index == 0 {
                continue;
            }

            scratch.clear();
            for prefix_ch in word[..index].chars() {
                if prefix_ch != SOFT_HYPHEN {
                    scratch.push(prefix_ch);
                }
            }
            if scratch.is_empty() {
                continue;
            }
            scratch.push('-');

            let width = match self.measurer.measure_text(&scratch, style) {
                Ok(width) => width,
                Err(error) => {
                    self.hyphen_scratch = scratch;
                    return Err(error);
                }
            };

            if width > available {
                break;
            }

            let split = index + SOFT_HYPHEN.len_utf8();
            best = Some((split, width));
        }

        self.hyphen_scratch = scratch;
        Ok(best)
    }

    /// The longest start of `word` that fits in `available` with a hyphen, as
    /// its length in bytes and its width with the hyphen. Only the letters
    /// between any leading and trailing punctuation are hyphenated.
    fn hyphenation_break(
        &mut self,
        word: &str,
        available: u32,
        style: TextStyle,
    ) -> Result<Option<(usize, u32)>, M::Error> {
        let Some(language) = self.settings.hyphenation() else {
            return Ok(None);
        };

        let Some(letters_start) = word.find(char::is_alphabetic) else {
            return Ok(None);
        };
        let letters_end = word
            .rfind(char::is_alphabetic)
            .and_then(|index| Some(index + word[index..].chars().next()?.len_utf8()))
            .unwrap_or(word.len());

        let letters = &word[letters_start..letters_end];

        if !letters.chars().all(char::is_alphabetic) {
            return Ok(None);
        }

        let mut scratch = mem::take(&mut self.hyphen_scratch);
        let mut best = None;

        for offset in language.breaks(letters) {
            let split = letters_start + offset;

            scratch.clear();
            scratch.push_str(&word[..split]);
            scratch.push('-');

            let width = match self.measurer.measure_text(&scratch, style) {
                Ok(width) => width,
                Err(error) => {
                    self.hyphen_scratch = scratch;

                    return Err(error);
                }
            };

            // a longer start is never narrower
            if width > available {
                break;
            }

            best = Some((split, width));
        }

        self.hyphen_scratch = scratch;

        Ok(best)
    }

    fn layout_oversized_word(
        &mut self,
        source: &'chapter str,
        word_start: usize,
        word_end: usize,
        style: TextStyle,
        link: Option<&'chapter LinkTarget>,
        align: TextAlign,
    ) -> Result<(), M::Error> {
        let word = &source[word_start..word_end];

        let mut start = 0usize;

        while start < word.len() {
            let Some(first_end) = self.next_boundary(word, start, style)? else {
                break;
            };

            let indent = self.prospective_line_indent();

            let available = self
                .viewport
                .width()
                .saturating_sub(indent)
                .saturating_sub(self.line_width);

            let mut best = None;
            let mut end = first_end;

            loop {
                let candidate = &word[start..end];

                let width = self.measure_visible_text(candidate, style)?;

                if width <= available {
                    best = Some((end, width));
                } else {
                    break;
                }

                if end == word.len() {
                    break;
                }

                let Some(next) = self.next_boundary(word, end, style)? else {
                    break;
                };

                end = next;
            }

            let (end, width) = match best {
                Some(best) => best,
                None => {
                    let candidate = &word[start..first_end];

                    (first_end, self.measure_visible_text(candidate, style)?)
                }
            };

            let source_start = word_start.saturating_add(start);
            let source_end = word_start.saturating_add(end);

            self.add_text_piece(source, source_start, source_end, width, style, link, align)?;

            start = end;

            if start < word.len() {
                self.flush_line(LineEnd::Wrapped);
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn add_text_piece(
        &mut self,
        source: &'chapter str,
        start: usize,
        end: usize,
        width: u32,
        style: TextStyle,
        link: Option<&'chapter LinkTarget>,
        align: TextAlign,
    ) -> Result<(), M::Error> {
        let text = &source[start..end];

        if text.is_empty() {
            return Ok(());
        }

        if !self.line_active {
            let indent = self.prospective_line_indent();

            self.line_active = true;
            self.line_start = self.cursor;
            self.line_align = align;
            self.line_indent = indent;
            if self.block_first_line {
                self.line_list_marker = self.block_list_marker.take();
            }
            self.block_first_line = false;
        }

        let height = self.measured_line_height(style)?;
        let text_height = self.measurer.line_height(style)?.max(1);

        self.line_items.push(PendingText {
            source,
            start,
            end,
            style,
            link,
            x: self.line_width,
            width,
            word_spacing: text.contains(' ').then_some(0),
            hyphen: false,
        });

        self.line_width = self.line_width.saturating_add(width);
        self.line_height = self.line_height.max(height);
        self.line_text_offset = self
            .line_text_offset
            .max(height.saturating_sub(text_height) / 2);
        self.line_text_height = self.line_text_height.max(text_height);
        self.cursor = self.cursor.advance_text(text);
        self.block_laid_out = true;

        Ok(())
    }

    fn layout_break(
        &mut self,
        block_kind: BlockKind,
        computed: ComputedStyle,
    ) -> Result<(), M::Error> {
        if self.line_active {
            self.flush_line(LineEnd::Last);

            return Ok(());
        }

        let style = TextStyle::new(
            self.settings.font_size(),
            block_kind,
            computed.font_weight(),
            computed.font_style(),
        );

        let indent = self.prospective_line_indent();

        self.line_active = true;
        self.line_start = self.cursor;
        self.line_align = self.block_align;
        self.line_indent = indent;
        self.block_first_line = false;
        self.line_height = self.measured_line_height(style)?;
        self.block_laid_out = true;

        self.flush_line(LineEnd::Last);

        Ok(())
    }

    fn flush_line(&mut self, end: LineEnd) {
        if !self.line_active {
            return;
        }

        // spaces at the end of a line take no room
        while let Some(item) = self.line_items.pop_if(|item| item.is_whitespace()) {
            self.line_width = self.line_width.saturating_sub(item.width);
        }

        let height = self.line_height.max(1);

        if self.used_height > 0 && self.used_height.saturating_add(height) > self.viewport.height()
        {
            self.push_page(self.line_start);
        }

        let y = self.used_height;
        let text_y = y.saturating_add(self.line_text_offset);
        let text_height = self.line_text_height.max(1);

        let line_width = self.viewport.width().saturating_sub(self.line_indent);

        if end == LineEnd::Wrapped && self.line_align == TextAlign::Justify {
            justify(
                &mut self.line_items,
                line_width.saturating_sub(self.line_width),
            );
        }

        let alignment = alignment_offset(self.line_align, line_width, self.line_width);
        let origin = self.line_indent.saturating_add(alignment);

        if let Some(marker) = self.line_list_marker.take() {
            let marker_x = origin.saturating_sub(marker.width);
            self.page_items.push(PageItem::Text(TextFragment::new(
                Cow::Borrowed(LIST_BULLET),
                Rect::new(marker_x, text_y, marker.width, text_height),
                marker.style,
                None,
                0,
                false,
            )));
        }

        let mut items = mem::take(&mut self.line_items).into_iter();

        if let Some(mut current) = items.next() {
            for next in items {
                if current.can_merge_with(&next) {
                    current.merge(next);
                    continue;
                }

                self.push_text_fragment(current, origin, text_y, text_height);
                current = next;
            }

            self.push_text_fragment(current, origin, text_y, text_height);
        }

        self.used_height = self.used_height.saturating_add(height);
        self.last_line_height = height;
        self.line_active = false;
        self.line_start = self.cursor;
        self.line_width = 0;
        self.line_height = 0;
        self.line_text_offset = 0;
        self.line_text_height = 0;
        self.line_align = TextAlign::Start;
        self.line_indent = 0;
        self.line_list_marker = None;
    }

    fn push_text_fragment(
        &mut self,
        item: PendingText<'chapter>,
        origin: u32,
        y: u32,
        height: u32,
    ) {
        let raw = item.text();
        let text = if item.hyphen || raw.contains(SOFT_HYPHEN) {
            let mut owned = String::with_capacity(raw.len() + usize::from(item.hyphen));
            for ch in raw.chars() {
                if ch != SOFT_HYPHEN {
                    owned.push(ch);
                }
            }
            if item.hyphen {
                owned.push('-');
            }
            Cow::Owned(owned)
        } else {
            Cow::Borrowed(raw)
        };

        self.page_items.push(PageItem::Text(TextFragment::new(
            text,
            Rect::new(origin.saturating_add(item.x), y, item.width, height),
            item.style,
            item.link,
            item.word_spacing.unwrap_or(0),
            item.hyphen,
        )));
    }

    fn add_block_spacing(&mut self, current: &ChapterBlock, next: &ChapterBlock) {
        if !self.block_laid_out {
            return;
        }

        let current = self.computed_style(current.style_node());
        let next = self.computed_style(next.style_node());

        let spacing = match (current.margin_bottom(), next.margin_top()) {
            (None, None) => u32::from(self.settings.block_spacing()),
            (Some(margin), None) | (None, Some(margin)) => self.resolve_block_length(margin),
            (Some(bottom), Some(top)) => self
                .resolve_block_length(bottom)
                .max(self.resolve_block_length(top)),
        };

        let spacing = if self.settings.extra_block_spacing() {
            spacing.saturating_add(self.last_line_height / 2)
        } else {
            spacing
        };

        if spacing == 0 || self.used_height == 0 {
            return;
        }

        if self.used_height.saturating_add(spacing) > self.viewport.height() {
            self.push_page(self.cursor);

            return;
        }

        self.used_height = self.used_height.saturating_add(spacing);
    }

    fn push_page(&mut self, end: ReadingPosition) {
        self.pages.push(Page::new(
            self.page_start,
            end,
            mem::take(&mut self.page_items),
        ));

        self.page_start = end;
        self.used_height = 0;
    }

    fn computed_style(&self, node: StyleNodeId) -> ComputedStyle {
        self.styles.style(node).unwrap_or_default()
    }

    fn measured_line_height(&mut self, style: TextStyle) -> Result<u32, M::Error> {
        let font_size = u32::from(style.font_size());
        let native = self.measurer.line_height(style)?.max(1);

        if let Some(percent) = self.settings.line_height_percent() {
            return Ok((font_size * u32::from(percent) / 100).max(native));
        }

        if let Some(height) = self.block_line_height.resolve(font_size) {
            return Ok(height.max(native));
        }

        Ok(native)
    }

    fn next_boundary(
        &mut self,
        text: &str,
        from: usize,
        style: TextStyle,
    ) -> Result<Option<usize>, M::Error> {
        let candidate = self.measurer.next_boundary(text, from, style)?;

        if let Some(candidate) = candidate
            && candidate > from
            && candidate <= text.len()
            && text.is_char_boundary(candidate)
        {
            return Ok(Some(candidate));
        }

        Ok(scalar_boundary(text, from))
    }

    fn prospective_line_indent(&self) -> u32 {
        if self.line_active {
            self.line_indent
        } else if self.block_first_line {
            self.block_hanging_indent
                .saturating_add(self.block_text_indent)
                .min(self.viewport.width().saturating_sub(1))
        } else {
            self.block_hanging_indent
        }
    }

    fn resolve_list_marker(
        &mut self,
        kind: BlockKind,
        style: ComputedStyle,
    ) -> Result<Option<ListMarker>, M::Error> {
        if kind != BlockKind::ListItem || style.hidden() {
            return Ok(None);
        }

        let marker_style = TextStyle::new(
            self.settings.font_size(),
            kind,
            FontWeight::Normal,
            FontStyle::Normal,
        );
        let marker_width = self
            .measurer
            .measure_text(LIST_BULLET, marker_style)?
            .min(self.viewport.width().saturating_sub(1));

        Ok(Some(ListMarker {
            style: marker_style,
            width: marker_width,
        }))
    }

    fn resolve_text_indent(
        &mut self,
        kind: BlockKind,
        style: ComputedStyle,
    ) -> Result<u32, M::Error> {
        let resolved = match self.settings.paragraph_indent_spaces() {
            Some(spaces) if kind == BlockKind::Paragraph => {
                let natural = matches!(
                    self.block_align,
                    TextAlign::Start | TextAlign::Left | TextAlign::Justify
                );

                if spaces == 0 || !natural {
                    0
                } else {
                    let space = TextStyle::new(
                        self.settings.font_size(),
                        kind,
                        FontWeight::Normal,
                        FontStyle::Normal,
                    );

                    self.measurer
                        .measure_text(" ", space)?
                        .saturating_mul(u32::from(spaces))
                }
            }

            _ => {
                let resolved = style
                    .text_indent()
                    .resolve(u32::from(self.settings.font_size()), self.viewport.width());

                u32::try_from(resolved.max(0)).unwrap_or(u32::MAX)
            }
        };

        // always leave at least one horizontal pixel available.
        Ok(resolved.min(self.viewport.width().saturating_sub(1)))
    }

    fn resolve_block_align(&self, kind: BlockKind, style: ComputedStyle) -> TextAlign {
        if matches!(kind, BlockKind::Heading(_)) {
            return style.text_align();
        }

        self.settings
            .paragraph_align()
            .or(style.declared_text_align())
            .unwrap_or(self.settings.undeclared_paragraph_align())
    }

    fn resolve_block_length(&self, length: CssLength) -> u32 {
        let resolved = length.resolve(u32::from(self.settings.font_size()), self.viewport.width());

        u32::try_from(resolved.max(0)).unwrap_or(u32::MAX)
    }
}

fn collect_word_slices<'a>(
    mut start: usize,
    end: usize,
    segments: &[RunSegment<'a>],
    seg_cursor: &mut usize,
    out: &mut Vec<WordSlice<'a>>,
) {
    out.clear();

    while start < end {
        while *seg_cursor < segments.len() && segments[*seg_cursor].block_end <= start {
            *seg_cursor += 1;
        }

        let seg = &segments[*seg_cursor];
        let piece_end = end.min(seg.block_end);

        out.push(WordSlice {
            source: seg.source,
            start: start - seg.block_start,
            end: piece_end - seg.block_start,
            style: seg.style,
            link: seg.link,
            block_start: start,
        });

        start = piece_end;
    }
}

/// spreads `spare` pixels over a line's gaps, the leftover pixels going to the
/// first gaps. A line without gaps, or one whose gaps would stretch beyond 3×
/// their natural width (creating wide rivers), keeps its natural spacing.
fn justify(items: &mut [PendingText<'_>], spare: u32) {
    let mut gaps = 0u32;
    let mut max_total_stretch = 0u64;

    for item in items.iter() {
        if !item.is_gap() {
            continue;
        }
        gaps = gaps.saturating_add(1);
        max_total_stretch = max_total_stretch.saturating_add(u64::from(item.width) * 2);
    }

    if gaps == 0 || spare == 0 || u64::from(spare) > max_total_stretch {
        return;
    }

    let extra = spare / gaps;
    let mut wider = spare % gaps;
    let mut shift = 0u32;

    for item in items {
        item.x = item.x.saturating_add(shift);

        if !item.is_gap() {
            continue;
        }

        let stretch = if wider > 0 {
            wider -= 1;
            extra.saturating_add(1)
        } else {
            extra
        };

        item.width = item.width.saturating_add(stretch);
        item.word_spacing = Some(stretch);
        shift = shift.saturating_add(stretch);
    }
}

fn alignment_offset(align: TextAlign, container_width: u32, content_width: u32) -> u32 {
    let remaining = container_width.saturating_sub(content_width);

    match align {
        TextAlign::Center => remaining / 2,
        TextAlign::End | TextAlign::Right => remaining,
        TextAlign::Start | TextAlign::Left | TextAlign::Justify => 0,
    }
}

fn fit_image_dimensions(intrinsic: ImageDimensions, viewport: Viewport) -> Option<ImageDimensions> {
    let mut width = intrinsic.width();
    let mut height = intrinsic.height();

    if width == 0 || height == 0 {
        return None;
    }

    // never upscale. First fit width.
    if width > viewport.width() {
        height = scale_dimension(height, viewport.width(), width);
        width = viewport.width();
    }

    // very tall images must still fit on one reader page.
    if height > viewport.height() {
        width = scale_dimension(width, viewport.height(), height);
        height = viewport.height();
    }

    Some(ImageDimensions::new(width.max(1), height.max(1)))
}

fn scale_dimension(value: u32, numerator: u32, denominator: u32) -> u32 {
    if denominator == 0 {
        return 0;
    }

    let scaled = u64::from(value).saturating_mul(u64::from(numerator)) / u64::from(denominator);

    u32::try_from(scaled).unwrap_or(u32::MAX).max(1)
}

fn scalar_boundary(text: &str, from: usize) -> Option<usize> {
    if from >= text.len() || !text.is_char_boundary(from) {
        return None;
    }

    let character = text[from..].chars().next()?;

    Some(from.saturating_add(character.len_utf8()))
}

// non-breaking spaces are laid out as part of the surrounding word
fn is_break_whitespace(character: char) -> bool {
    character.is_whitespace() && !matches!(character, '\u{00A0}' | '\u{202F}')
}

fn skip_break_whitespace(text: &str, mut start: usize, end: usize) -> usize {
    while let Some(character) = text[start..end].chars().next() {
        if !is_break_whitespace(character) {
            break;
        }

        start += character.len_utf8();
    }

    start
}

fn trim_break_whitespace(text: &str, start: usize, mut end: usize) -> usize {
    while let Some(character) = text[start..end].chars().next_back() {
        if !is_break_whitespace(character) {
            break;
        }

        end -= character.len_utf8();
    }

    end
}
