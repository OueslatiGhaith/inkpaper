    use core::convert::Infallible;
    use std::vec::Vec;

    use super::*;
    use crate::*;

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
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);

        let root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(100))
                    .child(div().w(px(80)).h(px(80)).child(div().w(px(40)).h(px(40)))),
                cx,
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
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);

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
                cx,
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
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);

        let root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(40))
                    .border(px(2))
                    .overflow_hidden()
                    .child(div().w(px(100)).h(px(100))),
                cx,
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
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);

        let root = frame
            .mount(
                div()
                    .w(px(50))
                    .h(px(30))
                    .border(px(1))
                    .overflow_hidden()
                    .child(div().w(px(50)).h(px(60)).bg(Color::RED)),
                cx,
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
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);

        let root = frame
            .mount(
                div()
                    .w(px(50))
                    .h(px(30))
                    .border(px(1))
                    .overflow_hidden()
                    .child(div().w(px(50)).h(px(60))),
                cx,
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
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);

        let root = frame
            .mount(
                div()
                    .flex()
                    .w(px(100))
                    .h(px(40))
                    .child(div().w(px(80)).h(px(20)).flex_shrink(1))
                    .child(div().w(px(80)).h(px(20)).flex_shrink(1)),
                cx,
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
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);

        let root = frame
            .mount(
                DeepBranching {
                    depth: super::VISUAL_CONTEXT_STACK_CAPACITY + EXTRA_DEPTH,
                },
                cx,
            )
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
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);
        let mut cx = MountCx::new(&mut frame, cx);

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

    #[test]
    fn subtree_paint_bounds_include_unclipped_overflow() {
        let mut frame = FrameArena::<8, 64>::default();
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);

        let root = frame
            .mount(
                div()
                    .w(px(20))
                    .h(px(20))
                    .child(div().w(px(40)).h(px(10)).bg(Color::RED)),
                cx,
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &TestTextMeasurer);

        assert_eq!(
            frame.subtree_paint_bounds(root,),
            Some(Rect::new(Point::ZERO, Size::new(px(40), px(20),),),),
        );
    }

    #[test]
    fn clipping_bounds_the_cached_subtree_extent() {
        let mut frame = FrameArena::<8, 64>::default();
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);

        let root = frame
            .mount(
                div()
                    .w(px(20))
                    .h(px(20))
                    .overflow_hidden()
                    .child(div().w(px(80)).h(px(80)).bg(Color::RED)),
                cx,
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &TestTextMeasurer);

        assert_eq!(
            frame.subtree_paint_bounds(root,),
            Some(Rect::new(Point::ZERO, Size::new(px(20), px(20),),),),
        );
    }

    #[test]
    fn visual_traversal_can_skip_children_and_remaining_siblings() {
        let mut frame = FrameArena::<32, 128>::default();
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);

        let root = frame
            .mount(
                div()
                    .child(
                        div()
                            .child(div().child("A"))
                            .child(div().child("B"))
                            .child(div().child("C")),
                    )
                    .child(div().child("after")),
                cx,
            )
            .unwrap();

        let branch = frame.node(root).first_child.unwrap();
        let first = frame.node(branch).first_child.unwrap();
        let after = frame.node(branch).next_sibling.unwrap();
        let after_text = frame.node(after).first_child.unwrap();

        let mut traversal = frame.visual_nodes(root);

        assert_eq!(traversal.next().unwrap().node(), root,);
        assert_eq!(traversal.next().unwrap().node(), branch,);
        assert_eq!(traversal.next().unwrap().node(), first,);

        traversal.skip_children_and_remaining_siblings();

        // first's child plus B and C must all have been skipped, but traversal must
        // resume at branch's sibling.
        assert_eq!(traversal.next().unwrap().node(), after,);
        assert_eq!(traversal.next().unwrap().node(), after_text,);
        assert!(traversal.next().is_none(),);
    }

    #[test]
    fn visual_traversal_can_skip_an_ordered_child_prefix() {
        let mut frame = FrameArena::<16, 64>::default();
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);

        let root = frame
            .mount(
                div()
                    .child(div().child(div()).child(div()).child(div()))
                    .child(div()),
                cx,
            )
            .unwrap();

        let branch = frame.node(root).first_child.unwrap();
        let first = frame.node(branch).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();
        let third = frame.node(second).next_sibling.unwrap();
        let after = frame.node(branch).next_sibling.unwrap();

        let mut traversal = frame.visual_nodes(root);

        assert_eq!(traversal.next().unwrap().node(), root,);
        assert_eq!(traversal.next().unwrap().node(), branch,);

        traversal.skip_children_before(second);

        // first child disappears completely.
        assert_eq!(traversal.next().unwrap().node(), second,);
        assert_eq!(traversal.next().unwrap().node(), third,);
        // parent's following sibling continuation must still work.
        assert_eq!(traversal.next().unwrap().node(), after,);
        assert!(traversal.next().is_none(),);
    }
