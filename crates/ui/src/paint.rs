use crate::{
    Axis, CanvasDraw, CanvasPainter, Color, DamageRegion, FrameArena, ImageFit, ImageSource,
    NodeId, NodeKind, Offset, Pixels, Position, Rect, ResolvedTextStyle, TextMeasurer,
    callback_store::CallbackStore, count_metric, entity_store::EntityStore, flow_axis,
    visual::VisualNode,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BorderPaint {
    pub width: Pixels,
    pub color: Color,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BoxPaint {
    pub background: Option<Color>,
    pub border: Option<BorderPaint>,
    pub radius: Pixels,
}

pub trait Painter: TextMeasurer {
    type Error;

    fn draw_box(
        &mut self,
        bounds: Rect,
        paint: BoxPaint,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error>;

    fn draw_text(
        &mut self,
        text: &str,
        bounds: Rect,
        style: ResolvedTextStyle,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error>;

    fn draw_image(
        &mut self,
        source: ImageSource,
        bounds: Rect,
        fit: ImageFit,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error>;

    fn draw_canvas(
        &mut self,
        bounds: Rect,
        clip: Option<Rect>,
        draw: &mut dyn FnMut(Rect, &mut dyn CanvasPainter),
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy)]
struct PaintRuntime<'a> {
    entities: &'a dyn EntityStore,
    callbacks: &'a dyn CallbackStore,
}

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
    fn paint_node<P>(
        &self,
        node_id: NodeId,
        bounds: Rect,
        clip: Option<Rect>,
        runtime: Option<PaintRuntime<'_>>,
        painter: &mut P,
    ) -> Result<(), P::Error>
    where
        P: Painter,
    {
        let node = self.node(node_id);
        match node.kind {
            NodeKind::Div { .. } => {
                let style = node.style().expect("div node must have style");
                let border = match (style.border_width.is_positive(), style.border_color) {
                    (true, Some(color)) => Some(BorderPaint {
                        width: style.border_width,
                        color,
                    }),
                    _ => None,
                };

                painter.draw_box(
                    bounds,
                    BoxPaint {
                        background: style.background,
                        border,
                        radius: style.border_radius,
                    },
                    clip,
                )
            }
            NodeKind::Text { text } => {
                painter.draw_text(self.text(text), bounds, node.effective_text_style, clip)
            }
            NodeKind::Canvas { draw, .. } => {
                let mut invoke =
                    |local_bounds: Rect, canvas_painter: &mut dyn CanvasPainter| match draw {
                        CanvasDraw::Static(draw) => draw(local_bounds, canvas_painter),
                        CanvasDraw::Entity(callback) => {
                            let Some(runtime) = runtime else {
                                debug_assert!(
                                    false,
                                    "entity canvas painted without callback context",
                                );
                                return;
                            };

                            let result = runtime.callbacks.invoke_canvas(
                                callback,
                                local_bounds,
                                canvas_painter,
                                runtime.entities,
                            );

                            debug_assert!(
                                result.is_ok(),
                                "entity canvas callback invocation failed: {result:?}",
                            );
                        }
                    };

                painter.draw_canvas(bounds, clip, &mut invoke)
            }

            NodeKind::Image { source, style } => {
                painter.draw_image(source, bounds, style.fit, clip)
            }
            NodeKind::Entity { .. } => Ok(()),
        }
    }

    fn paint_visual_node<P>(
        &self,
        visual: VisualNode,
        damage: DamageRegion,
        runtime: Option<PaintRuntime<'_>>,
        painter: &mut P,
    ) -> Result<(), P::Error>
    where
        P: Painter,
    {
        count_metric!(self, visual_nodes_visited);
        if !visual.is_visible() {
            return Ok(());
        }

        count_metric!(self, visible_nodes);
        let node_id = visual.node();
        if matches!(self.node(node_id).kind, NodeKind::Entity { .. }) {
            return Ok(());
        }

        let bounds = visual.bounds();

        // full damage preserves the existing paiting behavior exactly, one paint
        // invocation using the visual traversal's normal clip
        if damage.is_full() {
            count_metric!(self, nodes_painted);
            return self.paint_node(node_id, bounds, visual.clip(), runtime, painter);
        }

        debug_assert!(
            !damage.is_none(),
            "empty damage must be rejected before visual traversal"
        );

        let mut painted = false;

        for &damage_rect in damage.rects() {
            // first restrict the dirty rectangle to the node's own painted bounds
            let Some(mut clip) = bounds.intersection(damage_rect) else {
                continue;
            };
            // then preserve any clipping inherited from overflow/scroll ancestors
            if let Some(visual_clip) = visual.clip() {
                let Some(intersection) = clip.intersection(visual_clip) else {
                    continue;
                };

                clip = intersection;
            }

            if !painted {
                count_metric!(self, nodes_painted);
                painted = true;
            }
            count_metric!(self, damage_clip_paints);
            self.paint_node(node_id, bounds, Some(clip), runtime, painter)?;
        }

        if !painted {
            count_metric!(self, damage_culled_nodes);
        }

        Ok(())
    }

    fn visual_subtree_paint_bounds(&self, visual: VisualNode) -> Option<Rect> {
        let node = visual.node();
        let subtree = self.subtree_paint_bounds(node)?;
        let layout_bounds = self.node(node).layout.bounds;
        let translation = visual.bounds().origin - layout_bounds.origin;

        Some(subtree.translated(translation))
    }

    fn direct_child_for_mount_index(&self, parent: NodeId, index: usize) -> Option<NodeId> {
        let raw = u16::try_from(index).ok()?;
        let mut current = NodeId::new(raw);
        if current.index() >= self.nodes.len() {
            return None;
        }

        loop {
            let ancestor = self.node(current).parent?;
            if ancestor == parent {
                return Some(current);
            }

            current = ancestor;
        }
    }

    fn ordered_prefix_ends_before_damage(
        &self,
        child: NodeId,
        axis: Axis,
        child_translation: Offset,
        damage_bounds: Rect,
    ) -> Option<bool> {
        let prefix = self.ordered_prefix_paint_bounds(child)?;

        // an empty prefix cannot affect any damage and is therefore always safe to skip
        if !prefix.has_area() {
            return Some(true);
        }

        let prefix = prefix.translated(child_translation);

        Some(match axis {
            Axis::Horizontal => prefix.right() <= damage_bounds.x(),
            Axis::Vertical => prefix.bottom() <= damage_bounds.y(),
        })
    }

    fn first_ordered_child_reaching_damage(
        &self,
        visual: VisualNode,
        damage_bounds: Rect,
    ) -> Option<NodeId> {
        let parent = visual.node();
        let style = self.node(parent).style()?;
        let first = self.node(parent).first_child?;
        let last = self.node(parent).last_child?;
        if first == last {
            return Some(first);
        }

        debug_assert!(
            first.index() < last.index(),
            "ordering sibling roots must follow mount order"
        );

        let axis = flow_axis(style);

        // `visual.bounds()` already contains translation from every ancestor scroll offset
        let parent_layout_bounds = self.node(parent).layout.bounds;
        let parent_translation = visual.bounds().origin - parent_layout_bounds.origin;
        // children additionally receive this parent's own scroll translation
        let child_translation = parent_translation - self.node(parent).interaction.scroll_offset;

        // search the physical mount interval before the last sibling root.
        // every point in this interval belongs to one of the non-last direct-child
        // mount subtrees. A midpoint may land on a descendent. The helper maps it back
        // to the appropriate direct child.
        // the last child is excluded because its cache is deliberately exact rather
        // than cumulative
        let mut low = first.index();
        let mut high = last.index();

        while low < high {
            count_metric!(self, damage_prefix_search_steps);

            let mid = low + (high - low) / 2;
            let child = self.direct_child_for_mount_index(parent, mid)?;
            let before_damage = self.ordered_prefix_ends_before_damage(
                child,
                axis,
                child_translation,
                damage_bounds,
            )?;

            if before_damage {
                low = mid + 1;
            } else {
                high = mid;
            }
        }

        // if every non-last prefix lies before damage, the last child is our
        // conservative fallback
        if low == last.index() {
            return Some(last);
        }

        self.direct_child_for_mount_index(parent, low)
    }

    fn paint_internal<P>(
        &self,
        root: NodeId,
        runtime: Option<PaintRuntime<'_>>,
        damage: DamageRegion,
        painter: &mut P,
    ) -> Result<(), P::Error>
    where
        P: Painter,
    {
        // empty damage should cost literally nothing: not even a visual-tree traversal
        if damage.is_none() {
            return Ok(());
        }

        // full painting never needs any of the partial damage pruning machinery.
        // for partial painting this is computed exactly once, rather than scanning
        // the bounded damage region for every node
        let damage_bounds = if damage.is_full() {
            None
        } else {
            Some(damage.partial_damage_bounds())
        };

        // ordered sibling pruning assumes that source-order siblings preserve their
        // geometric flow order.
        // relative and absolute positioning can violate that assumption. Disable only
        // the ordered-sibling optimizations in positioned frames. Clip and subtree
        // extent pruning remain active.
        // this is intentionally conservative. Positioning correctness matters more
        // than recovering this optimization with additional metadata
        let ordered_sibling_pruning = !self.has_non_monotonic_positioning();

        let mut traversal = self.visual_nodes(root);

        while let Some(visual) = traversal.next() {
            // pruning only matters when there is actually a descendant subtree to skip.
            // In particular, don't inflate pruning metrics for leaf nodes
            let node = visual.node();
            let first_child = self.node(node).first_child;
            let has_children = first_child.is_some();

            if let Some(damage_bounds) = damage_bounds {
                // if this sibling begins after the far edge of all damage on the parent's
                // ordered flow axis, neither this node's descendants nor any following
                // sibling can contribute.
                if ordered_sibling_pruning
                    && self.ordered_sibling_tail_starts_after_damage(visual, damage_bounds)
                {
                    traversal.skip_children_and_remaining_siblings();
                    count_metric!(self, damage_pruned_sibling_runs);

                    // keep `damage_pruned_subtrees` meaning "a yielded node had its
                    // descendants supressed".
                    // do not attempt to count the sibling subtrees that disappeared:
                    // doing so would require traversing them
                    if has_children {
                        count_metric!(self, damage_pruned_subtrees);
                    }
                } else if has_children {
                    // a finite inherited clip bounds every descendant. If the clip misses
                    // damage, the entire descendant subtree is irrelevant
                    let clip_misses_damage = !visual.clip_region().intersects_damage(damage);
                    if clip_misses_damage {
                        traversal.skip_children();
                        count_metric!(self, damage_pruned_subtrees);
                    } else {
                        // the cached subtree extent conservatively contains every pixel this
                        // node or any of its descendants can affect.
                        // `None` means layout has not established the cache yet, so pruning
                        // is not allowed
                        let extent_misses_damage = match self.visual_subtree_paint_bounds(visual) {
                            Some(bounds) => !damage.intersects_rect(bounds),
                            None => false,
                        };
                        if extent_misses_damage {
                            traversal.skip_children();
                            count_metric!(self, damage_pruned_subtrees);
                            count_metric!(self, damage_extent_pruned_subtrees);
                        } else if ordered_sibling_pruning {
                            // the subtree does intersect damage, but an ordered prefix
                            // of its direct children may still lie completely before it
                            if let Some(first_relevant) =
                                self.first_ordered_child_reaching_damage(visual, damage_bounds)
                                && Some(first_relevant) != first_child
                            {
                                traversal.skip_children_before(first_relevant);
                                count_metric!(self, damage_pruned_sibling_prefixes);
                            }
                        }
                    }
                }
            }

            // pruning only changes future traversal.
            // the currently yielded node is still painted or culled normally
            self.paint_visual_node(visual, damage, runtime, painter)?;
        }

        Ok(())
    }

    pub fn paint<P>(&self, root: NodeId, painter: &mut P) -> Result<(), P::Error>
    where
        P: Painter,
    {
        self.paint_internal(root, None, DamageRegion::full(), painter)
    }

    pub fn paint_with_damage<P>(
        &self,
        root: NodeId,
        damage: DamageRegion,
        painter: &mut P,
    ) -> Result<(), P::Error>
    where
        P: Painter,
    {
        self.paint_internal(root, None, damage, painter)
    }

    pub(crate) fn paint_with_runtime<P>(
        &self,
        root: NodeId,
        entities: &dyn EntityStore,
        callbacks: &dyn CallbackStore,
        painter: &mut P,
    ) -> Result<(), P::Error>
    where
        P: Painter,
    {
        let runtime = PaintRuntime {
            entities,
            callbacks,
        };

        self.paint_internal(root, Some(runtime), DamageRegion::full(), painter)
    }

    pub(crate) fn paint_with_runtime_and_damage<P>(
        &self,
        root: NodeId,
        entities: &dyn EntityStore,
        callbacks: &dyn CallbackStore,
        damage: DamageRegion,
        painter: &mut P,
    ) -> Result<(), P::Error>
    where
        P: Painter,
    {
        let runtime = PaintRuntime {
            entities,
            callbacks,
        };

        self.paint_internal(root, Some(runtime), damage, painter)
    }

    fn ordered_sibling_tail_starts_after_damage(
        &self,
        visual: VisualNode,
        damage_bounds: Rect,
    ) -> bool {
        let node = visual.node();
        if self.node(node).next_sibling.is_none() {
            return false;
        }
        let Some(parent) = self.node(node).parent else {
            return false;
        };
        let Some(style) = self.node(parent).style() else {
            return false;
        };

        let bounds = visual.bounds();
        match flow_axis(style) {
            crate::Axis::Horizontal => bounds.x() >= damage_bounds.right(),
            crate::Axis::Vertical => bounds.y() >= damage_bounds.bottom(),
        }
    }

    fn has_non_monotonic_positioning(&self) -> bool {
        self.nodes.iter().any(|node| {
            let Some(style) = node.style() else {
                return false;
            };

            match style.position {
                Position::Static => false,
                Position::Absolute => true,
                Position::Relative => {
                    style.inset.top.is_some()
                        || style.inset.right.is_some()
                        || style.inset.bottom.is_some()
                        || style.inset.left.is_some()
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use core::convert::Infallible;
    use std::{vec, vec::Vec};

    use crate::*;

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
            fit: ImageFit,
            clip: Option<Rect>,
        },
        Canvas {
            bounds: Rect,
            clip: Option<Rect>,
        },
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum CanvasCommand {
        FillRect {
            rect: Rect,
            color: Color,
        },

        StrokeRect {
            rect: Rect,
            width: Pixels,
            color: Color,
        },

        Line {
            start: Point,
            end: Point,
            width: Pixels,
            color: Color,
        },

        FillCircle {
            center: Point,
            radius: Pixels,
            color: Color,
        },

        StrokeCircle {
            center: Point,
            radius: Pixels,
            width: Pixels,
            color: Color,
        },
    }

    #[derive(Default)]
    struct RecordingPainter {
        commands: Vec<Command>,
        canvas_commands: Vec<CanvasCommand>,
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
            fit: ImageFit,
            clip: Option<Rect>,
        ) -> Result<(), Self::Error> {
            self.commands.push(Command::Image {
                source,
                bounds,
                fit,
                clip,
            });

            Ok(())
        }

        fn draw_canvas(
            &mut self,
            bounds: Rect,
            clip: Option<Rect>,
            draw: &mut dyn FnMut(Rect, &mut dyn CanvasPainter),
        ) -> Result<(), Self::Error> {
            self.commands.push(Command::Canvas { bounds, clip });

            let local_bounds = Rect::new(Point::ZERO, Size::new(bounds.width(), bounds.height()));

            let mut painter = RecordingCanvasPainter {
                commands: &mut self.canvas_commands,
            };

            draw(local_bounds, &mut painter);

            Ok(())
        }
    }

    struct RecordingCanvasPainter<'a> {
        commands: &'a mut Vec<CanvasCommand>,
    }

    impl CanvasPainter for RecordingCanvasPainter<'_> {
        fn fill_rect(&mut self, rect: Rect, color: Color) {
            self.commands.push(CanvasCommand::FillRect { rect, color });
        }

        fn stroke_rect(&mut self, rect: Rect, width: Pixels, color: Color) {
            self.commands
                .push(CanvasCommand::StrokeRect { rect, width, color });
        }

        fn line(&mut self, start: Point, end: Point, width: Pixels, color: Color) {
            self.commands.push(CanvasCommand::Line {
                start,
                end,
                width,
                color,
            });
        }

        fn fill_circle(&mut self, center: Point, radius: Pixels, color: Color) {
            self.commands.push(CanvasCommand::FillCircle {
                center,
                radius,
                color,
            });
        }

        fn stroke_circle(&mut self, center: Point, radius: Pixels, width: Pixels, color: Color) {
            self.commands.push(CanvasCommand::StrokeCircle {
                center,
                radius,
                width,
                color,
            });
        }
    }

    fn draw_test_canvas(bounds: Rect, painter: &mut dyn CanvasPainter) {
        painter.fill_rect(
            Rect::new(Point::ZERO, Size::new(bounds.width(), px(4))),
            Color::RED,
        );
        painter.line(
            Point::new(px(0), px(0)),
            Point::new(bounds.width() - px(1), bounds.height() - px(1)),
            px(1),
            Color::GREEN,
        );
        painter.fill_circle(Point::new(px(10), px(10)), px(3), Color::BLUE);
    }

    #[test]
    fn frame_paints_backend_independent_commands_in_tree_order() {
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(80))
                    .h(px(40))
                    .bg(Color::RED)
                    .child(div().w(px(20)).h(px(10)).bg(Color::BLUE))
                    .child("Hi"),
            )
            .unwrap();

        let mut painter = RecordingPainter::default();
        frame.layout(root, Size::new(px(80), px(40)), &painter);
        frame.paint(root, &mut painter).unwrap();

        assert_eq!(painter.commands.len(), 3);
        assert_eq!(
            painter.commands[0],
            Command::Box {
                bounds: Rect::new(Point::new(px(0), px(0),), Size::new(px(80), px(40),),),
                paint: BoxPaint {
                    background: Some(Color::RED),
                    border: None,
                    radius: px(0),
                },
                clip: None
            }
        );

        assert_eq!(
            painter.commands[1],
            Command::Box {
                bounds: Rect::new(Point::new(px(0), px(0),), Size::new(px(20), px(10),),),
                paint: BoxPaint {
                    background: Some(Color::BLUE),
                    border: None,
                    radius: px(0),
                },
                clip: None
            }
        );

        assert_eq!(
            painter.commands[2],
            Command::Text {
                bounds: Rect::new(Point::new(px(0), px(10)), Size::new(px(12), px(10))),
                length: 2,
                style: ResolvedTextStyle::default(),
                clip: None
            }
        );
    }

    #[test]
    fn frame_converts_style_to_box_paint() {
        let mut frame = FrameArena::<8, 64>::default();

        let root = frame
            .mount(
                div()
                    .w(px(40))
                    .h(px(20))
                    .bg(Color::BLUE)
                    .border(px(2))
                    .border_color(Color::RED)
                    .rounded(px(6)),
            )
            .unwrap();

        let mut painter = RecordingPainter::default();
        frame.layout(root, Size::new(px(40), px(20)), &painter);
        frame.paint(root, &mut painter).unwrap();

        assert_eq!(
            painter.commands,
            vec![Command::Box {
                bounds: Rect::new(Point::new(px(0), px(0),), Size::new(px(40), px(20),),),
                paint: BoxPaint {
                    background: Some(Color::BLUE),
                    border: Some(crate::BorderPaint {
                        width: px(2),
                        color: Color::RED,
                    }),
                    radius: px(6),
                },
                clip: None
            }]
        );
    }

    #[test]
    fn overflow_hidden_clips_descendant_painting() {
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(50))
                    .h(px(30))
                    .overflow_hidden()
                    .child(div().w(px(100)).h(px(20)).bg(Color::RED)),
            )
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(root, Size::new(px(100), px(100)), &painter);
        frame.paint(root, &mut painter).unwrap();

        let child = frame.node(root).first_child.unwrap();
        let expected_clip = frame.bounds(root);

        assert_eq!(
            painter.commands[1],
            Command::Box {
                bounds: frame.bounds(child),
                paint: BoxPaint {
                    background: Some(Color::RED),
                    border: None,
                    radius: px(0),
                },
                clip: Some(expected_clip),
            }
        );
    }

    #[test]
    fn overflow_hidden_clips_children_inside_parent_border() {
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(50))
                    .h(px(30))
                    .border(px(2))
                    .border_color(Color::WHITE)
                    .overflow_hidden()
                    .child(div().w(px(100)).h(px(100)).bg(Color::RED)),
            )
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(root, Size::new(px(100), px(100)), &painter);
        frame.paint(root, &mut painter).unwrap();

        let child = frame.node(root).first_child.unwrap();

        assert_eq!(
            painter.commands[1],
            Command::Box {
                bounds: frame.visual_bounds(child),
                paint: BoxPaint {
                    background: Some(Color::RED),
                    border: None,
                    radius: px(0),
                },
                clip: Some(Rect::new(
                    Point::new(px(2), px(2),),
                    Size::new(px(46), px(26),),
                )),
            }
        );
    }

    #[test]
    fn painting_receives_resolved_text_style() {
        let mut frame = FrameArena::<8, 64>::default();

        let root = frame
            .mount(
                div()
                    .font(FontId::new(1))
                    .text_color(Color::WHITE)
                    .line_height(px(16))
                    .child(text("Hello").text_color(Color::RED)),
            )
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(root, Size::new(px(100), px(100)), &painter);
        frame.paint(root, &mut painter).unwrap();

        assert_eq!(
            painter.commands[1],
            Command::Text {
                bounds: Rect::new(Point::ZERO, Size::new(px(30), px(10),),),
                length: "Hello".len(),
                style: ResolvedTextStyle {
                    font: FontId::new(1),
                    color: Color::RED,
                    line_height: LineHeight::Pixels(px(16)),
                    ..Default::default()
                },
                clip: None,
            }
        );
    }

    #[test]
    fn frame_emits_image_paint_command() {
        let source = ImageSource::new(ImageId::new(0), Size::new(px(20), px(12)));

        let mut frame = FrameArena::<8, 128>::default();

        let root = frame
            .mount(div().w(px(80)).h(px(40)).child(image(source)))
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(root, Size::new(px(80), px(40)), &painter);
        frame.paint(root, &mut painter).unwrap();

        assert_eq!(painter.commands.len(), 2);
        assert_eq!(
            painter.commands[1],
            Command::Image {
                source,
                bounds: Rect::new(Point::new(px(0), px(0),), Size::new(px(20), px(12),),),
                fit: ImageFit::None,
                clip: None,
            }
        );
    }

    #[test]
    fn canvas_callback_draws_in_local_coordinates() {
        let mut frame = FrameArena::<8, 128>::default();

        let root = frame
            .mount(
                div()
                    .p(px(10))
                    .child(canvas(draw_test_canvas).size(Size::new(px(30), px(20)))),
            )
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(root, Size::new(px(100), px(100)), &painter);
        frame.paint(root, &mut painter).unwrap();

        assert_eq!(
            painter.canvas_commands[0],
            CanvasCommand::FillRect {
                rect: Rect::new(Point::ZERO, Size::new(px(30), px(4),),),
                color: Color::RED,
            }
        );
        assert_eq!(
            painter.canvas_commands[1],
            CanvasCommand::Line {
                start: Point::ZERO,
                end: Point::new(px(29), px(19),),
                width: px(1),
                color: Color::GREEN,
            }
        );
        assert_eq!(
            painter.canvas_commands[2],
            CanvasCommand::FillCircle {
                center: Point::new(px(10), px(10),),
                radius: px(3),
                color: Color::BLUE,
            }
        );
    }

    #[test]
    fn entity_canvas_can_draw_from_entity_state() {
        struct Gauge {
            value: Pixels,
        }

        impl Render for Gauge {
            fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
                div().w(px(100)).h(px(20)).child(
                    cx.canvas(|this, bounds, painter| {
                        painter.fill_rect(
                            Rect::new(Point::ZERO, Size::new(this.value, bounds.height())),
                            Color::GREEN,
                        );
                    })
                    .size(Size::new(px(100), px(20))),
                )
            }
        }

        let mut runtime = TestRuntime::default();

        let gauge = runtime.create(|_| Gauge { value: px(37) }).unwrap();

        runtime.rebuild(gauge).unwrap();

        let mut painter = RecordingPainter::default();

        runtime.layout(Size::new(px(100), px(20)), &painter);
        runtime.paint(&mut painter).unwrap();

        assert_eq!(
            painter.canvas_commands,
            vec![CanvasCommand::FillRect {
                rect: Rect::new(Point::ZERO, Size::new(px(37), px(20),),),
                color: Color::GREEN,
            },]
        );
    }

    type TestRuntime = Runtime<4096, 16, 4096, 32, 64, 512, 32>;

    #[test]
    fn entity_canvas_callback_can_capture_render_data() {
        struct Gauge {
            value: Pixels,
            inset: Pixels,
        }

        impl Render for Gauge {
            fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
                let inset = self.inset;

                div().child(
                    cx.canvas(move |this, bounds, painter| {
                        painter.fill_rect(
                            Rect::new(
                                Point::new(inset, Pixels::ZERO),
                                Size::new(this.value, bounds.height()),
                            ),
                            Color::BLUE,
                        );
                    })
                    .size(Size::new(px(100), px(20))),
                )
            }
        }

        let mut runtime = TestRuntime::default();

        let gauge = runtime
            .create(|_| Gauge {
                value: px(25),
                inset: px(4),
            })
            .unwrap();

        runtime.rebuild(gauge).unwrap();

        let mut painter = RecordingPainter::default();

        runtime.layout(Size::new(px(100), px(20)), &painter);
        runtime.paint(&mut painter).unwrap();

        assert_eq!(
            painter.canvas_commands,
            vec![CanvasCommand::FillRect {
                rect: Rect::new(Point::new(px(4), px(0),), Size::new(px(25), px(20),),),
                color: Color::BLUE,
            },]
        );
    }

    #[test]
    fn partial_damage_skips_non_intersecting_nodes() {
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(60))
                    .bg(Color::BLACK)
                    .child(div().w(px(100)).h(px(20)).bg(Color::RED))
                    .child(div().w(px(100)).h(px(20)).bg(Color::BLUE))
                    .child(div().w(px(100)).h(px(20)).bg(Color::GREEN)),
            )
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(root, Size::new(px(100), px(60)), &painter);

        let damage = Rect::new(Point::new(px(0), px(25)), Size::new(px(100), px(10)));

        frame
            .paint_with_damage(root, DamageRegion::from_rect(damage), &mut painter)
            .unwrap();

        assert_eq!(painter.commands.len(), 2,);
        assert_eq!(
            painter.commands[0],
            Command::Box {
                bounds: Rect::new(Point::ZERO, Size::new(px(100), px(60),),),
                paint: BoxPaint {
                    background: Some(Color::BLACK),
                    border: None,
                    radius: px(0),
                },
                clip: Some(damage),
            },
        );
        assert_eq!(
            painter.commands[1],
            Command::Box {
                bounds: Rect::new(Point::new(px(0), px(20),), Size::new(px(100), px(20),),),
                paint: BoxPaint {
                    background: Some(Color::BLUE),
                    border: None,
                    radius: px(0),
                },
                clip: Some(damage),
            },
        );
    }

    #[test]
    fn disjoint_damage_rectangles_produce_disjoint_paint_clips() {
        let mut frame = FrameArena::<8, 64>::default();

        let root = frame
            .mount(div().w(px(100)).h(px(40)).bg(Color::BLUE))
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(root, Size::new(px(100), px(40)), &painter);

        let first = Rect::new(Point::new(px(0), px(0)), Size::new(px(10), px(10)));
        let second = Rect::new(Point::new(px(80), px(20)), Size::new(px(10), px(10)));

        let damage = DamageRegion::none().add_rect(first).add_rect(second);

        frame.paint_with_damage(root, damage, &mut painter).unwrap();

        assert_eq!(
            painter.commands,
            vec![
                Command::Box {
                    bounds: Rect::new(Point::ZERO, Size::new(px(100), px(40),),),
                    paint: BoxPaint {
                        background: Some(Color::BLUE),
                        border: None,
                        radius: px(0),
                    },
                    clip: Some(first),
                },
                Command::Box {
                    bounds: Rect::new(Point::ZERO, Size::new(px(100), px(40),),),
                    paint: BoxPaint {
                        background: Some(Color::BLUE),
                        border: None,
                        radius: px(0),
                    },
                    clip: Some(second),
                },
            ],
        );
    }

    #[test]
    fn partial_damage_respects_existing_visual_clip() {
        let mut frame = FrameArena::<8, 64>::default();

        let root = frame
            .mount(
                div()
                    .w(px(40))
                    .h(px(20))
                    .overflow_hidden()
                    .child(div().w(px(80)).h(px(20)).bg(Color::RED)),
            )
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(root, Size::new(px(100), px(40)), &painter);

        // the child physically extends to x=80, but its visible portion ends at x=40.
        let damage = DamageRegion::from_rect(Rect::new(
            Point::new(px(50), px(0)),
            Size::new(px(10), px(20)),
        ));

        frame.paint_with_damage(root, damage, &mut painter).unwrap();

        assert!(painter.commands.is_empty());
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn empty_damage_performs_no_visual_work() {
        let mut frame = FrameArena::<8, 64>::default();

        let root = frame
            .mount(div().w(px(100)).h(px(40)).bg(Color::BLUE))
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(root, Size::new(px(100), px(40)), &painter);
        frame.reset_performance_metrics();
        frame
            .paint_with_damage(root, DamageRegion::none(), &mut painter)
            .unwrap();

        let metrics = frame.performance_metrics();

        assert_eq!(metrics.visual_traversal_passes, 0,);
        assert_eq!(metrics.visual_traversal_nodes, 0,);
        assert_eq!(metrics.visual_nodes_visited, 0,);
        assert_eq!(metrics.nodes_painted, 0,);
        assert!(painter.commands.is_empty());
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn damage_paint_prunes_subtree_when_inherited_clip_misses_damage() {
        struct App;

        impl Render for App {
            fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
                div().w(px(100)).h(px(100)).bg(Color::BLACK).child(
                    div().w(px(40)).h(px(20)).overflow_hidden().child(
                        div()
                            .w(px(80))
                            .h(px(20))
                            .bg(Color::RED)
                            .child(div().w(px(80)).h(px(10)).bg(Color::BLUE)),
                    ),
                )
            }
        }

        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| App).unwrap();

        runtime.rebuild(app).unwrap();

        let mut painter = RecordingPainter::default();

        runtime
            .layout(Size::new(px(100), px(100)), &painter)
            .unwrap();

        // Damage lies outside the 40px-wide overflow
        // viewport.
        //
        // The wide red child geometrically reaches this
        // damage rectangle, but its inherited visual clip
        // does not. Therefore its descendants can safely
        // be pruned.
        let damage = DamageRegion::from_rect(Rect::new(
            Point::new(px(60), px(0)),
            Size::new(px(10), px(10)),
        ));

        runtime.reset_performance_metrics();
        runtime
            .paint_with_damage(damage, &mut painter)
            .unwrap()
            .unwrap();

        let metrics = runtime.performance_metrics();

        assert_eq!(metrics.damage_pruned_subtrees, 1,);
        assert!(
            metrics.visual_traversal_nodes
                < u64::try_from(runtime.frame_node_count(),).unwrap_or(u64::MAX),
        );

        // only the black root intersects the damage.
        // the clipped red subtree must produce no command.

        assert_eq!(painter.commands.len(), 1,);
        assert_eq!(
            painter.commands[0],
            Command::Box {
                bounds: Rect::new(Point::ZERO, Size::new(px(100), px(100),),),
                paint: BoxPaint {
                    background: Some(Color::BLACK),
                    border: None,
                    radius: px(0),
                },
                clip: Some(Rect::new(
                    Point::new(px(60), px(0),),
                    Size::new(px(10), px(10),),
                ),),
            },
        );
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn damage_paint_prunes_off_damage_child_subtrees_by_extent() {
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(80))
                    .bg(Color::BLACK)
                    .child(div().w(px(100)).h(px(20)).bg(Color::RED).child("A"))
                    .child(div().w(px(100)).h(px(20)).bg(Color::BLUE).child("B"))
                    .child(div().w(px(100)).h(px(20)).bg(Color::GREEN).child("C"))
                    .child(div().w(px(100)).h(px(20)).bg(Color::WHITE).child("D")),
            )
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(root, Size::new(px(100), px(80)), &painter);

        let damage = DamageRegion::from_rect(Rect::new(
            Point::new(px(0), px(45)),
            Size::new(px(100), px(5)),
        ));

        frame.reset_performance_metrics();
        frame.paint_with_damage(root, damage, &mut painter).unwrap();

        let metrics = frame.performance_metrics();

        // rows A, B and D miss damage.
        // their text children should never be traversed.
        assert_eq!(metrics.damage_pruned_sibling_prefixes, 1,);
        assert!(metrics.damage_prefix_search_steps > 0,);
        assert_eq!(metrics.damage_extent_pruned_subtrees, 1,);
        assert_eq!(metrics.damage_pruned_subtrees, 1,);
        assert_eq!(metrics.damage_pruned_sibling_runs, 0,);

        // root
        // row A
        // row B
        // row C
        // text C
        // row D
        //
        // the other three text nodes were skipped.
        assert_eq!(metrics.visual_traversal_nodes, 4,);
        assert_eq!(metrics.nodes_painted, 3,);
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn partial_paint_before_layout_does_not_use_stale_subtree_bounds() {
        let mut frame = FrameArena::<16, 128>::default();

        let first_root = frame
            .mount(
                div()
                    .w(px(20))
                    .h(px(20))
                    .child(div().w(px(20)).h(px(20)).child("old")),
            )
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(first_root, Size::new(px(100), px(100)), &painter);
        frame.clear();

        let second_root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(100))
                    .child(div().w(px(100)).h(px(100)).bg(Color::RED).child("new")),
            )
            .unwrap();

        frame.reset_performance_metrics();

        let damage = DamageRegion::from_rect(Rect::new(
            Point::new(px(80), px(80)),
            Size::new(px(10), px(10)),
        ));

        frame
            .paint_with_damage(second_root, damage, &mut painter)
            .unwrap();

        let metrics = frame.performance_metrics();

        // the old layout cache must not be trusted after rebuilding a new tree that
        // has not yet been laid out.
        assert_eq!(metrics.damage_extent_pruned_subtrees, 0,);
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn damage_paint_prunes_ordered_vertical_sibling_tail() {
        let mut frame = FrameArena::<32, 256>::default();

        let root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(120))
                    .bg(Color::BLACK)
                    .child(div().w(px(100)).h(px(20)).bg(Color::RED).child("A"))
                    .child(div().w(px(100)).h(px(20)).bg(Color::RED).child("B"))
                    .child(div().w(px(100)).h(px(20)).bg(Color::RED).child("C"))
                    .child(div().w(px(100)).h(px(20)).bg(Color::RED).child("D"))
                    .child(div().w(px(100)).h(px(20)).bg(Color::RED).child("E"))
                    .child(div().w(px(100)).h(px(20)).bg(Color::RED).child("F")),
            )
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(root, Size::new(px(100), px(120)), &painter);

        // only row B intersects:
        //
        // A  0..20
        // B 20..40   <- damage 25..35
        // C 40..60   <- suffix sentinel
        // D 60..80
        // E 80..100
        // F 100..120
        let damage = DamageRegion::from_rect(Rect::new(
            Point::new(px(0), px(25)),
            Size::new(px(100), px(10)),
        ));

        frame.reset_performance_metrics();
        frame.paint_with_damage(root, damage, &mut painter).unwrap();

        let metrics = frame.performance_metrics();

        // A disappears before being yielded
        assert_eq!(metrics.damage_pruned_sibling_prefixes, 1,);
        assert!(metrics.damage_prefix_search_steps > 0,);
        // C has a child, so suppressing C's descendants counts as one prune subtree
        assert_eq!(metrics.damage_pruned_subtrees, 1,);
        // C starts after the damage and removes the whole remaining sibling suffix
        assert_eq!(metrics.damage_pruned_sibling_runs, 1);
        // A no longer individually pruned
        assert_eq!(metrics.damage_extent_pruned_subtrees, 0,);

        // root
        // B
        // text B
        // C
        assert_eq!(metrics.visual_traversal_nodes, 4);

        // root + B + text B
        assert_eq!(metrics.nodes_painted, 3);
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn damage_paint_prunes_ordered_horizontal_leaf_sibling_tail() {
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .flex_row()
                    .w(px(120))
                    .h(px(20))
                    .bg(Color::BLACK)
                    .child(div().w(px(20)).h(px(20)).bg(Color::RED))
                    .child(div().w(px(20)).h(px(20)).bg(Color::BLUE))
                    .child(div().w(px(20)).h(px(20)).bg(Color::GREEN))
                    .child(div().w(px(20)).h(px(20)).bg(Color::WHITE))
                    .child(div().w(px(20)).h(px(20)).bg(Color::RED))
                    .child(div().w(px(20)).h(px(20)).bg(Color::BLUE)),
            )
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(root, Size::new(px(120), px(20)), &painter);

        // child 0:  0..20   <- prefix
        // child 1: 20..40   <- damage 25..35
        // child 2: 40..60   <- suffix sentinel
        let damage = DamageRegion::from_rect(Rect::new(
            Point::new(px(25), px(0)),
            Size::new(px(10), px(20)),
        ));

        frame.reset_performance_metrics();
        frame.paint_with_damage(root, damage, &mut painter).unwrap();

        let metrics = frame.performance_metrics();

        // third child begins at x=40 while damage ends at x=35, so it terminates
        // the ordered sibling run.
        assert_eq!(metrics.damage_pruned_sibling_prefixes, 1);
        assert!(metrics.damage_prefix_search_steps > 0);
        assert_eq!(metrics.damage_pruned_sibling_runs, 1);

        // these are leaf nodes, so neither prefix nor suffix pruning suppresses
        // a descendant subtree.
        assert_eq!(metrics.damage_pruned_subtrees, 0,);
        assert_eq!(metrics.damage_extent_pruned_subtrees, 0,);

        // root
        // child 0
        // child 1
        // child 2
        assert_eq!(metrics.visual_traversal_nodes, 3);

        // root + child 1
        assert_eq!(metrics.nodes_painted, 2);
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn damage_paint_binary_searches_ordered_sibling_prefix() {
        let mut frame = FrameArena::<64, 512>::default();

        let root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(160))
                    .children((0..16).map(|_| div().w(px(100)).h(px(10)).child("row"))),
            )
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(root, Size::new(px(100), px(160)), &painter);

        let damage = DamageRegion::from_rect(Rect::new(
            Point::new(px(0), px(85)),
            Size::new(px(100), px(10)),
        ));

        frame.reset_performance_metrics();
        frame.paint_with_damage(root, damage, &mut painter).unwrap();

        let metrics = frame.performance_metrics();

        // rows 0..7 lie wholly before damage and should be jumped over without
        // being yielded.
        assert_eq!(metrics.damage_pruned_sibling_prefixes, 1,);
        assert!(metrics.damage_prefix_search_steps > 0,);

        // root
        // row 8
        // text 8
        // row 9
        // text 9
        // row 10  <- suffix sentinel
        assert_eq!(metrics.visual_traversal_nodes, 6,);
        // root + row/text 8 + row/text 9
        assert_eq!(metrics.nodes_painted, 5,);

        assert_eq!(metrics.damage_pruned_sibling_runs, 1,);
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn ordered_prefix_search_preserves_earlier_overflowing_subtree() {
        let mut frame = FrameArena::<32, 256>::default();

        let root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(120))
                    .child(
                        div()
                            .w(px(100))
                            .h(px(20))
                            .child(div().w(px(100)).h(px(100)).bg(Color::RED)),
                    )
                    .child(div().w(px(100)).h(px(20)))
                    .child(div().w(px(100)).h(px(20)))
                    .child(div().w(px(100)).h(px(20)))
                    .child(div().w(px(100)).h(px(20))),
            )
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(root, Size::new(px(100), px(120)), &painter);

        let damage_rect = Rect::new(Point::new(px(0), px(80)), Size::new(px(100), px(10)));

        let damage = DamageRegion::from_rect(damage_rect);

        frame.reset_performance_metrics();
        frame.paint_with_damage(root, damage, &mut painter).unwrap();

        let metrics = frame.performance_metrics();

        // the first sibling's own box ends at y=20, but its descendant paints through
        // y=100. The cumulative prefix cache must therefore prevent us from jumping
        // over it.
        assert_eq!(metrics.damage_pruned_sibling_prefixes, 0,);

        let painted_overflow = painter.commands.iter().any(|command| match command {
            Command::Box { paint, clip, .. } => {
                paint.background == Some(Color::RED) && *clip == Some(damage_rect)
            }

            _ => false,
        });

        assert!(
            painted_overflow,
            "overflowing earlier sibling must still paint into damage",
        );
    }

    #[test]
    fn partial_damage_keeps_relative_sibling_shifted_back_into_damage() {
        let mut frame = FrameArena::<32, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .w(px(300))
                    .h(px(40))
                    .child(div().w(px(100)).h(px(40)))
                    .child(div().w(px(100)).h(px(40)))
                    .child(
                        div()
                            .relative()
                            .right(px(190))
                            .w(px(100))
                            .h(px(40))
                            .bg(Color::BLUE),
                    ),
            )
            .unwrap();

        let mut painter = RecordingPainter::default();

        frame.layout(root, Size::new(px(300), px(40)), &painter);

        let damage = DamageRegion::from_rect(Rect::new(
            Point::new(px(10), px(0)),
            Size::new(px(10), px(40)),
        ));

        frame.paint_with_damage(root, damage, &mut painter).unwrap();

        assert!(
            painter.commands.iter().any(|command| {
                matches!(
                    command,
                    Command::Box {
                        bounds,
                        paint: BoxPaint {
                            background: Some(Color::BLUE),
                            ..
                        },
                        ..
                    } if *bounds
                        == Rect::new(
                            Point::new(px(10), px(0)),
                            Size::new(px(100), px(40)),
                        )
                )
            }),
            "relative sibling shifted backward into damage must still be painted",
        );
    }
}
