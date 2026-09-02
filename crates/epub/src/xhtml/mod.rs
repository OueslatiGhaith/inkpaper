use alloc::{string::String, vec::Vec};

use crate::{ArchivePath, PathError};

mod parser;
mod style_context;

#[cfg(test)]
mod tests;

pub(crate) use parser::parse_xhtml;
pub use style_context::{StyleNode, StyleNodeId, StylesheetSource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
    path: ArchivePath,
    blocks: Vec<ChapterBlock>,
    style_nodes: Vec<StyleNode>,
    stylesheets: Vec<StylesheetSource>,
}

impl Chapter {
    pub fn path(&self) -> &ArchivePath {
        &self.path
    }

    pub fn blocks(&self) -> &[ChapterBlock] {
        &self.blocks
    }

    pub fn style_nodes(&self) -> &[StyleNode] {
        &self.style_nodes
    }

    pub fn style_node(&self, id: StyleNodeId) -> Option<&StyleNode> {
        self.style_nodes.get(id.index())
    }

    pub fn stylesheets(&self) -> &[StylesheetSource] {
        &self.stylesheets
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
    style: InlineStyle,
    style_node: StyleNodeId,
    link: Option<LinkTarget>,
}

impl TextRun {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub const fn style(&self) -> InlineStyle {
        self.style
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InlineStyle {
    bold: bool,
    italic: bool,
}

impl InlineStyle {
    pub const fn bold(self) -> bool {
        self.bold
    }

    pub const fn italic(self) -> bool {
        self.italic
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
                    Some(String::from(fragment))
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

struct ChapterBlockBuilder {
    kind: BlockKind,
    element_depth: Option<usize>,
    style_node: StyleNodeId,
    inlines: Vec<Inline>,
    pending_space: Option<(InlineStyle, Option<LinkTarget>, StyleNodeId)>,
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
        if let Some((space_style, space_link, space_style_node)) = self.pending_space.take()
            && self.can_precede_collapsed_space()
        {
            self.append_text(" ", space_style, space_link, space_style_node);
        }

        self.inlines.push(Inline::Image(image));
    }

    fn push_text(
        &mut self,
        text: &str,
        style: InlineStyle,
        link: Option<LinkTarget>,
        style_node: StyleNodeId,
    ) {
        let mut output = String::new();

        for character in text.chars() {
            if character.is_whitespace() {
                // flush text before recording the collapsed space.
                // this is important for: Chapter <em>One</em>
                // without flushing "Chapter" here, the block still appears empty when
                // the trailing whitespace is encountered and the separating space is lost.
                if !output.is_empty() {
                    self.append_text(&output, style, link.clone(), style_node);

                    output.clear();
                }

                // only preserve a collapsed space after actual text.
                // in particular, whitespace immediately following <br> must not produce
                // a leading space on the next line.
                if self.can_precede_collapsed_space() && self.pending_space.is_none() {
                    self.pending_space = Some((style, link.clone(), style_node));
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
                && let Some((space_style, space_link, space_style_node)) = self.pending_space.take()
                && self.can_precede_collapsed_space()
                && !suppresses_preceding_space(character)
            {
                self.append_text(" ", space_style, space_link, space_style_node);
            }

            output.push(character);
        }

        if !output.is_empty() {
            self.append_text(&output, style, link, style_node);
        }
    }

    fn append_text(
        &mut self,
        text: &str,
        style: InlineStyle,
        link: Option<LinkTarget>,
        style_node: StyleNodeId,
    ) {
        if text.is_empty() {
            return;
        }

        if let Some(Inline::Text(last)) = self.inlines.last_mut()
            && last.style == style
            && last.link == link
            && last.style_node == style_node
        {
            last.text.push_str(text);
            return;
        }

        self.inlines.push(Inline::Text(TextRun {
            text: String::from(text),
            style,
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

fn suppresses_preceding_space(character: char) -> bool {
    matches!(
        character,
        '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '%'
    )
}
