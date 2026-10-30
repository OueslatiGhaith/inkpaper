//! the boxes nested in a node's layout bounds. Each is its own type, so code that
//! measures against one of them has to say which

use super::{border_width, content_size};
use crate::{Offset, Pixels, Point, Rect, Style};

/// a node's layout bounds, border included
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BorderBox(Rect);

/// the area inside a node's border. Its children clip to it and scroll within it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PaddingBox(Rect);

/// the area inside a node's padding, where its children are placed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContentBox(Rect);

impl BorderBox {
    pub(crate) const fn new(bounds: Rect) -> Self {
        Self(bounds)
    }

    /// `style` is `None` for nodes that have no border, such as text
    pub(crate) fn padding_box(self, style: Option<Style>) -> PaddingBox {
        PaddingBox(
            self.0
                .inset(style.map(border_width).unwrap_or(Pixels::ZERO)),
        )
    }

    pub(crate) fn content_box(self, style: Style) -> ContentBox {
        let border = border_width(style);

        ContentBox(Rect::new(
            Point::new(
                self.0.x() + border + style.padding.left.non_negative(),
                self.0.y() + border + style.padding.top.non_negative(),
            ),
            content_size(style, self.0.size),
        ))
    }
}

impl PaddingBox {
    pub(crate) const fn rect(self) -> Rect {
        self.0
    }
}

impl ContentBox {
    /// the root lays out in the viewport as if it were a parent's content box
    pub(crate) const fn viewport(viewport: Rect) -> Self {
        Self(viewport)
    }

    pub(crate) const fn rect(self) -> Rect {
        self.0
    }
}

/// the far edges of what a scroll container's children cover
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChildrenExtent {
    right: Pixels,
    bottom: Pixels,
}

impl ChildrenExtent {
    pub(crate) fn new(child: Rect) -> Self {
        Self {
            right: child.right(),
            bottom: child.bottom(),
        }
    }

    pub(crate) fn include(self, child: Rect) -> Self {
        Self {
            right: self.right.max(child.right()),
            bottom: self.bottom.max(child.bottom()),
        }
    }

    /// how far `viewport` scrolls to show the children and the padding after them.
    /// Children are placed past the leading padding, so only the trailing padding
    /// adds to their extent
    pub(crate) fn scroll_range(self, viewport: PaddingBox, style: Option<Style>) -> Offset {
        let (right_padding, bottom_padding) = style.map_or((Pixels::ZERO, Pixels::ZERO), |style| {
            (
                style.padding.right.non_negative(),
                style.padding.bottom.non_negative(),
            )
        });

        Offset::new(
            (self.right + right_padding - viewport.0.right()).non_negative(),
            (self.bottom + bottom_padding - viewport.0.bottom()).non_negative(),
        )
    }
}
