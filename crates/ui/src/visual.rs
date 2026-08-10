use heapless::Vec;

use crate::{FrameArena, NodeId, Point, Rect, px};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipRegion {
    Unbounded,
    Rect(Rect),
    Empty,
}

impl ClipRegion {
    pub(crate) fn intersect(self, rect: Rect) -> Self {
        if rect.width().0 <= 0 || rect.height().0 <= 0 {
            return Self::Empty;
        }

        match self {
            Self::Unbounded => Self::Rect(rect),
            Self::Rect(existing) => match existing.intersection(rect) {
                Some(intersection) => Self::Rect(intersection),
                None => Self::Empty,
            },
            Self::Empty => Self::Empty,
        }
    }

    pub(crate) fn contains(self, point: Point) -> bool {
        match self {
            Self::Unbounded => true,
            Self::Rect(rect) => rect.contains(point),
            Self::Empty => false,
        }
    }

    pub(crate) fn painter_clip(self) -> Option<Rect> {
        match self {
            Self::Unbounded => None,
            Self::Rect(rect) => Some(rect),
            Self::Empty => None,
        }
    }

    pub(crate) const fn is_empty(self) -> bool {
        matches!(self, Self::Empty)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisualContext {
    translation: Point,
    clip: ClipRegion,
}

impl VisualContext {
    pub(crate) const ROOT: Self = Self {
        translation: Point::ZERO,
        clip: ClipRegion::Unbounded,
    };

    pub(crate) fn translate_rect(self, rect: Rect) -> Rect {
        rect.translated(self.translation)
    }

    pub(crate) fn translated_by_scroll(self, scroll: Point) -> Self {
        Self {
            translation: Point::new(
                px(self.translation.x.0.saturating_sub(scroll.x.0)),
                px(self.translation.y.0.saturating_sub(scroll.y.0)),
            ),
            clip: self.clip,
        }
    }

    pub(crate) fn with_clip(self, clip: ClipRegion) -> Self {
        Self {
            translation: self.translation,
            clip,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct VisualNode {
    node: NodeId,
    bounds: Rect,
    clip: ClipRegion,
}

impl VisualNode {
    pub(crate) const fn node(self) -> NodeId {
        self.node
    }

    pub(crate) const fn bounds(self) -> Rect {
        self.bounds
    }

    pub(crate) fn clip(self) -> Option<Rect> {
        self.clip.painter_clip()
    }

    pub(crate) const fn is_visible(self) -> bool {
        !self.clip.is_empty()
    }

    pub(crate) fn contains(self, point: Point) -> bool {
        self.bounds.contains(point) && self.clip.contains(point)
    }
}

pub(crate) struct VisualTraversal<'a, const NODES: usize, const TEXT_BYTES: usize> {
    frame: &'a FrameArena<NODES, TEXT_BYTES>,
    current: Option<(NodeId, VisualContext)>,
    ancestors: Vec<(NodeId, VisualContext), NODES>,
}

impl<'a, const NODES: usize, const TEXT_BYTES: usize> VisualTraversal<'a, NODES, TEXT_BYTES> {
    pub(crate) fn new(frame: &'a FrameArena<NODES, TEXT_BYTES>, root: NodeId) -> Self {
        Self {
            frame,
            current: Some((root, VisualContext::ROOT)),
            ancestors: Vec::new(),
        }
    }

    fn advance(&mut self, node: NodeId, context: VisualContext) {
        if let Some(child) = self.frame.node(node).first_child {
            self.ancestors
                .push((node, context))
                .expect("visual traversal depth exceeds frame capacity");
            let child_context = self.frame.child_visual_context(node, context);
            self.current = Some((child, child_context));
            return;
        }

        let mut cursor = node;
        let mut cursor_context = context;

        loop {
            if let Some(sibling) = self.frame.node(cursor).next_sibling {
                self.current = Some((sibling, cursor_context));
                return;
            }
            let Some((parent, parent_context)) = self.ancestors.pop() else {
                self.current = None;
                return;
            };

            cursor = parent;
            cursor_context = parent_context;
        }
    }
}

impl<const NODES: usize, const TEXT_BYTES: usize> Iterator
    for VisualTraversal<'_, NODES, TEXT_BYTES>
{
    type Item = VisualNode;

    fn next(&mut self) -> Option<Self::Item> {
        let (node, context) = self.current?;
        let bounds = context.translate_rect(self.frame.node(node).layout.bounds);
        self.advance(node, context);

        Some(VisualNode {
            node,
            bounds,
            clip: context.clip,
        })
    }
}

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
    pub(crate) fn visual_nodes(&self, root: NodeId) -> VisualTraversal<'_, NODES, TEXT_BYTES> {
        VisualTraversal::new(self, root)
    }

    pub(crate) fn child_visual_context(
        &self,
        parent: NodeId,
        parent_context: VisualContext,
    ) -> VisualContext {
        let mut clip = parent_context.clip;
        if self.node_clips_children(parent) {
            let parent_clip = self.children_clip_bounds(parent, parent_context);
            clip = clip.intersect(parent_clip);
        }

        let scroll = self.node(parent).interaction.scroll_offset;

        parent_context.translated_by_scroll(scroll).with_clip(clip)
    }

    fn node_clips_children(&self, node: NodeId) -> bool {
        let node = self.node(node);
        let style_clips = node
            .style()
            .map(|style| style.clip_children)
            .unwrap_or(false);

        style_clips || node.interaction.scroll_axes.any()
    }

    fn children_clip_bounds(&self, node: NodeId, context: VisualContext) -> Rect {
        let bounds = context.translate_rect(self.node(node).layout.bounds);
        let border = self
            .node(node)
            .style()
            .map(|style| px(style.border_width.0.max(0)))
            .unwrap_or(px(0));

        bounds.inset(border)
    }

    pub fn visual_bounds(&self, node: NodeId) -> Rect {
        let mut translation = Point::ZERO;
        let mut current = self.node(node).parent;

        while let Some(parent) = current {
            let scroll = self.node(parent).interaction.scroll_offset;
            translation = Point::new(
                px(translation.x.0.saturating_sub(scroll.x.0)),
                px(translation.y.0.saturating_sub(scroll.y.0)),
            );

            current = self.node(parent).parent;
        }

        self.node(node).layout.bounds.translated(translation)
    }
}

#[cfg(test)]
mod tests {
    use core::convert::Infallible;
    use std::vec::Vec;

    use crate::*;

    struct TestTextMeasurer;

    impl TextMeasurer for TestTextMeasurer {
        fn measure(&self, _text: &str, _max_size: Size) -> Size {
            Size::ZERO
        }
    }

    #[test]
    fn visual_traversal_accumulates_nested_scroll_offsets() {
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(100))
                    .child(div().w(px(80)).h(px(80)).child(div().w(px(40)).h(px(40)))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &TestTextMeasurer);

        let outer = frame.node(root).first_child.unwrap();

        frame.node_mut(root).interaction.scroll_offset = Point::new(px(0), px(10));
        frame.node_mut(outer).interaction.scroll_offset = Point::new(px(0), px(7));

        let mut traversal = frame.visual_nodes(root);
        let root_visual = traversal.next().unwrap();
        let outer_visual = traversal.next().unwrap();
        let inner_visual = traversal.next().unwrap();

        assert_eq!(root_visual.bounds().y(), px(0));
        assert_eq!(outer_visual.bounds().y(), px(-10));
        assert_eq!(inner_visual.bounds().y(), px(-17));
    }

    #[test]
    fn visual_traversal_intersects_nested_clips() {
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(100))
                    .border(px(2))
                    .overflow_hidden()
                    .child(
                        div()
                            .w(px(60))
                            .h(px(60))
                            .border(px(3))
                            .overflow_hidden()
                            .child(div().w(px(100)).h(px(100))),
                    ),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &TestTextMeasurer);

