use alloc::vec::Vec;
use core::mem;

use inkpaper_epub::{
    BlockKind, BookLocation, Chapter, ChapterBlock, ChapterImage, ChapterStyles, ComputedStyle,
    ContentOffset, ImageDimensions, Inline, LinkTarget, SpineIndex, StyleNodeId, TextAlign,
    TextRun,
};

use crate::{
    ImageFragment, ImageMeasurer, Page, PageItem, PageRange, ReaderSettings, Rect, TextFragment,
    TextMeasurer, TextStyle, Viewport,
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
    text: &'a str,
    style: TextStyle,
    link: Option<&'a LinkTarget>,
    x: u32,
    width: u32,
}

struct Paginator<'chapter, 'context, M> {
    chapter: &'chapter Chapter,
    styles: &'context ChapterStyles,
    spine: SpineIndex,
    viewport: Viewport,
    settings: ReaderSettings,
    measurer: &'context mut M,

    pages: Vec<Page<'chapter>>,
    page_items: Vec<PageItem<'chapter>>,

    page_start: ContentOffset,
    cursor: ContentOffset,
    used_height: u32,

    line_active: bool,
    line_start: ContentOffset,
    line_width: u32,
    line_height: u32,
    line_align: TextAlign,
    line_items: Vec<PendingText<'chapter>>,

