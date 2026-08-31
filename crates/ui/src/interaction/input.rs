use core::any::TypeId;

use crate::{
    DamageRegion, FrameArena, Invalidation, NodeId, Offset, Pixels, Point, Rect, count_metric,
    element,
    element::state::ElementStateId,
    px,
    interaction::scroll::{ScrollAxes, ScrollStateTable},
};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FocusTarget {
    pub(crate) element: ElementStateId,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ScrollTarget {
    pub(crate) node: NodeId,
    pub(crate) element: ElementStateId,
    pub(crate) axes: ScrollAxes,
    pub(crate) max_offset: Offset,
    pub(crate) damage: DamageRegion,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ActivationState {
    pressed: Option<ElementStateId>,
}

impl ActivationState {
    pub(crate) fn set_pressed(&mut self, element: Option<ElementStateId>) {
        self.pressed = element;
    }

    pub(crate) fn pressed(&self) -> Option<ElementStateId> {
        self.pressed
    }

    pub(crate) fn cancel(&mut self) {
        self.pressed = None;
    }
}

fn scroll_axis_into_view(
    current: Pixels,
    maximum: Pixels,
    target_start: Pixels,
    target_end: Pixels,
    viewport_start: Pixels,
    viewport_end: Pixels,
) -> Pixels {
    let maximum = maximum.non_negative();
    if viewport_end <= viewport_start {
        return current.clamp(px(0), maximum);
    }

    let delta = if target_start < viewport_start && target_end > viewport_end {
        // the target is larger than the viewport and already spans both edges
        // there is no position that can make it fully visible, so don't jump
        px(0)
    } else if target_start < viewport_start {
        target_start - viewport_start
    } else if target_end > viewport_end {
        target_end - viewport_end
    } else {
        px(0)
    };

    (current + delta).clamp(px(0), maximum)
}

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
    pub(crate) fn hit_test_event(
        &self,
        root: NodeId,
        position: Point,
        event_type: TypeId,
    ) -> Option<NodeId> {
        let mut hit = None;

        for visual in self.visual_nodes(root) {
            if !visual.contains(position) {
                continue;
            }

            let node = visual.node();
            if self.event_callbacks(node, event_type).next().is_some() {
                hit = Some(node);
            }
        }

        hit
    }

    fn focus_target_from_node(&self, node_id: NodeId) -> Option<FocusTarget> {
        let node = self.node(node_id);
        if !node.interaction.focusable {
            return None;
        }

        let element = node.element_state_id?;

        Some(FocusTarget { element })
    }

    pub(crate) fn focus_target_for_element(
        &self,
        root: NodeId,
        element: ElementStateId,
    ) -> Option<FocusTarget> {
        let mut current = Some(root);

        while let Some(node_id) = current {
            if let Some(target) = self.focus_target_from_node(node_id)
                && target.element == element
            {
                return Some(target);
            }

            current = self.next_depth_first_node(node_id);
        }

        None
    }

    fn first_focus_target(&self, root: NodeId) -> Option<FocusTarget> {
        let mut current = Some(root);

        while let Some(node_id) = current {
            if let Some(target) = self.focus_target_from_node(node_id) {
                return Some(target);
            }

            current = self.next_depth_first_node(node_id);
        }

        None
    }

    fn last_focus_target(&self, root: NodeId) -> Option<FocusTarget> {
        let mut current = Some(root);
        let mut last = None;

        while let Some(node_id) = current {
            if let Some(target) = self.focus_target_from_node(node_id) {
                last = Some(target);
            }

            current = self.next_depth_first_node(node_id);
        }

        last
    }

    pub(crate) fn next_focus_target(
        &self,
        root: NodeId,
        current_element: Option<ElementStateId>,
    ) -> Option<FocusTarget> {
        let first = self.first_focus_target(root)?;

        let Some(current_element) = current_element else {
            return Some(first);
        };

        let mut found_current = false;
        let mut current = Some(root);

        while let Some(node_id) = current {
            if let Some(target) = self.focus_target_from_node(node_id) {
                if found_current {
                    return Some(target);
                }

                if target.element == current_element {
                    found_current = true;
                }
            }

            current = self.next_depth_first_node(node_id);
        }

        Some(first)
    }

    pub(crate) fn previous_focus_target(
        &self,
        root: NodeId,
        current_element: Option<ElementStateId>,
    ) -> Option<FocusTarget> {
        let last = self.last_focus_target(root)?;

        let Some(current_element) = current_element else {
            return Some(last);
        };

        let mut previous = None;
        let mut current = Some(root);

        while let Some(node_id) = current {
            if let Some(target) = self.focus_target_from_node(node_id) {
                if target.element == current_element {
                    return previous.or(Some(last));
                }

                previous = Some(target);
            }

            current = self.next_depth_first_node(node_id);
        }

        Some(last)
    }

    pub(crate) fn focused_style_invalidation(&self, element: ElementStateId) -> Invalidation {
        for node in self.nodes.iter() {
            if node.element_state_id == Some(element) {
                return node.interaction.focused_style.invalidation();
            }
        }

        Invalidation::None
    }

    pub(crate) fn pressed_style_invalidation(&self, element: ElementStateId) -> Invalidation {
        for node in self.nodes.iter() {
            if node.element_state_id == Some(element) {
                return node.interaction.pressed_style.invalidation();
            }
        }

        Invalidation::None
    }

    pub(crate) fn hit_test_scroll(&self, root: NodeId, position: Point) -> Option<ScrollTarget> {
        let mut hit = None;

        for visual in self.visual_nodes(root) {
            if !visual.contains(position) {
                continue;
            }

            let node_id = visual.node();
            let node = self.node(node_id);
            let axes = node.interaction.scroll_axes;
            if !axes.any() {
                continue;
            }
            let Some(element) = node.element_state_id else {
                continue;
            };

            let viewport = self.scroll_viewport_bounds(node_id, visual.bounds());
            let damage = match visual.clip() {
                Some(clip) => DamageRegion::from_rect(viewport).clipped_to(clip),
                None => DamageRegion::from_rect(viewport),
            };

            hit = Some(ScrollTarget {
                node: node_id,
                element,
                axes,
                max_offset: self.max_scroll_offset(node_id),
                damage,
            });
        }

        hit
    }

    pub(crate) fn set_scroll_offset(&mut self, node: NodeId, offset: Offset) {
        self.node_mut(node).interaction.scroll_offset = offset;
    }

    pub(crate) fn clamp_scroll_offset<const SLOTS: usize>(
        &mut self,
        states: &mut ScrollStateTable<SLOTS>,
    ) {
        for index in 0..self.nodes.len() {
            let node_id = NodeId::new(index as u16);
            let node = self.node(node_id);
            let axes = node.interaction.scroll_axes;
            if !axes.any() {
                continue;
            }
            let Some(element) = node.element_state_id else {
                continue;
            };

            let maximum = self.max_scroll_offset(node_id);
            let current = states.offset(element);
            let next = Offset::new(
                if axes.horizontal() {
                    current.x.clamp(px(0), maximum.x)
                } else {
                    px(0)
                },
                if axes.vertical() {
                    current.y.clamp(px(0), maximum.y)
                } else {
                    px(0)
                },
            );

            states.set_offset(element, next);

            self.node_mut(node_id).interaction.scroll_offset = next;
        }
    }

    pub(crate) fn node_for_element(&self, root: NodeId, element: ElementStateId) -> Option<NodeId> {
        let mut current = Some(root);

        while let Some(node_id) = current {
            if self.node(node_id).element_state_id == Some(element) {
                return Some(node_id);
            }

            current = self.next_depth_first_node(node_id);
        }

        None
    }

    fn next_after_subtree(&self, node: NodeId) -> Option<NodeId> {
        let mut current = node;

        loop {
            if let Some(sibling) = self.node(current).next_sibling {
                return Some(sibling);
            }

            current = self.node(current).parent?;
        }
    }

    pub(crate) fn visual_damage_for_element(
        &self,
        root: NodeId,
        first: Option<ElementStateId>,
        second: Option<ElementStateId>,
    ) -> Option<DamageRegion> {
        if first.is_none() && second.is_none() {
            return Some(DamageRegion::none());
        }

        let first_node = match first {
            Some(element) => Some(self.node_for_element(root, element)?),
            None => None,
        };
        let second_node = match second {
            Some(element) => Some(self.node_for_element(root, element)?),
            None => None,
        };

        let first_end = first_node.and_then(|node| self.next_after_subtree(node));
        let second_end = second_node.and_then(|node| self.next_after_subtree(node));

        let mut first_active = false;
        let mut second_active = false;
        let mut damage = DamageRegion::none();

        for visual in self.visual_nodes(root) {
            let node = visual.node();

            // end markers point to the first node outside the corresponding subtree,
            // so deactivate before considering that node
            if first_active && first_end == Some(node) {
                first_active = false;
            }
            if second_active && second_end == Some(node) {
                second_active = false;
            }

            if first_node == Some(node) {
                first_active = true;
            }
            if second_node == Some(node) {
                second_active = true;
            }

            if !first_active && !second_active {
                continue;
            }
            if !visual.is_visible() {
                continue;
            }

            // entity nodes do not issue paint commands, their painted descendants
            // are still visited
            if matches!(self.node(node).kind, crate::NodeKind::Entity { .. }) {
                continue;
            }

            let mut bounds = visual.bounds();
            if let Some(clip) = visual.clip() {
                let Some(clipped) = bounds.intersection(clip) else {
                    continue;
                };
                bounds = clipped;
            }

            damage = damage.add_rect(bounds);
        }

        Some(damage)
    }

    fn scroll_viewport_bounds(&self, node: NodeId, visual_bounds: Rect) -> Rect {
        let border = self
            .node(node)
            .style()
            .map(|s| s.border_width.non_negative())
            .unwrap_or_default();

        visual_bounds.inset(border)
    }

    pub(crate) fn scroll_element_into_view<const SLOTS: usize>(
        &mut self,
        root: NodeId,
        element: ElementStateId,
        states: &mut ScrollStateTable<SLOTS>,
    ) -> DamageRegion {
        let Some(target_node) = self.node_for_element(root, element) else {
            return DamageRegion::none();
        };

        // `target_bounds` and `damage` start in the unscrolled layout coordinate system.
        // as we walk ancestors inside-out, we progressively apply their final scroll
        // translations. By the time we reach the root, damage is in visual/screen
        // coordinates
        let mut target_bounds = self.node(target_node).layout.bounds;
        let mut damage = DamageRegion::none();
        let mut current = self.node(target_node).parent;

        while let Some(ancestor) = current {
            count_metric!(self, scroll_into_view_ancestor_visits);
            let next_parent = self.node(ancestor).parent;
            let (axes, scroll_element, applied_scroll) = {
                let node = self.node(ancestor);
                (
                    node.interaction.scroll_axes,
                    node.element_state_id,
                    node.interaction.scroll_offset,
                )
            };

            // first express the target using the currently applied scroll offset.
            // if the offset changes below, `target_bounds` is adjusted again
            target_bounds = target_bounds.translated(Offset::ZERO - applied_scroll);

            let viewport = self.scroll_viewport_bounds(ancestor, self.node(ancestor).layout.bounds);
            let mut final_scroll = applied_scroll;
            let mut scroll_changed = false;

            if axes.any()
                && let Some(scroll_element) = scroll_element
            {
                let previous = states.offset(scroll_element);
                debug_assert_eq!(
                    previous, applied_scroll,
                    "persistent and frame scroll offsets must remain synchronized"
                );

                let maximum = self.max_scroll_offset(ancestor);
                let mut next = previous;

                if axes.horizontal() {
                    next.x = scroll_axis_into_view(
                        previous.x,
                        maximum.x,
                        target_bounds.x(),
                        target_bounds.right(),
                        viewport.x(),
                        viewport.right(),
                    );
                }
                if axes.vertical() {
                    next.y = scroll_axis_into_view(
                        previous.y,
                        maximum.y,
                        target_bounds.y(),
                        target_bounds.bottom(),
                        viewport.y(),
                        viewport.bottom(),
                    );
                }

                if next != previous {
                    // target bounds already contains `-previous`. convert it to `-next`
                    target_bounds = target_bounds.translated(previous - next);
                    states.set_offset(scroll_element, next);
                    self.set_scroll_offset(ancestor, next);
                    final_scroll = next;
                    scroll_changed = true;
                }
            }

            // everything already damaged belongs to descendants of this ancestor, so
            // its FINAL scroll offset moves all those rectangles
            damage = damage.translated(Offset::ZERO - final_scroll);

            // apply the same clipping rule used by visual traversal. This keeps nested
            // scroll damage restricted to the part actually visible through this ancestor
            if self.node_clips_children(ancestor) {
                damage = damage.clipped_to(viewport);
            }

            // the viewport itself is unaffected by its own scroll translation. If this
            // ancestor scrolled, every visible pixel inside its viewport may contain
            // different content
            if scroll_changed {
                damage = damage.add_rect(viewport);
            }

            current = next_parent;
        }

        damage
    }
}
