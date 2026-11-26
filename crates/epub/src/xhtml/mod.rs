use alloc::{string::String, vec, vec::Vec};

use crate::{
    ArchivePath, ChapterStyles, ComputedStyle, FontStyle, FontWeight, PathError,
    location::ContentOffset, path::decode_url_component,
};

mod parser;
mod style_context;

#[cfg(test)]
mod tests;

pub(crate) use parser::parse_xhtml;
pub use style_context::{StyleContext, StyleNode, StyleNodeId, StylesheetSource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
    path: ArchivePath,
    blocks: Vec<ChapterBlock>,
}

impl Chapter {
    /// A chapter from its XHTML outside any book, like a settings preview.
    /// `path` resolves its relative links and images.
    pub fn parse(xhtml: &str, path: ArchivePath) -> Result<(Self, StyleContext), crate::XhtmlError> {
        parse_xhtml(xhtml, path)
    }

    /// Starts building a chapter directly from blocks and styled text runs,
    /// without parsing XHTML or resolving CSS.
    pub const fn builder(path: ArchivePath) -> ChapterBuilder {
        ChapterBuilder::new(path)
    }

    pub fn path(&self) -> &ArchivePath {
        &self.path
    }

    pub fn blocks(&self) -> &[ChapterBlock] {
        &self.blocks
    }

    pub fn content_len(&self) -> ContentOffset {
        let mut offset = ContentOffset::ZERO;

        for block in &self.blocks {
            for inline in block.inlines() {
                if let Inline::Text(text) = inline {
                    offset = offset.advance_text(text.text());
                }
            }
        }

        offset
    }

    pub fn anchor_offset(&self, anchor: &str) -> Option<ContentOffset> {
        let mut offset = ContentOffset::ZERO;

        for block in &self.blocks {
            for inline in block.inlines() {
                match inline {
                    Inline::Text(text) => offset = offset.advance_text(text.text()),
                    Inline::Anchor(candidate) if candidate == anchor => return Some(offset),
                    Inline::Image(_) | Inline::Break | Inline::Anchor(_) => {}
                }
            }
        }

        None
    }