    block_laid_out: bool,
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
        Self {
            chapter,
            styles,
            spine,
            viewport,
            settings,
            measurer,
            pages: Vec::new(),
            page_items: Vec::new(),
            page_start: ContentOffset::ZERO,
            cursor: ContentOffset::ZERO,
            used_height: 0,
            line_active: false,
            line_start: ContentOffset::ZERO,
            line_width: 0,
            line_height: 0,
            line_align: TextAlign::Start,
            line_items: Vec::new(),
            block_laid_out: false,
        }
    }

    fn paginate(mut self) -> Result<Pagination<'chapter>, M::Error> {
        for (index, block) in self.chapter.blocks().iter().enumerate() {
            self.layout_block(block)?;

            if index + 1 < self.chapter.blocks().len() {
                self.add_block_spacing();
            }
        }

        self.flush_line();

        let end = self.chapter.content_len();

        debug_assert_eq!(self.cursor, end);

        if self.used_height > 0 {
            self.push_page(end);
        } else if self.pages.is_empty() {
            self.pages.push(Page::new(
                PageRange::new(
                    BookLocation::new(self.spine, ContentOffset::ZERO),
                    BookLocation::new(self.spine, end),
                ),
                mem::take(&mut self.page_items),
            ));
        } else if self.page_start < end {
            // hidden trailing content advances the canonical location without creating visual layout.
            if let Some(last) = self.pages.last_mut() {
                last.set_end(BookLocation::new(self.spine, end));
            }
        }

        Ok(Pagination { pages: self.pages })
    }

    fn layout_block(&mut self, block: &'chapter ChapterBlock) -> Result<(), M::Error> {
        self.block_laid_out = false;

        let block_style = self.computed_style(block.style_node());

        for inline in block.inlines() {
            match inline {
                Inline::Text(text) => {
                    self.layout_text(block.kind(), text)?;
                }
                Inline::Image(image) => {
                    self.layout_image(image);
                }
                Inline::Break => {
                    if !block_style.hidden() {
                        self.layout_break(block.kind(), block_style)?;
                    }
                }
                Inline::Anchor(_) => {}
            }
        }

        self.flush_line();

        Ok(())
    }

    fn layout_text(
        &mut self,
        block_kind: BlockKind,
        run: &'chapter TextRun,
    ) -> Result<(), M::Error> {
        let computed = self.computed_style(run.style_node());

        if computed.hidden() {
            self.cursor = self.cursor.advance_text(run.text());

            return Ok(());
        }

        let style = TextStyle::new(
            self.settings.font_size(),
            block_kind,
            computed.font_weight(),
            computed.font_style(),
        );

        self.layout_text_content(run.text(), style, run.link(), computed.text_align())
    }

    fn layout_image(&mut self, image: &'chapter ChapterImage) {
        let computed = self.computed_style(image.style_node());

        if computed.hidden() {
            return;
        }

        self.flush_line();

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

    fn layout_text_content(
        &mut self,
        text: &'chapter str,
        style: TextStyle,
        link: Option<&'chapter LinkTarget>,
        align: TextAlign,
    ) -> Result<(), M::Error> {
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
                self.layout_whitespace(segment, style, link, align)?;
            } else {
                self.layout_word(segment, style, link, align)?;
            }

            start = end;
        }

        Ok(())
    }

    fn layout_whitespace(
        &mut self,
        whitespace: &'chapter str,
        style: TextStyle,
        link: Option<&'chapter LinkTarget>,
        align: TextAlign,
    ) -> Result<(), M::Error> {
        if !self.line_active {
            self.cursor = self.cursor.advance_text(whitespace);

            return Ok(());
        }

        let width = self.measurer.measure_text(whitespace, style)?;

        if self.line_width.saturating_add(width) > self.viewport.width() {
            self.flush_line();

            self.cursor = self.cursor.advance_text(whitespace);

            return Ok(());
        }

        self.add_text_piece(whitespace, width, style, link, align)
    }

    fn layout_word(
        &mut self,
        word: &'chapter str,
        style: TextStyle,
        link: Option<&'chapter LinkTarget>,
        align: TextAlign,
    ) -> Result<(), M::Error> {
        let width = self.measurer.measure_text(word, style)?;

        if self.line_active && self.line_width.saturating_add(width) > self.viewport.width() {
            self.flush_line();
        }

        if width <= self.viewport.width() {
            return self.add_text_piece(word, width, style, link, align);
        }

        self.layout_oversized_word(word, style, link, align)
    }

    fn layout_oversized_word(
        &mut self,
        word: &'chapter str,
        style: TextStyle,
        link: Option<&'chapter LinkTarget>,
        align: TextAlign,
    ) -> Result<(), M::Error> {
        let mut start = 0usize;

        while start < word.len() {
            let Some(first_end) = self.next_boundary(word, start, style)? else {
                break;
            };

            let available = self.viewport.width().saturating_sub(self.line_width);

            let mut best = None;
            let mut end = first_end;

            loop {
                let candidate = &word[start..end];

                let width = self.measurer.measure_text(candidate, style)?;

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

                    (first_end, self.measurer.measure_text(candidate, style)?)
                }
            };

            let piece = &word[start..end];

            self.add_text_piece(piece, width, style, link, align)?;

            start = end;

            if start < word.len() {
                self.flush_line();
            }
        }

        Ok(())
    }

    fn add_text_piece(
        &mut self,
        text: &'chapter str,
        width: u32,
        style: TextStyle,
        link: Option<&'chapter LinkTarget>,
        align: TextAlign,
    ) -> Result<(), M::Error> {
        if text.is_empty() {
            return Ok(());
        }

        if !self.line_active {
            self.line_active = true;
            self.line_start = self.cursor;
            self.line_align = align;
        }

        let height = self.measured_line_height(style)?;

        self.line_items.push(PendingText {
            text,
            style,
            link,
            x: self.line_width,
            width,
        });

        self.line_width = self.line_width.saturating_add(width);
        self.line_height = self.line_height.max(height);
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
            self.flush_line();

            return Ok(());
        }

        let style = TextStyle::new(
            self.settings.font_size(),
            block_kind,
            computed.font_weight(),
            computed.font_style(),
        );

        self.line_active = true;
        self.line_start = self.cursor;
        self.line_align = computed.text_align();
        self.line_height = self.measured_line_height(style)?;
        self.block_laid_out = true;

        self.flush_line();

        Ok(())
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

        let y = self.used_height;

        let offset = alignment_offset(self.line_align, self.viewport.width(), self.line_width);

        for item in mem::take(&mut self.line_items) {
            self.page_items.push(PageItem::Text(TextFragment::new(
                item.text,
                Rect::new(offset.saturating_add(item.x), y, item.width, height),
                item.style,
                item.link,
            )));
        }

        self.used_height = self.used_height.saturating_add(height);
        self.line_active = false;
        self.line_start = self.cursor;
        self.line_width = 0;
        self.line_height = 0;
        self.line_align = TextAlign::Start;
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
        let range = PageRange::new(
            BookLocation::new(self.spine, self.page_start),
            BookLocation::new(self.spine, end),
        );

        self.pages
            .push(Page::new(range, mem::take(&mut self.page_items)));

        self.page_start = end;
        self.used_height = 0;
    }

    fn computed_style(&self, node: StyleNodeId) -> ComputedStyle {
        self.styles.style(node).unwrap_or_default()
    }

    fn measured_line_height(&mut self, style: TextStyle) -> Result<u32, M::Error> {
        Ok(self.measurer.line_height(style)?.max(1))
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
