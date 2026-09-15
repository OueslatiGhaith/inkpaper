use inkpaper_ui::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IconKind {
    Folder,
    Recent,
    Transfer,
    Settings,
    Book,
    BookOpen,
    BookMarked,
    ChevronLeft,
    SlidersHorizontal,
    BatteryCharging,
    BatteryFull,
    BatteryMedium,
    BatteryLow,
    BatteryWarning,
    Wifi,
    Library,
    RadioTower,
    Usb,
    NearbyTransfer,
    ReadingStats,
}
#[component]
pub(crate) struct Icon {
    kind: IconKind,
    size: Pixels,
}

impl RenderOnce for Icon {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let source = match self.kind {
            IconKind::Folder => include_svg!("assets/icons/lucide/folder.svg"),
            IconKind::Recent => include_svg!("assets/icons/lucide/history.svg"),
            IconKind::Transfer => include_svg!("assets/icons/lucide/send-horizontal.svg"),
            IconKind::Settings => include_svg!("assets/icons/lucide/settings-2.svg"),
            IconKind::Book => include_svg!("assets/icons/lucide/book.svg"),
            IconKind::BookOpen => include_svg!("assets/icons/lucide/book-open.svg"),
            IconKind::BookMarked => include_svg!("assets/icons/lucide/book-bookmark.svg"),
            IconKind::ChevronLeft => include_svg!("assets/icons/lucide/chevron-left.svg"),
            IconKind::SlidersHorizontal => {
                include_svg!("assets/icons/lucide/sliders-horizontal.svg")
            }
            IconKind::BatteryCharging => {
                include_svg!("assets/icons/lucide/battery-charging.svg")
            }
            IconKind::BatteryFull => include_svg!("assets/icons/lucide/battery-full.svg"),
            IconKind::BatteryMedium => include_svg!("assets/icons/lucide/battery-medium.svg"),
            IconKind::BatteryLow => include_svg!("assets/icons/lucide/battery-low.svg"),
            IconKind::BatteryWarning => {
                include_svg!("assets/icons/lucide/battery-warning.svg")
            }
            IconKind::Wifi => include_svg!("assets/icons/lucide/wifi.svg"),
            IconKind::Library => include_svg!("assets/icons/lucide/library-big.svg"),
            IconKind::RadioTower => include_svg!("assets/icons/lucide/radio-tower.svg"),
            IconKind::Usb => include_svg!("assets/icons/lucide/usb.svg"),
            IconKind::NearbyTransfer => {
                include_svg!("assets/icons/lucide/chevrons-left-right-ellipsis.svg")
            }
            IconKind::ReadingStats => {
                include_svg!("assets/icons/lucide/chart-no-axes-column-increasing.svg")
            }
        };

        svg(source)
            .size(Size::new(self.size, self.size))
            .text_color(Color::BLACK)
    }
}