        let mut traversal = frame.visual_nodes(root);
        let root_visual = traversal.next().unwrap();
        let child_visual = traversal.next().unwrap();
        let grandchild_visual = traversal.next().unwrap();

        assert_eq!(root_visual.clip(), None);
        assert_eq!(
            child_visual.clip(),
            Some(crate::Rect::new(
                Point::new(px(2), px(2),),
                Size::new(px(96), px(96),),
            ))
        );
        assert_eq!(
            grandchild_visual.clip(),
            Some(crate::Rect::new(
                Point::new(px(5), px(5),),
                Size::new(px(54), px(54),),
            ))
        );
    }

    #[test]
    fn scroll_translation_does_not_move_scroll_viewport_clip() {
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(40))
                    .border(px(2))
                    .overflow_hidden()
                    .child(div().w(px(100)).h(px(100))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(40)), &TestTextMeasurer);
        frame.node_mut(root).interaction.scroll_offset = Point::new(px(0), px(20));

        let mut traversal = frame.visual_nodes(root);
        let root_visual = traversal.next().unwrap();
        let child_visual = traversal.next().unwrap();

        assert_eq!(root_visual.bounds().y(), px(0));
        assert_eq!(child_visual.bounds().y(), px(-18));
        assert_eq!(
            child_visual.clip(),
            Some(crate::Rect::new(
                Point::new(px(2), px(2),),
                Size::new(px(96), px(36),),
            ))
        );
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Command {
        Box {
            bounds: Rect,
            paint: BoxPaint,
            clip: Option<Rect>,
        },
        Text {
            origin: Point,
            length: usize,
            clip: Option<Rect>,
        },
    }

    #[derive(Default)]
    struct RecordingPainter {
        commands: Vec<Command>,
    }

    impl TextMeasurer for RecordingPainter {
        fn measure(&self, text: &str, max_size: Size) -> Size {
            let width = (text.chars().count() as i32)
                .saturating_mul(6)
                .min(max_size.width.0.max(0));
            let height = if text.is_empty() {
                0
            } else {
                10.min(max_size.height.0.max(0))
            };

            Size::new(px(width), px(height))
        }
    }

    impl Painter for RecordingPainter {
        type Error = Infallible;

        fn draw_box(
            &mut self,
            bounds: Rect,
            paint: BoxPaint,
            clip: Option<Rect>,
        ) -> Result<(), Self::Error> {
            self.commands.push(Command::Box {
                bounds,
                paint,
                clip,
            });
            Ok(())
        }

        fn draw_text(
            &mut self,
            text: &str,
            origin: Point,
            clip: Option<Rect>,
        ) -> Result<(), Self::Error> {
            self.commands.push(Command::Text {
                origin,
                length: text.len(),
                clip,
            });
            Ok(())
        }
    }

    #[test]
    fn painting_uses_carried_scroll_and_clip_context() {
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(50))
                    .h(px(30))
                    .border(px(1))
                    .overflow_hidden()
                    .child(div().w(px(50)).h(px(60)).bg(Color::RED)),
            )
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(root, Size::new(px(50), px(30)), &painter);
        frame.node_mut(root).interaction.scroll_offset = Point::new(px(0), px(10));
        frame.paint(root, &mut painter).unwrap();

        assert_eq!(
            painter.commands[1],
            Command::Box {
                bounds: Rect::new(Point::new(px(1), px(-9)), Size::new(px(50), px(60))),
                paint: BoxPaint {
                    background: Some(Color::RED),
                    border: None,
                    radius: px(0),
                },
                clip: Some(Rect::new(
                    Point::new(px(1), px(1),),
                    Size::new(px(48), px(28),),
                )),
            }
        );
    }
}
