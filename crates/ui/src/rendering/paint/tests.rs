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
        paint: ImagePaint,
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

impl ResourcePainter for RecordingPainter {
    fn draw_text(
        &mut self,
        _: &mut (),
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
        _: &mut (),
        source: ImageSource,
        bounds: Rect,
        paint: ImagePaint,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error> {
        self.commands.push(Command::Image {
            source,
            bounds,
            paint,
            clip,
        });

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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .w(px(80))
                .h(px(40))
                .bg(Color::RED)
                .child(div().w(px(20)).h(px(10)).bg(Color::BLUE))
                .child("Hi"),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .w(px(40))
                .h(px(20))
                .bg(Color::BLUE)
                .border(px(2))
                .border_color(Color::RED)
                .rounded(px(6)),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .w(px(50))
                .h(px(30))
                .overflow_hidden()
                .child(div().w(px(100)).h(px(20)).bg(Color::RED)),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .w(px(50))
                .h(px(30))
                .border(px(2))
                .border_color(Color::WHITE)
                .overflow_hidden()
                .child(div().w(px(100)).h(px(100)).bg(Color::RED)),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .font(FontId::new(1))
                .text_color(Color::WHITE)
                .line_height(px(16))
                .child(text("Hello").text_color(Color::RED)),
            cx,
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

    let globals = GlobalArena::<0, 0>::default();

    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div().w(px(80)).h(px(40)).child(
                image(source)
                    .cover()
                    .position(ImagePosition::Bottom)
                    .sampling(ImageSampling::Bilinear)
                    .monochrome()
                    .brightness(8)
                    .contrast(125)
                    .invert()
                    .dither(ImageDither::Bayer2x2),
            ),
            cx,
        )
        .unwrap();

    let mut painter = RecordingPainter::default();

    frame.layout(root, Size::new(px(80), px(40)), &painter);
    frame.paint(root, &mut painter).unwrap();

    assert_eq!(painter.commands.len(), 2);

    assert_eq!(
        painter.commands[1],
        Command::Image {
            source,
            bounds: Rect::new(Point::new(px(0), px(0)), Size::new(px(20), px(12)),),
            paint: ImagePaint {
                fit: ImageFit::Cover,
                position: ImagePosition::Bottom,
                sampling: ImageSampling::Bilinear,
                color_mode: ImageColorMode::Monochrome,
                brightness: 8,
                contrast: 125,
                invert: true,
                dither: ImageDither::Bayer2x2,
            },
            clip: None,
        }
    );
}

#[test]
fn canvas_callback_draws_in_local_coordinates() {
    let mut frame = FrameArena::<8, 128>::default();
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .p(px(10))
                .child(canvas(draw_test_canvas).size(Size::new(px(30), px(20)))),
            cx,
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

    runtime.layout_with_measurer(Size::new(px(100), px(20)), &painter);
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

    runtime.layout_with_measurer(Size::new(px(100), px(20)), &painter);
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .w(px(100))
                .h(px(60))
                .bg(Color::BLACK)
                .child(div().w(px(100)).h(px(20)).bg(Color::RED))
                .child(div().w(px(100)).h(px(20)).bg(Color::BLUE))
                .child(div().w(px(100)).h(px(20)).bg(Color::GREEN)),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(div().w(px(100)).h(px(40)).bg(Color::BLUE), cx)
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .w(px(40))
                .h(px(20))
                .overflow_hidden()
                .child(div().w(px(80)).h(px(20)).bg(Color::RED)),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(div().w(px(100)).h(px(40)).bg(Color::BLUE), cx)
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
        .layout_with_measurer(Size::new(px(100), px(100)), &painter)
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let first_root = frame
        .mount(
            div()
                .w(px(20))
                .h(px(20))
                .child(div().w(px(20)).h(px(20)).child("old")),
            cx,
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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .w(px(100))
                .h(px(160))
                .children((0..16).map(|_| div().w(px(100)).h(px(10)).child("row"))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
