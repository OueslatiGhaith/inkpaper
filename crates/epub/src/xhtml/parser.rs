use alloc::{string::String, vec::Vec};

use xmlparser::{ElementEnd, Token, Tokenizer};

use crate::{ArchivePath, XhtmlError, xml::decode_xml_value};

use super::{BlockKind, Chapter, ChapterBlock, ChapterBlockBuilder, InlineStyle, LinkTarget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ElementKind {
    Body,
    Paragraph,
    Heading(u8),
    ListItem,
    Strong,
    Emphasis,
    Anchor,
    Break,
    BlockBoundary,
    Ignored,
    Other,
}

impl ElementKind {
    fn from_name(name: &str) -> Self {
        match name {
            "body" => Self::Body,

            "p" => Self::Paragraph,

            "h1" => Self::Heading(1),
            "h2" => Self::Heading(2),
            "h3" => Self::Heading(3),
            "h4" => Self::Heading(4),
            "h5" => Self::Heading(5),
            "h6" => Self::Heading(6),

            "li" => Self::ListItem,

            "strong" | "b" => Self::Strong,

            "em" | "i" => Self::Emphasis,

            "a" => Self::Anchor,

            "br" => Self::Break,

            "div" | "section" | "article" | "aside" | "main" | "header" | "footer" | "nav"
            | "blockquote" | "ul" | "ol" | "dl" | "dt" | "dd" | "pre" | "table" | "thead"
            | "tbody" | "tfoot" | "tr" | "td" | "th" => Self::BlockBoundary,

            "script" | "style" | "svg" | "math" => Self::Ignored,

            _ => Self::Other,
        }
    }

    const fn block_kind(self) -> Option<BlockKind> {
        match self {
            Self::Paragraph => Some(BlockKind::Paragraph),
            Self::Heading(level) => Some(BlockKind::Heading(level)),
            Self::ListItem => Some(BlockKind::ListItem),
            _ => None,
        }
    }
}

struct PendingElement {
    kind: ElementKind,
    id: Option<String>,
    name: Option<String>,
    href: Option<String>,
}

impl PendingElement {
    fn new(kind: ElementKind) -> Self {
        Self {
            kind,
            id: None,
            name: None,
            href: None,
        }
    }
}

struct ActiveLink {
    depth: usize,
    target: LinkTarget,
}

struct XhtmlParser {
    path: ArchivePath,

    depth: usize,

    saw_body: bool,
    body_depth: Option<usize>,
    ignored_depth: Option<usize>,

    pending: Option<PendingElement>,

    current: Option<ChapterBlockBuilder>,

    blocks: Vec<ChapterBlock>,

    pending_anchors: Vec<String>,

    bold_depth: usize,
    italic_depth: usize,

    active_link: Option<ActiveLink>,
}

impl XhtmlParser {
    fn new(path: ArchivePath) -> Self {
        Self {
            path,
            depth: 0,
            saw_body: false,
            body_depth: None,
            ignored_depth: None,
            pending: None,
            current: None,
            blocks: Vec::new(),
            pending_anchors: Vec::new(),
            bold_depth: 0,
            italic_depth: 0,
            active_link: None,
        }
    }

    fn element_start(&mut self, name: &str) {
        self.depth = self.depth.saturating_add(1);
        self.pending = Some(PendingElement::new(ElementKind::from_name(name)));
    }

    fn attribute(&mut self, name: &str, value: &str) {
        let Some(element) = &mut self.pending else {
            return;
        };

        match name {
            "id" => element.id = Some(decode_xml_value(value)),
            "name" if element.kind == ElementKind::Anchor => {
                element.name = Some(decode_xml_value(value));
            }
            "href" if element.kind == ElementKind::Anchor => {
                element.href = Some(decode_xml_value(value));
            }
            _ => {}
        }
    }

    fn element_boundary(&mut self, empty: bool) -> Result<(), XhtmlError> {
        let Some(element) = self.pending.take() else {
            return Ok(());
        };

        if element.kind == ElementKind::Body {
            self.saw_body = true;
            if !empty {
                self.body_depth = Some(self.depth);
            }

            return Ok(());
        }

        if self.body_depth.is_none() {
            return Ok(());
        }

        if self.ignored_depth.is_some() {
            return Ok(());
        }

        if element.kind == ElementKind::Ignored {
            if !empty {
                self.ignored_depth = Some(self.depth);
            }

            return Ok(());
        }

        if let Some(kind) = element.kind.block_kind() {
            self.finish_current();

            self.current = Some(ChapterBlockBuilder::new(
                kind,
                if empty { None } else { Some(self.depth) },
            ));

            self.flush_pending_anchors();

            if let Some(id) = element.id {
                self.push_anchor(id);
            }

            if empty {
                self.finish_current();
            }

            return Ok(());
        }

        match element.kind {
            ElementKind::Strong => {
                if let Some(id) = element.id {
                    self.push_anchor(id);
                }
                if !empty {
                    self.bold_depth = self.bold_depth.saturating_add(1);
                }
            }
            ElementKind::Emphasis => {
                if let Some(id) = element.id {
                    self.push_anchor(id);
                }
                if !empty {
                    self.italic_depth = self.italic_depth.saturating_add(1);
                }
            }
            ElementKind::Anchor => {
                if let Some(id) = element.id {
                    self.push_anchor(id);
                }
                if let Some(name) = element.name
                    && !name.is_empty()
                {
                    self.push_anchor(name);
                }
                if !empty && let Some(href) = element.href {
                    let target =
                        LinkTarget::resolve(&self.path, &href).map_err(XhtmlError::Path)?;

                    self.active_link = Some(ActiveLink {
                        depth: self.depth,
                        target,
                    });
                }
            }
            ElementKind::Break => {
                if let Some(id) = element.id {
                    self.push_anchor(id);
                }

                self.ensure_implicit_block();
                self.current
                    .as_mut()
                    .expect("implicit block exists")
                    .push_break();
            }
            ElementKind::BlockBoundary => {
                self.finish_implicit_block();
                if let Some(id) = element.id {
                    self.queue_anchor(id);
                }
            }
            ElementKind::Other => {
                if let Some(id) = element.id {
                    self.push_anchor(id);
                }
            }
            ElementKind::Body
            | ElementKind::Paragraph
            | ElementKind::Heading(_)
            | ElementKind::ListItem
            | ElementKind::Ignored => {}
        }

        Ok(())
    }

    fn close_element(&mut self, name: &str) {
        let kind = ElementKind::from_name(name);

        if let Some(ignored_depth) = self.ignored_depth {
            if ignored_depth == self.depth {
                self.ignored_depth = None;
            }

            return;
        }

        if self.body_depth.is_none() {
            return;
        }

        if kind == ElementKind::Body && self.body_depth == Some(self.depth) {
            self.finish_current();

            self.body_depth = None;

            return;
        }

        match kind {
            ElementKind::Strong => {
                self.bold_depth = self.bold_depth.saturating_sub(1);
            }
            ElementKind::Emphasis => {
                self.italic_depth = self.italic_depth.saturating_sub(1);
            }
            ElementKind::Anchor => {
                if self
                    .active_link
                    .as_ref()
                    .is_some_and(|link| link.depth == self.depth)
                {
                    self.active_link = None;
                }
            }
            ElementKind::Paragraph | ElementKind::Heading(_) | ElementKind::ListItem => {
                let should_finish = self
                    .current
                    .as_ref()
                    .is_some_and(|block| block.element_depth == Some(self.depth));

                if should_finish {
                    self.finish_current();
                }
            }
            ElementKind::BlockBoundary => {
                self.finish_implicit_block();
            }

            _ => {}
        }
    }

    fn text(&mut self, text: &str, cdata: bool) {
        if self.body_depth.is_none() || self.ignored_depth.is_some() {
            return;
        }

        let text = if cdata {
            String::from(text)
        } else {
            decode_xml_value(text)
        };

        if text.chars().all(char::is_whitespace) && self.current.is_none() {
            return;
        }

        self.ensure_implicit_block();

        let style = InlineStyle {
            bold: self.bold_depth > 0,

            italic: self.italic_depth > 0,
        };

        let link = self.active_link.as_ref().map(|link| link.target.clone());

        self.current
            .as_mut()
            .expect("chapter block exists")
            .push_text(&text, style, link);
    }

    fn push_anchor(&mut self, anchor: String) {
        if anchor.is_empty() {
            return;
        }

        if self.current.is_none() {
            self.queue_anchor(anchor);

            return;
        }

        self.current
            .as_mut()
            .expect("chapter block exists")
            .push_anchor(anchor);
    }

    fn queue_anchor(&mut self, anchor: String) {
        if anchor.is_empty()
            || self
                .pending_anchors
                .iter()
                .any(|existing| existing == &anchor)
        {
            return;
        }

        self.pending_anchors.push(anchor);
    }

    fn flush_pending_anchors(&mut self) {
        if self.current.is_none() {
            return;
        }

        let anchors = core::mem::take(&mut self.pending_anchors);

        let block = self.current.as_mut().expect("chapter block exists");

        for anchor in anchors {
            block.push_anchor(anchor);
        }
    }

    fn ensure_implicit_block(&mut self) {
        if self.current.is_some() {
            return;
        }

        self.current = Some(ChapterBlockBuilder::new(BlockKind::Paragraph, None));

        self.flush_pending_anchors();
    }

    fn finish_implicit_block(&mut self) {
        let implicit = self
            .current
            .as_ref()
            .is_some_and(|block| block.element_depth.is_none());

        if implicit {
            self.finish_current();
        }
    }

    fn finish_current(&mut self) {
        let Some(block) = self.current.take() else {
            return;
        };

        if let Some(block) = block.finish() {
            self.blocks.push(block);
        }
    }

    fn finish(mut self) -> Result<Chapter, XhtmlError> {
        self.finish_current();

        if !self.saw_body {
            return Err(XhtmlError::MissingBody);
        }

        Ok(Chapter {
            path: self.path,
            blocks: self.blocks,
        })
    }
}

pub(crate) fn parse_xhtml(xml: &str, path: ArchivePath) -> Result<Chapter, XhtmlError> {
    let mut parser = XhtmlParser::new(path);

    for token in Tokenizer::from(xml) {
        match token.map_err(XhtmlError::Xml)? {
            Token::ElementStart { local, .. } => {
                parser.element_start(local.as_str());
            }
            Token::Attribute { local, value, .. } => {
                parser.attribute(local.as_str(), value.as_str());
            }
            Token::Text { text } => {
                parser.text(text.as_str(), false);
            }
            Token::Cdata { text, .. } => {
                parser.text(text.as_str(), true);
            }
            Token::ElementEnd {
                end: ElementEnd::Open,
                ..
            } => {
                parser.element_boundary(false)?;
            }
            Token::ElementEnd {
                end: ElementEnd::Empty,
                ..
            } => {
                parser.element_boundary(true)?;

                parser.depth = parser.depth.saturating_sub(1);
            }
            Token::ElementEnd {
                end: ElementEnd::Close(_, local),
                ..
            } => {
                parser.close_element(local.as_str());

                parser.depth = parser.depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    parser.finish()
}
