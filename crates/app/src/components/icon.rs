use inkpaper_ui::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IconKind {
    Folder,
    Recent,
    Transfer,
    Settings,
    BookOpen,
}

#[component]
pub(crate) struct Icon {
    kind: IconKind,
}

impl RenderOnce for Icon {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let draw = match self.kind {
            IconKind::Folder => draw_folder,
            IconKind::Recent => draw_recent,
            IconKind::Transfer => draw_transfer,
            IconKind::Settings => draw_settings,
            IconKind::BookOpen => draw_book_open,
        };

        canvas(draw).size(Size::new(px(32), px(32)))
    }
}

fn draw_folder(paint: &mut PaintCx<'_>) {
    paint.draw_shapes(paint.bounds(), |_, painter| {
        let black = Color::BLACK;
        let stroke = px(2);

        painter.line(
            Point::new(px(4), px(9)),
            Point::new(px(12), px(9)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(12), px(9)),
            Point::new(px(15), px(12)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(15), px(12)),
            Point::new(px(28), px(12)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(28), px(12)),
            Point::new(px(28), px(25)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(28), px(25)),
            Point::new(px(4), px(25)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(4), px(25)),
            Point::new(px(4), px(9)),
            stroke,
            black,
        );
    });
}

fn draw_recent(paint: &mut PaintCx<'_>) {
    paint.draw_shapes(paint.bounds(), |_, painter| {
        let black = Color::BLACK;
        let stroke = px(2);

        painter.stroke_circle(Point::new(px(17), px(16)), px(10), stroke, black);

        painter.line(
            Point::new(px(17), px(10)),
            Point::new(px(17), px(16)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(17), px(16)),
            Point::new(px(21), px(19)),
            stroke,
            black,
        );

        painter.line(
            Point::new(px(4), px(7)),
            Point::new(px(4), px(13)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(4), px(7)),
            Point::new(px(10), px(7)),
            stroke,
            black,
        );
    });
}

fn draw_transfer(paint: &mut PaintCx<'_>) {
    paint.draw_shapes(paint.bounds(), |_, painter| {
        let black = Color::BLACK;
        let stroke = px(2);

        painter.line(
            Point::new(px(10), px(24)),
            Point::new(px(10), px(8)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(5), px(13)),
            Point::new(px(10), px(8)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(15), px(13)),
            Point::new(px(10), px(8)),
            stroke,
            black,
        );

        painter.line(
            Point::new(px(22), px(8)),
            Point::new(px(22), px(24)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(17), px(19)),
            Point::new(px(22), px(24)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(27), px(19)),
            Point::new(px(22), px(24)),
            stroke,
            black,
        );
    });
}

fn draw_settings(paint: &mut PaintCx<'_>) {
    paint.draw_shapes(paint.bounds(), |_, painter| {
        let black = Color::BLACK;
        let stroke = px(2);

        painter.stroke_circle(Point::new(px(16), px(16)), px(9), stroke, black);
        painter.stroke_circle(Point::new(px(16), px(16)), px(3), stroke, black);

        painter.line(
            Point::new(px(16), px(3)),
            Point::new(px(16), px(7)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(16), px(25)),
            Point::new(px(16), px(29)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(3), px(16)),
            Point::new(px(7), px(16)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(25), px(16)),
            Point::new(px(29), px(16)),
            stroke,
            black,
        );

        painter.line(
            Point::new(px(7), px(7)),
            Point::new(px(10), px(10)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(22), px(22)),
            Point::new(px(25), px(25)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(25), px(7)),
            Point::new(px(22), px(10)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(10), px(22)),
            Point::new(px(7), px(25)),
            stroke,
            black,
        );
    });
}

fn draw_book_open(paint: &mut PaintCx<'_>) {
    paint.draw_shapes(paint.bounds(), |_, painter| {
        let black = Color::BLACK;
        let stroke = px(2);

        painter.line(
            Point::new(px(4), px(8)),
            Point::new(px(10), px(7)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(10), px(7)),
            Point::new(px(16), px(10)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(16), px(10)),
            Point::new(px(22), px(7)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(22), px(7)),
            Point::new(px(28), px(8)),
            stroke,
            black,
        );

        painter.line(
            Point::new(px(4), px(8)),
            Point::new(px(4), px(24)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(4), px(24)),
            Point::new(px(10), px(23)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(10), px(23)),
            Point::new(px(16), px(26)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(16), px(26)),
            Point::new(px(22), px(23)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(22), px(23)),
            Point::new(px(28), px(24)),
            stroke,
            black,
        );
        painter.line(
            Point::new(px(28), px(24)),
            Point::new(px(28), px(8)),
            stroke,
            black,
        );

        painter.line(
            Point::new(px(16), px(10)),
            Point::new(px(16), px(26)),
            stroke,
            black,
        );
    });
}
