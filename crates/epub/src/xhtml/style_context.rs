use alloc::{string::String, vec::Vec};

use crate::ArchivePath;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StyleNodeId(usize);

impl StyleNodeId {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleNode {
    parent: Option<StyleNodeId>,
    element: String,
    id: Option<String>,
    classes: Vec<String>,
    inline_style: Option<String>,
}

impl StyleNode {
    pub(crate) fn new(
        parent: Option<StyleNodeId>,
        element: String,
        id: Option<String>,
        classes: Vec<String>,
        inline_style: Option<String>,
    ) -> Self {
        Self {
            parent,
            element,
            id,
            classes,
            inline_style,
        }
    }

    pub const fn parent(&self) -> Option<StyleNodeId> {
        self.parent
    }

    pub fn element(&self) -> &str {
        &self.element
    }

    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn classes(&self) -> &[String] {
        &self.classes
    }

    pub fn has_class(&self, class: &str) -> bool {
        self.classes.iter().any(|candidate| candidate == class)
    }

    pub fn inline_style(&self) -> Option<&str> {
        self.inline_style.as_deref()
    }
}

/// Element tree and stylesheet links captured during XHTML parsing and consumed
/// when resolving [`crate::ChapterStyles`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleContext {
    nodes: Vec<StyleNode>,
    stylesheets: Vec<StylesheetSource>,
}

impl StyleContext {
    pub(crate) fn new(nodes: Vec<StyleNode>, stylesheets: Vec<StylesheetSource>) -> Self {
        Self { nodes, stylesheets }
    }

    pub fn style_nodes(&self) -> &[StyleNode] {
        &self.nodes
    }

    pub fn style_node(&self, id: StyleNodeId) -> Option<&StyleNode> {
        self.nodes.get(id.index())
    }

    pub fn stylesheets(&self) -> &[StylesheetSource] {
        &self.stylesheets
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StylesheetSource {
    External(ArchivePath),
    Embedded(String),
}

impl StylesheetSource {
    pub fn external_path(&self) -> Option<&ArchivePath> {
        match self {
            Self::External(path) => Some(path),
            Self::Embedded(_) => None,
        }
    }

    pub fn embedded_css(&self) -> Option<&str> {
        match self {
            Self::Embedded(css) => Some(css),
            Self::External(_) => None,
        }
    }
}