    pub fn contains_offset(&self, offset: ContentOffset) -> bool {
        offset <= self.content_len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterBlock {
    kind: BlockKind,
    style_node: StyleNodeId,
    inlines: Vec<Inline>,
}

impl ChapterBlock {
    pub const fn kind(&self) -> BlockKind {
        self.kind
    }

    pub const fn style_node(&self) -> StyleNodeId {
        self.style_node
    }

    pub fn inlines(&self) -> &[Inline] {
        &self.inlines
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    ListItem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(TextRun),
    Image(ChapterImage),
    Break,
    Anchor(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRun {
    text: String,
    style_node: StyleNodeId,
    link: Option<LinkTarget>,
}

impl TextRun {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub const fn style_node(&self) -> StyleNodeId {
        self.style_node
    }

    pub const fn link(&self) -> Option<&LinkTarget> {
        self.link.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterImage {
    path: ArchivePath,
    alt: Option<String>,
    style_node: StyleNodeId,
    link: Option<LinkTarget>,
}

impl ChapterImage {
    pub fn path(&self) -> &ArchivePath {
        &self.path
    }

    pub fn alt(&self) -> Option<&str> {
        self.alt.as_deref()
    }

    pub const fn style_node(&self) -> StyleNodeId {
        self.style_node
    }

    pub const fn link(&self) -> Option<&LinkTarget> {
        self.link.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    Internal {
        path: ArchivePath,
        fragment: Option<String>,
    },
    External(String),
}

impl LinkTarget {
    pub fn path(&self) -> Option<&ArchivePath> {
        match self {
            Self::Internal { path, .. } => Some(path),
            Self::External(_) => None,
        }
    }

    pub fn fragment(&self) -> Option<&str> {
        match self {
            Self::Internal { fragment, .. } => fragment.as_deref(),
            Self::External(_) => None,
        }
    }

    pub fn external(&self) -> Option<&str> {
        match self {
            Self::External(value) => Some(value),
            Self::Internal { .. } => None,
        }
    }

    pub const fn is_external(&self) -> bool {
        matches!(self, Self::External(_),)
    }

    fn resolve(base: &ArchivePath, reference: &str) -> Result<Self, PathError> {
        if is_external_reference(reference) {
            return Ok(Self::External(String::from(reference)));
        }

        let (resource, fragment) = match reference.split_once('#') {
            Some((resource, fragment)) => (
                resource,
                if fragment.is_empty() {
                    None
                } else {
                    Some(decode_url_component(fragment))
                },
            ),

            None => (reference, None),
        };

        let resource = resource
            .split_once('?')
            .map(|(resource, _)| resource)
            .unwrap_or(resource);

        let path = if resource.is_empty() {
            base.clone()
        } else {
            base.resolve(resource)?
        };

        Ok(Self::Internal { path, fragment })
    }
}

fn is_external_reference(reference: &str) -> bool {
    if reference.starts_with("//") {
        return true;
    }

    let Some(colon) = reference.find(':') else {
        return false;
    };

    let scheme = &reference[..colon];

    if scheme.is_empty() || !scheme.as_bytes()[0].is_ascii_alphabetic() {
        return false;
    }

    scheme
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

const STYLE_NORMAL: StyleNodeId = StyleNodeId::new(0);
const STYLE_BOLD: StyleNodeId = StyleNodeId::new(1);
const STYLE_ITALIC: StyleNodeId = StyleNodeId::new(2);
const STYLE_BOLD_ITALIC: StyleNodeId = StyleNodeId::new(3);

const fn style_node_for(weight: FontWeight, style: FontStyle) -> StyleNodeId {
    match (weight, style) {
        (FontWeight::Normal, FontStyle::Normal) => STYLE_NORMAL,
        (FontWeight::Bold, FontStyle::Normal) => STYLE_BOLD,
        (FontWeight::Normal, FontStyle::Italic) => STYLE_ITALIC,
        (FontWeight::Bold, FontStyle::Italic) => STYLE_BOLD_ITALIC,
    }
}

/// Builds a [`Chapter`] and its [`ChapterStyles`] directly from blocks and
/// styled text runs, skipping XHTML serialization, XML parsing, and CSS
/// cascade resolution.
pub struct ChapterBuilder {
    path: ArchivePath,
    blocks: Vec<ChapterBlock>,
    current: Option<ChapterBlockBuilder>,
}

impl ChapterBuilder {
    pub const fn new(path: ArchivePath) -> Self {
        Self {
            path,
            blocks: Vec::new(),
            current: None,
        }
    }

    /// Finishes any open block and starts a new block of `kind`.
    pub fn start_block(&mut self, kind: BlockKind) {
        self.end_block();
        self.current = Some(ChapterBlockBuilder::new(kind, None, STYLE_NORMAL));
    }

    /// Appends `text` to the current block (starting a paragraph if none is
    /// open), applying the same whitespace collapsing and run merging as XHTML
    /// parsing.
    pub fn push_text(&mut self, text: &str, weight: FontWeight, style: FontStyle) {
        let style_node = style_node_for(weight, style);
        let block = self
            .current
            .get_or_insert_with(|| ChapterBlockBuilder::new(BlockKind::Paragraph, None, STYLE_NORMAL));

        block.push_text(text, None, style_node);
    }

    /// Inserts an explicit line break into the current block, if one is open.
    pub fn push_break(&mut self) {
        if let Some(block) = &mut self.current {
            block.push_break();
        }
    }

    /// Finishes the current block, dropping it if it contains no inlines.
    pub fn end_block(&mut self) {
        if let Some(block) = self.current.take().and_then(ChapterBlockBuilder::finish) {
            self.blocks.push(block);
        }
    }

    /// Completes the chapter and returns it paired with its resolved styles.
    pub fn finish(mut self) -> (Chapter, ChapterStyles) {
        self.end_block();

        let styles = ChapterStyles::from_styles(vec![
            ComputedStyle::with_font(FontWeight::Normal, FontStyle::Normal),
            ComputedStyle::with_font(FontWeight::Bold, FontStyle::Normal),
            ComputedStyle::with_font(FontWeight::Normal, FontStyle::Italic),
            ComputedStyle::with_font(FontWeight::Bold, FontStyle::Italic),
        ]);

        (
            Chapter {
                path: self.path,
                blocks: self.blocks,
            },
            styles,
        )
    }
}

struct ChapterBlockBuilder {
    kind: BlockKind,
    element_depth: Option<usize>,
    style_node: StyleNodeId,
    inlines: Vec<Inline>,
    pending_space: Option<(Option<LinkTarget>, StyleNodeId)>,
}

impl ChapterBlockBuilder {
    fn new(kind: BlockKind, element_depth: Option<usize>, style_node: StyleNodeId) -> Self {
        Self {
            kind,
            element_depth,
            style_node,
            inlines: Vec::new(),
            pending_space: None,
        }
    }

    fn push_anchor(&mut self, anchor: String) {
        if anchor.is_empty() {
            return;
        }

        if self.inlines.iter().any(|inline| {
            matches!(
                inline,
                Inline::Anchor(
                    existing,
                ) if existing == &anchor
            )
        }) {
            return;
        }

        self.inlines.push(Inline::Anchor(anchor));
    }

    fn push_break(&mut self) {
        self.pending_space = None;
        self.inlines.push(Inline::Break);
    }

    fn push_image(&mut self, image: ChapterImage) {
        if let Some((space_link, space_style_node)) = self.pending_space.take()
            && self.can_precede_collapsed_space()
        {
            self.append_text(" ", space_link, space_style_node);
        }

        self.inlines.push(Inline::Image(image));
    }

    fn push_text(&mut self, text: &str, link: Option<LinkTarget>, style_node: StyleNodeId) {
        let mut output = String::new();

        for character in text.chars() {
            // U+FEFF is a byte order mark / zero width no-break space with no visible form
            if character == '\u{FEFF}' {
                continue;
            }

            // only XML whitespace collapses. Other spaces, in particular U+00A0 and
            // U+202F, are kept as text so layout can treat them as non-breaking.
            if is_collapsible_whitespace(character) {
                // flush text before recording the collapsed space.
                // this is important for: Chapter <em>One</em>
                // without flushing "Chapter" here, the block still appears empty when
                // the trailing whitespace is encountered and the separating space is lost.
                if !output.is_empty() {
                    self.append_text(&output, link.clone(), style_node);

                    output.clear();
                }

                // only preserve a collapsed space after actual text.
                // in particular, whitespace immediately following <br> must not produce
                // a leading space on the next line.
                if self.can_precede_collapsed_space() && self.pending_space.is_none() {
                    self.pending_space = Some((link.clone(), style_node));
                }

                continue;
            }

            // emit a pending collapsed space before the next non-whitespace character.
            // the pending space retains the style/link state from where the whitespace
            // occurred, rather than taking the state of the text that follows it.
            // this makes:
            //   See <a>website</a>
            // normalize to:
            //   "See "   unlinked
            //   "website" linked
            if output.is_empty()
                && let Some((space_link, space_style_node)) = self.pending_space.take()
                && self.can_precede_collapsed_space()
                && !suppresses_preceding_space(character)
            {
                self.append_text(" ", space_link, space_style_node);
            }

            output.push(character);
        }

        if !output.is_empty() {
            self.append_text(&output, link, style_node);
        }
    }

    fn append_text(&mut self, text: &str, link: Option<LinkTarget>, style_node: StyleNodeId) {
        if text.is_empty() {
            return;
        }

        if let Some(Inline::Text(last)) = self.inlines.last_mut()
            && last.link == link
            && last.style_node == style_node
        {
            last.text.push_str(text);
            return;
        }

        self.inlines.push(Inline::Text(TextRun {
            text: String::from(text),
            style_node,
            link,
        }));
    }

    fn can_precede_collapsed_space(&self) -> bool {
        for inline in self.inlines.iter().rev() {
            match inline {
                Inline::Text(text) => return !text.text.is_empty(),
                Inline::Image(_) => return true,
                Inline::Break => return false,
                Inline::Anchor(_) => {}
            }
        }

        false
    }

    fn finish(self) -> Option<ChapterBlock> {
        if self.inlines.is_empty() {
            return None;
        }

        Some(ChapterBlock {
            kind: self.kind,
            style_node: self.style_node,
            inlines: self.inlines,
        })
    }
}

pub(crate) fn is_collapsible_whitespace(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\r')
}

fn suppresses_preceding_space(character: char) -> bool {
    matches!(
        character,
        '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '%'
    )
}
