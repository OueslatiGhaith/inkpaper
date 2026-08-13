use crate::{
    CanvasDraw, CanvasDrawFn, CanvasPainter, Color, FrameArena, ImageFit, ImageSource, NodeId,
    NodeKind, Pixels, Rect, ResolvedTextStyle, TextMeasurer, entity_store::EntityStore,
    listener_store::ListenerStore, visual::VisualNode,
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
    listeners: &'a dyn ListenerStore,
}

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
    fn paint_visual_node<P>(
        &self,
        visual: VisualNode,
        runtime: Option<PaintRuntime<'_>>,
        painter: &mut P,
    ) -> Result<(), P::Error>
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
            NodeKind::Canvas { draw, .. } => {
                let mut invoke =
                    |local_bounds: Rect, canvas_painter: &mut dyn CanvasPainter| match draw {
                        CanvasDraw::Static(draw) => draw(local_bounds, canvas_painter),
                        CanvasDraw::Entity(callback) => {
                            let Some(runtime) = runtime else {
                                debug_assert!(
                                    false,
                                    "entity canvas painted without callback context"
                                );
                                return;
                            };

                            let result = runtime.listeners.invoke_canvas(
                                callback,
                                local_bounds,
                                canvas_painter,
                                runtime.entities,
                            );

                            debug_assert!(
                                result.is_ok(),
                                "entity canvas callback invocation failed: {result:?}"
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

    pub fn paint<P>(&self, root: NodeId, painter: &mut P) -> Result<(), P::Error>
    where
        P: Painter,
    {
        for visual in self.visual_nodes(root) {
            self.paint_visual_node(visual, None, painter)?;
        }

        Ok(())
    }

    pub(crate) fn paint_with_runtime<P>(
        &self,
        root: NodeId,
        entities: &dyn EntityStore,
        listeners: &dyn ListenerStore,
        painter: &mut P,
    ) -> Result<(), P::Error>
    where
        P: Painter,
    {
        let runtime = PaintRuntime {
            entities,
            listeners,
        };

        for visual in self.visual_nodes(root) {
            self.paint_visual_node(visual, Some(runtime), painter)?;
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
}
