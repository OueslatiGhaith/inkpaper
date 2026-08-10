use crate::{Color, FrameArena, NodeId, NodeKind, Pixels, Point, Rect, Size, TextMeasurer, px};

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

#[derive(Debug, Clone, Copy)]
enum ClipRegion {
    Unbounded,
    Rect(Rect),
    Empty,
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
        origin: Point,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error>;
}

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
    fn paint_node<P>(&self, node_id: NodeId, painter: &mut P) -> Result<(), P::Error>
    where
        P: Painter,
    {
        let clip = match self.clip_for_node(node_id) {
            ClipRegion::Unbounded => None,
            ClipRegion::Rect(rect) => Some(rect),
            ClipRegion::Empty => return Ok(()),
        };

        let node = self.node(node_id);
        let bounds = self.visual_bounds(node_id);

        match node.kind {
            NodeKind::Div { .. } => {
                let style = node.style().expect("div node must have style");
                let border = match (style.border_width.0 > 0, style.border_color) {
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
            NodeKind::Text { text } => painter.draw_text(self.text(text), bounds.origin, clip),
            NodeKind::Entity { .. } => Ok(()),
        }
    }

    pub fn paint<P>(&self, root: NodeId, painter: &mut P) -> Result<(), P::Error>
    where
        P: Painter,
    {
        let mut current = Some(root);
        while let Some(node) = current {
            self.paint_node(node, painter)?;
            current = self.next_depth_first_node(node);
        }

        Ok(())
    }

    fn node_clips_children(&self, node: NodeId) -> bool {
        let node = self.node(node);

        let style_clips = match node.kind {
            NodeKind::Div { .. } => node
                .style()
                .map(|style| style.clip_children)
                .unwrap_or(false),
            _ => false,
        };

        style_clips || node.interaction.scroll_axes.any()
    }

    pub(crate) fn visual_bounds(&self, node: NodeId) -> Rect {
        self.node(node)
            .layout
            .bounds
            .translated(self.node_visual_offset(node))
    }

    fn clip_for_node(&self, node: NodeId) -> ClipRegion {
        let mut clip = ClipRegion::Unbounded;
        let mut current = self.node(node).parent;

        while let Some(parent) = current {
            if self.node_clips_children(parent) {
                let parent_bounds = self.visual_bounds(parent);
                clip = match clip {
                    ClipRegion::Unbounded => ClipRegion::Rect(parent_bounds),
                    ClipRegion::Rect(existing) => match existing.intersection(parent_bounds) {
                        Some(rect) => ClipRegion::Rect(rect),
                        None => return ClipRegion::Empty,
                    },
                    ClipRegion::Empty => return ClipRegion::Empty,
                };
            }
            current = self.node(parent).parent;
        }

        clip
    }

    pub(crate) fn point_visible_for_node(&self, node: NodeId, position: Point) -> bool {
        match self.clip_for_node(node) {
            ClipRegion::Unbounded => true,
            ClipRegion::Rect(clip) => clip.contains(position),
            ClipRegion::Empty => false,
        }
    }

    fn node_visual_offset(&self, node: NodeId) -> Point {
        let mut x = 0i32;
        let mut y = 0i32;
        let mut current = self.node(node).parent;

        while let Some(parent) = current {
            let scroll = self.node(parent).interaction.scroll_offset;
            x = x.saturating_sub(scroll.x.0);
            y = y.saturating_sub(scroll.y.0);
            current = self.node(parent).parent;
        }

        Point::new(px(x), px(y))
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
                origin: Point::new(px(0), px(10),),
                length: 2,
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
}
