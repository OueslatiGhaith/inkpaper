use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind, IconProps};

#[component]
pub(crate) struct HomeHeader<'a> {
    battery: &'a str,
}

impl RenderOnce for HomeHeader<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full h-full relative">
                <div class="absolute top-[5px] right-[18px] flex items-center gap-1">
                    <text class="text-[10px] leading-3">{self.battery}</text>
                    <BatteryIcon />
                </div>
            </div>
        }
    }
}

#[component]
pub(crate) struct FileBrowserHeader<'a> {
    title: &'a str,
    battery: &'a str,
}

impl RenderOnce for FileBrowserHeader<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full h-full relative">
                <BatteryStatus battery={self.battery} />

                <div class="absolute left-[10px] top-[22px] w-8 h-8">
                    <Icon kind={IconKind::ChevronLeft} size={px(32)} />
                </div>

                <div class="absolute left-[60px] top-3 h-[52px] flex items-center">
                    <text class="font-bold text-[12px] leading-4 no-wrap max-lines-1 text-ellipsis">
                        {self.title}
                    </text>
                </div>

                <div class="absolute right-[14px] top-[26px] w-6 h-6">
                    <Icon kind={IconKind::SlidersHorizontal} size={px(24)} />
                </div>
            </div>
        }
    }
}

#[component]
struct BatteryStatus<'a> {
    battery: &'a str,
}

impl RenderOnce for BatteryStatus<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="absolute top-0 right-[18px] flex items-center gap-1">
                <text class="text-[10px] leading-3">{self.battery}</text>
                <BatteryIcon />
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
