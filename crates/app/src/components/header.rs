use inkpaper_ui::prelude::*;

#[component]
pub(crate) struct HomeHeader<'a> {
    battery: &'a str,
}

impl RenderOnce for HomeHeader<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full h-full relative">
                <div class="absolute top-[5px] right-[18px] flex items-center gap-[4px]">
                    <text class="text-[10px] leading-[12px]">{self.battery}</text>
                    <BatteryIcon />
                </div>
            </div>
        }
    }
}

#[component]
struct BatteryIcon;

impl BatteryIcon {
    fn draw(paint: &mut PaintCx<'_>) {
        paint.draw_shapes(paint.bounds(), |_, painter| {
            let black = Color::BLACK;

            painter.line(
                Point::new(px(1), px(0)),
                Point::new(px(12), px(0)),
                px(1),
                black,
            );
            painter.line(
                Point::new(px(1), px(11)),
                Point::new(px(12), px(11)),
                px(1),
                black,
            );
            painter.line(
                Point::new(px(0), px(1)),
                Point::new(px(0), px(10)),
                px(1),
                black,
            );
            painter.line(
                Point::new(px(13), px(1)),
                Point::new(px(13), px(10)),
                px(1),
                black,
            );

            painter.line(
                Point::new(px(14), px(3)),
                Point::new(px(14), px(8)),
                px(1),
                black,
            );
            painter.line(
                Point::new(px(15), px(4)),
                Point::new(px(15), px(7)),
                px(1),
                black,
            );

            painter.fill_rect(
                Rect::new(Point::new(px(2), px(2)), Size::new(px(3), px(8))),
                black,
            );
            painter.fill_rect(
                Rect::new(Point::new(px(6), px(2)), Size::new(px(3), px(8))),
                black,
            );
            painter.fill_rect(
                Rect::new(Point::new(px(10), px(2)), Size::new(px(3), px(8))),
                black,
            );
        });
    }
}

impl RenderOnce for BatteryIcon {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        canvas(Self::draw).size(Size::new(px(16), px(12)))
    }
}
