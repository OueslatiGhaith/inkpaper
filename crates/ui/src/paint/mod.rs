use crate::{
    Color, FrameArena, NodeId, NodeKind, Pixels, Rect, ResolvedTextStyle, TextMeasurer,
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
}

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
    fn paint_visual_node<P>(&self, visual: VisualNode, painter: &mut P) -> Result<(), P::Error>
    where
        P: Painter,
    {
        if !visual.is_visible() {
            return Ok(());
        }

        let node_id = visual.node();
        let node = self.node(node_id);
        let bounds = visual.bounds();
        let clip = visual.clip();

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
            NodeKind::Image { .. } | NodeKind::Entity { .. } => Ok(()),
        }
    }

    pub fn paint<P>(&self, root: NodeId, painter: &mut P) -> Result<(), P::Error>
    where
        P: Painter,
    {
        for visual in self.visual_nodes(root) {
            self.paint_visual_node(visual, painter)?;
        }

        Ok(())
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
}
