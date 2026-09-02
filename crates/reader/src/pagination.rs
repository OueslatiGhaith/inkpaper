use alloc::vec::Vec;

use inkpaper_epub::{
    BlockKind, BookLocation, Chapter, ChapterBlock, ChapterStyles, ComputedStyle, ContentOffset,
    Inline, SpineIndex, StyleNodeId, TextRun,
};

use crate::{ReaderSettings, TextMeasurer, TextStyle, Viewport};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRange {
    start: BookLocation,
    end: BookLocation,
}

impl PageRange {
    pub const fn new(start: BookLocation, end: BookLocation) -> Self {
        Self { start, end }
    }

    pub const fn start(self) -> BookLocation {
        self.start
    }

    pub const fn end(self) -> BookLocation {
        self.end
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pagination {
    pages: Vec<PageRange>,
}

impl Pagination {
    pub fn pages(&self) -> &[PageRange] {
        &self.pages
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }
}

pub fn paginate_chapter<M>(
    chapter: &Chapter,
    styles: &ChapterStyles,
    spine: SpineIndex,
    viewport: Viewport,
    settings: ReaderSettings,
    measurer: &mut M,
) -> Pagination
where
    M: TextMeasurer,
{
    Paginator::new(chapter, styles, spine, viewport, settings, measurer).paginate()
}

struct Paginator<'a, M> {
    chapter: &'a Chapter,
    styles: &'a ChapterStyles,
    spine: SpineIndex,
    viewport: Viewport,
    settings: ReaderSettings,
    measurer: &'a mut M,

    pages: Vec<PageRange>,

    page_start: ContentOffset,
    cursor: ContentOffset,
    used_height: u32,

    line_active: bool,
    line_start: ContentOffset,
    line_width: u32,
    line_height: u32,

    block_laid_out: bool,
}

impl<'a, M> Paginator<'a, M>
where
    M: TextMeasurer,
{
    fn new(
        chapter: &'a Chapter,
        styles: &'a ChapterStyles,
        spine: SpineIndex,
        viewport: Viewport,
        settings: ReaderSettings,
        measurer: &'a mut M,
    ) -> Self {
        Self {
            chapter,
            styles,
            spine,
            viewport,
            settings,
            measurer,
            pages: Vec::new(),
            page_start: ContentOffset::ZERO,
            cursor: ContentOffset::ZERO,
            used_height: 0,
            line_active: false,
            line_start: ContentOffset::ZERO,
            line_width: 0,
            line_height: 0,
            block_laid_out: false,
        }
    }

    fn paginate(mut self) -> Pagination {
        for (index, block) in self.chapter.blocks().iter().enumerate() {
            self.layout_block(block);

            if index + 1 < self.chapter.blocks().len() {
                self.add_block_spacing();
            }
        }

        self.flush_line();

        let end = self.chapter.content_len();

        debug_assert_eq!(self.cursor, end,);

        if self.used_height > 0 {
            self.push_page(end);
        } else if self.pages.is_empty() {
            self.pages.push(PageRange::new(
                BookLocation::new(self.spine, ContentOffset::ZERO),
                BookLocation::new(self.spine, end),
            ));
        } else if self.page_start < end {
            // this can happen when only hidden text follows the last visible page. Hidden
            // content still contributes to the canonical content offset, so extend
            // the last page's range rather than creating an empty visual page.
            if let Some(last) = self.pages.last_mut() {
                last.end = BookLocation::new(self.spine, end);
            }
        }

        Pagination { pages: self.pages }
    }

    fn layout_block(&mut self, block: &ChapterBlock) {
        self.block_laid_out = false;

        let block_style = self.computed_style(block.style_node());

        for inline in block.inlines() {
            match inline {
                Inline::Text(text) => {
                    self.layout_text(block.kind(), text);
                }
                Inline::Break => {
                    if !block_style.hidden() {
                        self.layout_break(block.kind(), block_style);
                    }
                }
                // images deliberately do not affect pagination yet. Their canonical text offset
                // is already zero, and image measurement will be added through
                // a separate reader measurement path.
                Inline::Image(_) => {}
                Inline::Anchor(_) => {}
            }
        }

        self.flush_line();
    }

    fn layout_text(&mut self, block_kind: BlockKind, run: &TextRun) {
        let computed = self.computed_style(run.style_node());

        if computed.hidden() {
            self.cursor = self.cursor.advance_text(run.text());

            return;
        }

        let style = TextStyle::new(
            self.settings.font_size(),
            block_kind,
            computed.font_weight(),
            computed.font_style(),
        );

        self.layout_text_content(run.text(), style);
    }

    fn layout_text_content(&mut self, text: &str, style: TextStyle) {
        let mut start = 0usize;

        while start < text.len() {
            let Some(first) = text[start..].chars().next() else {
                break;
            };

            let whitespace = first.is_whitespace();

            let mut end = text.len();

            let first_end = start.saturating_add(first.len_utf8());

            for (relative, character) in text[first_end..].char_indices() {
                if character.is_whitespace() != whitespace {
                    end = first_end.saturating_add(relative);

                    break;
                }
            }

            let segment = &text[start..end];

            if whitespace {
                self.layout_whitespace(segment, style);
            } else {
                self.layout_word(segment, style);
            }

            start = end;
        }
    }

    fn layout_whitespace(&mut self, whitespace: &str, style: TextStyle) {
        if !self.line_active {
            self.cursor = self.cursor.advance_text(whitespace);

            return;
        }

        let width = self.measurer.measure_text(whitespace, style);

        if self.line_width.saturating_add(width) > self.viewport.width() {
            self.flush_line();

            self.cursor = self.cursor.advance_text(whitespace);

            return;
        }

        self.line_width = self.line_width.saturating_add(width);
        self.line_height = self.line_height.max(self.measured_line_height(style));
        self.cursor = self.cursor.advance_text(whitespace);
    }

    fn layout_word(&mut self, word: &str, style: TextStyle) {
        let width = self.measurer.measure_text(word, style);

        if self.line_active && self.line_width.saturating_add(width) > self.viewport.width() {
            self.flush_line();
        }

        if width <= self.viewport.width() {
            self.add_text_piece(word, width, style);

            return;
        }

        self.layout_oversized_word(word, style);
    }

    fn layout_oversized_word(&mut self, word: &str, style: TextStyle) {
        let mut start = 0usize;

        while start < word.len() {
            let Some(first_end) = self.next_boundary(word, start, style) else {
                break;
            };

            let available = self.viewport.width().saturating_sub(self.line_width);

            let mut best = None;
            let mut end = first_end;

            loop {
                let candidate = &word[start..end];
                let width = self.measurer.measure_text(candidate, style);

                if width <= available {
                    best = Some((end, width));
                } else {
                    break;
                }

                if end == word.len() {
                    break;
                }

                let Some(next) = self.next_boundary(word, end, style) else {
                    break;
                };

                end = next;
            }

            let (end, width) = match best {
                Some(best) => best,
                None => {
                    let candidate = &word[start..first_end];
                    (first_end, self.measurer.measure_text(candidate, style))
                }
            };

            let piece = &word[start..end];

            self.add_text_piece(piece, width, style);

            start = end;

            if start < word.len() {
                self.flush_line();
            }
        }
    }

    fn add_text_piece(&mut self, text: &str, width: u32, style: TextStyle) {
        if text.is_empty() {
            return;
        }

        if !self.line_active {
            self.line_active = true;
            self.line_start = self.cursor;
        }

        self.line_width = self.line_width.saturating_add(width);
        self.line_height = self.line_height.max(self.measured_line_height(style));
        self.cursor = self.cursor.advance_text(text);
        self.block_laid_out = true;
    }

    fn layout_break(&mut self, block_kind: BlockKind, computed: ComputedStyle) {
        if self.line_active {
            self.flush_line();
            return;
        }

        let style = TextStyle::new(
            self.settings.font_size(),
            block_kind,
            computed.font_weight(),
            computed.font_style(),
        );

        self.line_active = true;
        self.line_start = self.cursor;
        self.line_height = self.measured_line_height(style);
        self.block_laid_out = true;

        self.flush_line();
    }

    fn flush_line(&mut self) {
        if !self.line_active {
            return;
        }

        let height = self.line_height.max(1);

        if self.used_height > 0 && self.used_height.saturating_add(height) > self.viewport.height()
        {
            self.push_page(self.line_start);
        }

        self.used_height = self.used_height.saturating_add(height);
        self.line_active = false;
        self.line_start = self.cursor;
        self.line_width = 0;
        self.line_height = 0;
    }

    fn add_block_spacing(&mut self) {
        if !self.block_laid_out {
            return;
        }

        let spacing = u32::from(self.settings.block_spacing());
        if spacing == 0 || self.used_height == 0 {
            return;
        }

        if self.used_height.saturating_add(spacing) > self.viewport.height() {
            self.push_page(self.cursor);

            return;
        }

        self.used_height = self.used_height.saturating_add(spacing);
    }

    fn push_page(&mut self, end: ContentOffset) {
        self.pages.push(PageRange::new(
            BookLocation::new(self.spine, self.page_start),
            BookLocation::new(self.spine, end),
        ));

        self.page_start = end;
        self.used_height = 0;
    }

    fn computed_style(&self, node: StyleNodeId) -> ComputedStyle {
        self.styles.style(node).unwrap_or_default()
    }

    fn measured_line_height(&mut self, style: TextStyle) -> u32 {
        self.measurer.line_height(style).max(1)
    }

    fn next_boundary(&mut self, text: &str, from: usize, style: TextStyle) -> Option<usize> {
        let candidate = self.measurer.next_boundary(text, from, style);

        if let Some(candidate) = candidate
            && candidate > from
            && candidate <= text.len()
            && text.is_char_boundary(candidate)
        {
            return Some(candidate);
        }

        scalar_boundary(text, from)
    }
}

fn scalar_boundary(text: &str, from: usize) -> Option<usize> {
    if from >= text.len() || !text.is_char_boundary(from) {
        return None;
    }

    let character = text[from..].chars().next()?;

    Some(from.saturating_add(character.len_utf8()))
}
