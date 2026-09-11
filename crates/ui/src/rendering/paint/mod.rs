use crate::{
    Axis, CanvasPainter, Color, DamageRegion, FrameArena, ImageColorMode, ImagePaint, ImageSource,
    NodeId, NodeKind, Offset, Pixels, Position, Rect, ResolvedTextStyle, callback::CallbackStore,
    count_metric, entity::EntityStore, flow_axis, visual::VisualNode,
};

mod canvas;
#[cfg(test)]
mod tests;

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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PaintedContent {
    text: bool,
    graphics: bool,
    monochrome_images: bool,
    continuous_tone_images: bool,
}

impl PaintedContent {
    pub const fn has_text(self) -> bool {
        self.text
    }

    pub const fn has_graphics(self) -> bool {
        self.graphics
    }

    pub const fn has_images(self) -> bool {
        self.monochrome_images || self.continuous_tone_images
    }

    pub const fn has_monochrome_images(self) -> bool {
        self.monochrome_images
    }

    pub const fn has_continuous_tone_images(self) -> bool {
        self.continuous_tone_images
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PaintReport {
    damage: DamageRegion,
    content: PaintedContent,
}

impl PaintReport {
    pub const fn new(damage: DamageRegion) -> Self {
        Self {
            damage,
            content: PaintedContent {
                text: false,
                graphics: false,
                monochrome_images: false,
                continuous_tone_images: false,
            },
        }
    }

    pub const fn damage(self) -> DamageRegion {
        self.damage
    }

    pub const fn content(self) -> PaintedContent {
        self.content
    }

    fn mark_text(&mut self) {
        self.content.text = true;
    }

    fn mark_graphics(&mut self) {
        self.content.graphics = true;
    }

    fn mark_image(&mut self, mode: ImageColorMode) {
        match mode {
            ImageColorMode::Monochrome => {
                self.content.monochrome_images = true;
            }
            ImageColorMode::Color | ImageColorMode::Grayscale => {
                self.content.continuous_tone_images = true;
            }
        }
    }
}

pub trait Painter {
    type Error;

    fn draw_box(
        &mut self,
        bounds: Rect,
        paint: BoxPaint,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error>;

    fn draw_canvas(
        &mut self,
        bounds: Rect,
        clip: Option<Rect>,
        draw: &mut dyn FnMut(Rect, &mut dyn CanvasPainter),
    ) -> Result<(), Self::Error>;
}

pub trait ResourcePainter<R: ?Sized = ()>: Painter {
    fn draw_text(
        &mut self,
        resources: &mut R,
        text: &str,
        bounds: Rect,
        style: ResolvedTextStyle,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error>;

    fn draw_image(
        &mut self,
        resources: &mut R,
        source: ImageSource,
        bounds: Rect,
        paint: ImagePaint,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy)]
struct PaintRuntime<'a> {
    entities: &'a dyn EntityStore,
    callbacks: &'a dyn CallbackStore,
}

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
    #[allow(clippy::too_many_arguments)]
    fn paint_node<R: ?Sized, P>(
        &self,
        node_id: NodeId,
        bounds: Rect,
        clip: Option<Rect>,
        runtime: Option<PaintRuntime<'_>>,
        resources: &mut R,
        report: &mut PaintReport,
        painter: &mut P,
    ) -> Result<(), P::Error>
    where
        P: ResourcePainter<R>,
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

                let paint = BoxPaint {
                    background: style.background,
                    border,
                    radius: style.border_radius,
                };

                painter.draw_box(bounds, paint, clip)?;

                if paint.background.is_some() || paint.border.is_some() {
                    report.mark_graphics();
                }

                Ok(())
            }

            NodeKind::Text { text } => {
                let text = self.text(text);

                painter.draw_text(resources, text, bounds, node.effective_text_style, clip)?;

                if !text.is_empty() {
                    report.mark_text();
                }

                Ok(())
            }

            NodeKind::Canvas { draw, .. } => canvas::paint_canvas(
                draw,
                bounds,
                clip,
                node.effective_text_style,
                runtime,
                resources,
                report,
                painter,
            ),

            NodeKind::Image { source, style } => {
                painter.draw_image(resources, source, bounds, style.paint, clip)?;

                report.mark_image(style.paint.color_mode);

                Ok(())
            }

            NodeKind::Entity { .. } => Ok(()),
        }
    }

    fn paint_visual_node<R: ?Sized, P>(
        &self,
        visual: VisualNode,
        damage: DamageRegion,
        runtime: Option<PaintRuntime<'_>>,
        resources: &mut R,
        report: &mut PaintReport,
        painter: &mut P,
    ) -> Result<(), P::Error>
    where
        P: ResourcePainter<R>,
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
            return self.paint_node(
                node_id,
                bounds,
                visual.clip(),
                runtime,
                resources,
                report,
                painter,
            );
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

            self.paint_node(
                node_id,
                bounds,
                Some(clip),
                runtime,
                resources,
                report,
                painter,
            )?;
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

    fn paint_internal<R: ?Sized, P>(
        &self,
        root: NodeId,
        runtime: Option<PaintRuntime<'_>>,
        damage: DamageRegion,
        resources: &mut R,
        painter: &mut P,
    ) -> Result<PaintReport, P::Error>
    where
        P: ResourcePainter<R>,
    {
        let mut report = PaintReport::new(damage);

        // empty damage should cost literally nothing: not even a visual-tree traversal
        if damage.is_none() {
            return Ok(report);
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

            self.paint_visual_node(visual, damage, runtime, resources, &mut report, painter)?;
        }

        Ok(report)
    }

    pub fn paint<P>(&self, root: NodeId, painter: &mut P) -> Result<PaintReport, P::Error>
    where
        P: ResourcePainter,
    {
        let mut resources = ();
        self.paint_internal(root, None, DamageRegion::full(), &mut resources, painter)
    }

    pub fn paint_with_damage<P>(
        &self,
        root: NodeId,
        damage: DamageRegion,
        painter: &mut P,
    ) -> Result<PaintReport, P::Error>
    where
        P: ResourcePainter,
    {
        let mut resources = ();
        self.paint_internal(root, None, damage, &mut resources, painter)
    }

    pub(crate) fn paint_with_runtime<R: ?Sized, P>(
        &self,
        root: NodeId,
        entities: &dyn EntityStore,
        callbacks: &dyn CallbackStore,
        resources: &mut R,
        painter: &mut P,
    ) -> Result<PaintReport, P::Error>
    where
        P: ResourcePainter<R>,
    {
        let runtime = PaintRuntime {
            entities,
            callbacks,
        };

        self.paint_internal(
            root,
            Some(runtime),
            DamageRegion::full(),
            resources,
            painter,
        )
    }

    pub(crate) fn paint_with_runtime_and_damage<R: ?Sized, P>(
        &self,
        root: NodeId,
        entities: &dyn EntityStore,
        callbacks: &dyn CallbackStore,
        resources: &mut R,
        damage: DamageRegion,
        painter: &mut P,
    ) -> Result<PaintReport, P::Error>
    where
        P: ResourcePainter<R>,
    {
        let runtime = PaintRuntime {
            entities,
            callbacks,
        };

        self.paint_internal(root, Some(runtime), damage, resources, painter)
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
