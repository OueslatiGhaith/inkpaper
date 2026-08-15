use heapless::Vec;

use crate::{DamageRegion, FrameArena, NodeId, Offset, Point, Rect, count_metric, px};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipRegion {
    Unbounded,
    Rect(Rect),
    Empty,
}

impl ClipRegion {
    pub(crate) fn intersect(self, rect: Rect) -> Self {
        if rect.width().is_non_positive() || rect.height().is_non_positive() {
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

    pub(crate) fn intersects_damage(self, damage: DamageRegion) -> bool {
        match self {
            ClipRegion::Unbounded => true,
            ClipRegion::Rect(rect) => damage.intersects_rect(rect),
            ClipRegion::Empty => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisualContext {
    translation: Offset,
    clip: ClipRegion,
}

impl VisualContext {
    pub(crate) const ROOT: Self = Self {
        translation: Offset::ZERO,
        clip: ClipRegion::Unbounded,
    };

    pub(crate) fn translate_rect(self, rect: Rect) -> Rect {
        rect.translated(self.translation)
    }

    pub(crate) fn translated_by_scroll(self, scroll: Offset) -> Self {
        Self {
            translation: self.translation - scroll,
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

    pub(crate) fn clip_region(self) -> ClipRegion {
        self.clip
    }
}

const VISUAL_CONTEXT_STACK_CAPACITY: usize = 16;

#[derive(Debug, Clone, Copy)]
struct VisualContinuation {
    sibling: NodeId,
    context: VisualContext,
    #[cfg(feature = "metrics")]
    depth: u16,
}

pub(crate) struct VisualTraversal<'a, const NODES: usize, const TEXT_BYTES: usize> {
    frame: &'a FrameArena<NODES, TEXT_BYTES>,
    root: NodeId,
    current: Option<(NodeId, VisualContext)>,
    pending_advance: Option<(NodeId, VisualContext)>,
    branch_continuations: Vec<VisualContinuation, VISUAL_CONTEXT_STACK_CAPACITY>,
    overflowed_branch_continuations: u16,
    #[cfg(feature = "metrics")]
    depth: u16,
}

impl<'a, const NODES: usize, const TEXT_BYTES: usize> VisualTraversal<'a, NODES, TEXT_BYTES> {
    pub(crate) fn new(frame: &'a FrameArena<NODES, TEXT_BYTES>, root: NodeId) -> Self {
        count_metric!(frame, visual_traversal_passes);

        Self {
            frame,
            root,
            current: Some((root, VisualContext::ROOT)),
            pending_advance: None,
            branch_continuations: Vec::new(),
            overflowed_branch_continuations: 0,
            #[cfg(feature = "metrics")]
            depth: 0,
        }
    }

    fn record_descent(&mut self) {
        count_metric!(self.frame, visual_ancestor_pushes);

        #[cfg(feature = "metrics")]
        {
            self.depth = self.depth.saturating_add(1);
            self.frame.metrics.increment(|metrics| {
                metrics.visual_ancestor_depth_peak =
                    metrics.visual_ancestor_depth_peak.max(self.depth as u64);
            });
        }
    }

    #[cfg(feature = "metrics")]
    fn record_ascents_to(&mut self, depth: u16) {
        let ascents = self.depth.saturating_sub(depth);
        self.frame.metrics.increment(|metrics| {
            metrics.visual_ancestor_pops =
                metrics.visual_ancestor_pops.saturating_add(ascents as u64);
        });

        self.depth = depth;
    }

    fn save_branch_continuation(&mut self, sibling: NodeId, context: VisualContext) {
        let continuation = VisualContinuation {
            sibling,
            context,
            #[cfg(feature = "metrics")]
            depth: self.depth,
        };

        match self.branch_continuations.push(continuation) {
            Ok(()) => {
                count_metric!(self.frame, visual_context_stack_pushes);
                #[cfg(feature = "metrics")]
                self.frame.metrics.increment(|metrics| {
                    let depth = self.branch_continuations.len() as u64;
                    metrics.visual_context_stack_depth_peak =
                        metrics.visual_context_stack_depth_peak.max(depth);
                });
            }

            Err(_) => {
                self.overflowed_branch_continuations =
                    self.overflowed_branch_continuations.saturating_add(1);

                count_metric!(self.frame, visual_context_stack_overflows);
            }
        }
    }

    fn restore_saved_continuation(&mut self) -> Option<(NodeId, VisualContext)> {
        let continuation = self.branch_continuations.pop()?;
        count_metric!(self.frame, visual_context_stack_pops);

        #[cfg(feature = "metrics")]
        self.record_ascents_to(continuation.depth);

        Some((continuation.sibling, continuation.context))
    }

    fn restore_overflowed_continuation(&mut self, node: NodeId) -> (NodeId, VisualContext) {
        debug_assert!(self.overflowed_branch_continuations > 0);
        let mut cursor = node;

        #[cfg(feature = "metrics")]
        let mut ascents = 0u16;

        loop {
            let parent = self
                .frame
                .node(cursor)
                .parent
                .expect("overflowed visual continuation must have an ancestor");

            #[cfg(feature = "metrics")]
            {
                ascents = ascents.saturating_add(1);
            }

            if let Some(sibling) = self.frame.node(parent).next_sibling {
                self.overflowed_branch_continuations -= 1;
                count_metric!(self.frame, visual_context_recomputations);

                #[cfg(feature = "metrics")]
                {
                    let target_depth = self.depth.saturating_sub(ascents);
                    self.record_ascents_to(target_depth);
                }

                let context = self.frame.visual_context_for_node(self.root, parent);

                return (sibling, context);
            }

            cursor = parent;
        }
    }

    fn finish_subtree(&mut self, node: NodeId) {
        if self.overflowed_branch_continuations > 0 {
            self.current = Some(self.restore_overflowed_continuation(node));
            return;
        }

        if let Some(continuation) = self.restore_saved_continuation() {
            self.current = Some(continuation);
            return;
        }

        #[cfg(feature = "metrics")]
        self.record_ascents_to(0);

        self.current = None;
    }

    fn advance(&mut self, node: NodeId, context: VisualContext) {
        let (first_child, next_sibling) = {
            let node = self.frame.node(node);
            (node.first_child, node.next_sibling)
        };

        if let Some(child) = first_child {
            if let Some(sibling) = next_sibling {
                self.save_branch_continuation(sibling, context);
            }
            self.record_descent();

            let child_context = self.frame.child_visual_context(node, context);
            self.current = Some((child, child_context));
            return;
        }

        if let Some(sibling) = next_sibling {
            self.current = Some((sibling, context));
            return;
        }

        self.finish_subtree(node);
    }

    fn advance_without_children(&mut self, node: NodeId, context: VisualContext) {
        let next_sibling = self.frame.node(node).next_sibling;
        if let Some(sibling) = next_sibling {
            self.current = Some((sibling, context));
            return;
        }

        self.finish_subtree(node);
    }

    pub(crate) fn skip_children(&mut self) {
        let Some((node, context)) = self.pending_advance.take() else {
            return;
        };

        self.advance_without_children(node, context);
    }

    fn finish_pending_advance(&mut self) {
        let Some((node, context)) = self.pending_advance.take() else {
            return;
        };

        self.advance(node, context);
    }
}

impl<const NODES: usize, const TEXT_BYTES: usize> Iterator
    for VisualTraversal<'_, NODES, TEXT_BYTES>
{
    type Item = VisualNode;

    fn next(&mut self) -> Option<Self::Item> {
        // if the called did not explicitly prune the previously yielded node,
        // descend normally
        self.finish_pending_advance();

        let (node, context) = self.current.take()?;
        count_metric!(self.frame, visual_traversal_nodes);

        let bounds = context.translate_rect(self.frame.node(node).layout.bounds);

        // advancement is deferred until the caller asks for the next node. This gives
        // it one chance to replace normal descent with `skip_children()`
        self.pending_advance = Some((node, context));

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
        count_metric!(self, visual_context_derivations);
        let mut clip = parent_context.clip;
        if self.node_clips_children(parent) {
            count_metric!(self, visual_clip_intersections);
            let parent_clip = self.children_clip_bounds(parent, parent_context);
            clip = clip.intersect(parent_clip);
        }

        let scroll = self.node(parent).interaction.scroll_offset;

        parent_context.translated_by_scroll(scroll).with_clip(clip)
    }

    pub(crate) fn node_clips_children(&self, node: NodeId) -> bool {
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
            .map(|style| style.border_width.non_negative())
            .unwrap_or(px(0));

        bounds.inset(border)
    }

    pub fn visual_bounds(&self, node: NodeId) -> Rect {
        let mut translation = Offset::ZERO;
        let mut current = self.node(node).parent;

        while let Some(parent) = current {
            count_metric!(self, visual_bounds_ancestor_visits);
            translation -= self.node(parent).interaction.scroll_offset;
            current = self.node(parent).parent;
        }

        self.node(node).layout.bounds.translated(translation)
    }

    fn visual_context_for_node(&self, root: NodeId, node: NodeId) -> VisualContext {
        if node == root {
            return VisualContext::ROOT;
        }

        let mut depth = 0usize;
        let mut cursor = node;

        while cursor != root {
            cursor = self
                .node(cursor)
                .parent
                .expect("visual node must descend from traversal root");

            depth = depth.saturating_add(1);
        }

        let mut context = VisualContext::ROOT;
        let mut level = 0usize;

        while level < depth {
            let steps_up = depth - level;
            let mut ancestor = node;

            for _ in 0..steps_up {
                ancestor = self
                    .node(ancestor)
                    .parent
                    .expect("visual node must descend from traversal root");
            }

            context = self.child_visual_context(ancestor, context);
            level += 1;
        }

        context
    }
}

#[cfg(test)]
mod tests {
    use core::convert::Infallible;
    use std::vec::Vec;

    use crate::{visual::VisualTraversal, *};

    struct TestTextMeasurer;

    impl TextMeasurer for TestTextMeasurer {
        fn measure_text(&self, text: &str, _style: ResolvedTextStyle, max_size: Size) -> Size {
            let width = px(i32::try_from(text.chars().count()).unwrap_or(i32::MAX))
                .saturating_mul(6)
                .min(max_size.width.non_negative());

            let height = if text.is_empty() {
                px(0)
            } else {
                px(0).min(max_size.height.non_negative())
            };

            Size::new(width, height)
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

        frame.node_mut(root).interaction.scroll_offset = Offset::new(px(0), px(10));
        frame.node_mut(outer).interaction.scroll_offset = Offset::new(px(0), px(7));

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
        frame.node_mut(root).interaction.scroll_offset = Offset::new(px(0), px(20));

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
            bounds: Rect,
            length: usize,
            style: ResolvedTextStyle,
            clip: Option<Rect>,
        },
        Image {
            source: ImageSource,
            bounds: Rect,
            clip: Option<Rect>,
        },
    }

    #[derive(Default)]
    struct RecordingPainter {
        commands: Vec<Command>,
    }

    impl TextMeasurer for RecordingPainter {
        fn measure_text(&self, text: &str, _style: ResolvedTextStyle, max_size: Size) -> Size {
            let width = px(i32::try_from(text.chars().count()).unwrap_or(i32::MAX))
                .saturating_mul(6)
                .min(max_size.width.non_negative());

            let height = if text.is_empty() {
                Pixels::ZERO
            } else {
                px(10).min(max_size.height.non_negative())
            };

            Size::new(width, height)
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
            bounds: Rect,
            style: ResolvedTextStyle,
            clip: Option<Rect>,
        ) -> Result<(), Self::Error> {
            self.commands.push(Command::Text {
                bounds,
                length: text.len(),
                style,
                clip,
            });

            Ok(())
        }

        fn draw_image(
            &mut self,
            source: ImageSource,
            bounds: Rect,
            _fit: ImageFit,
            clip: Option<Rect>,
        ) -> Result<(), Self::Error> {
            self.commands.push(Command::Image {
                source,
                bounds,
                clip,
            });

            Ok(())
        }

        fn draw_canvas(
            &mut self,
            _: Rect,
            _: Option<Rect>,
            _: &mut dyn FnMut(Rect, &mut dyn CanvasPainter),
        ) -> Result<(), Self::Error> {
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
        frame.node_mut(root).interaction.scroll_offset = Offset::new(px(0), px(10));
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

    #[test]
    fn explicit_size_can_overflow_parent_constraints() {
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(50))
                    .h(px(30))
                    .border(px(1))
                    .overflow_hidden()
                    .child(div().w(px(50)).h(px(60))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(50), px(30)), &TestTextMeasurer);

        let child = frame.node(root).first_child.unwrap();

        assert_eq!(
            frame.bounds(root),
            Rect::new(Point::new(px(0), px(0),), Size::new(px(50), px(30),),)
        );
        assert_eq!(
            frame.bounds(child),
            Rect::new(Point::new(px(1), px(1),), Size::new(px(50), px(60),),)
        );
    }

    #[test]
    fn flex_shrink_can_reduce_explicitly_oversized_items() {
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .w(px(100))
                    .h(px(40))
                    .child(div().w(px(80)).h(px(20)).flex_shrink(1))
                    .child(div().w(px(80)).h(px(20)).flex_shrink(1)),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(40)), &TestTextMeasurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();

        assert_eq!(frame.bounds(first).width(), px(50));
        assert_eq!(frame.bounds(second).width(), px(50));
    }

    #[test]
    fn visual_traversal_size_does_not_scale_with_node_capacity() {
        use core::mem::size_of;

        let small = size_of::<VisualTraversal<'static, 8, 128>>();
        let large = size_of::<VisualTraversal<'static, 2048, 128>>();

        assert_eq!(small, large,);
        assert!(large <= 1024, "visual traversal grew to {large} bytes",);
    }

    struct DeepBranching {
        depth: usize,
    }

    impl Element for DeepBranching {
        fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
            let root = div().w(px(100)).h(px(1)).mount(cx)?;
            if self.depth == 0 {
                return Ok(root);
            }

            let child = DeepBranching {
                depth: self.depth - 1,
            }
            .mount(cx)?;
            cx.append_child(root, child);

            let sibling = div().w(px(100)).h(px(1)).mount(cx)?;
            cx.append_child(root, sibling);

            Ok(root)
        }
    }

    #[test]
    fn visual_traversal_handles_branch_depth_beyond_inline_stack() {
        const EXTRA_DEPTH: usize = 8;

        let mut frame = FrameArena::<128, 128>::default();

        let root = frame
            .mount(DeepBranching {
                depth: super::VISUAL_CONTEXT_STACK_CAPACITY + EXTRA_DEPTH,
            })
            .unwrap();

        frame.layout(root, Size::new(px(100), px(1000)), &TestTextMeasurer);

        let mut current = Some(root);

        while let Some(node) = current {
            frame.node_mut(node).interaction.scroll_offset = Offset::new(px(0), px(1));

            current = frame.node(node).first_child;
        }

        for visual in frame.visual_nodes(root) {
            assert_eq!(visual.bounds(), frame.visual_bounds(visual.node(),),);
        }
    }

    #[test]
    fn visual_traversal_visits_branching_tree_in_depth_first_order() {
        let mut frame = FrameArena::<16, 128>::default();
        let mut cx = MountCx::new(&mut frame);

        let root = div()
            .child(div().child(div()).child(div()))
            .child(div().child(div()))
            .mount(&mut cx)
            .unwrap();

        let mut visited = heapless::Vec::<NodeId, 16>::new();

        for visual in frame.visual_nodes(root) {
            visited.push(visual.node()).unwrap();
        }

        let root_node = frame.node(root);

        let first = root_node.first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();

        let first_first = frame.node(first).first_child.unwrap();
        let first_second = frame.node(first_first).next_sibling.unwrap();
        let second_first = frame.node(second).first_child.unwrap();

        assert_eq!(
            visited.as_slice(),
            &[root, first, first_first, first_second, second, second_first,],
        );
    }
}
