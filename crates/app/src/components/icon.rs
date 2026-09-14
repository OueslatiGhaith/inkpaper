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
        let source = match self.kind {
            IconKind::Folder => include_svg!("assets/icons/folder.svg"),
            IconKind::Recent => include_svg!("assets/icons/history.svg"),
            IconKind::Transfer => include_svg!("assets/icons/send-horizontal.svg"),
            IconKind::Settings => include_svg!("assets/icons/settings-2.svg"),
            IconKind::BookOpen => include_svg!("assets/icons/book-open.svg"),
        };

        svg(source)
            .size(Size::new(px(32), px(32)))
            .text_color(Color::BLACK)
    }
}
