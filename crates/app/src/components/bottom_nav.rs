use inkpaper_ui::prelude::*;

use crate::{Route, theme::Theme};

pub struct BottomNav {
    current: Route,
    home: Listener<ActivateEvent>,
    library: Listener<ActivateEvent>,
    settings: Listener<ActivateEvent>,
}

impl BottomNav {
    pub const fn new(
        current: Route,
        home: Listener<ActivateEvent>,
        library: Listener<ActivateEvent>,
        settings: Listener<ActivateEvent>,
    ) -> Self {
        Self {
            current,
            home,
            library,
            settings,
        }
    }
}

impl RenderOnce for BottomNav {
    fn render(self, cx: &AppContext<'_>) -> impl IntoElement {
        let theme = cx.global::<Theme>();

        div()
            .w_full()
            .h(px(68))
            .px(px(16))
            .gap(px(8))
            .flex()
            .items_center()
            .bg(theme.paper)
            .border(px(1))
            .border_color(theme.subtle)
            .child(nav_item(
                "nav-home",
                "Home",
                self.current == Route::Home,
                &theme,
                self.home,
            ))
            .child(nav_item(
                "nav-library",
                "Library",
                self.current == Route::Library,
                &theme,
                self.library,
            ))
            .child(nav_item(
                "nav-settings",
                "Settings",
                self.current == Route::Settings,
                &theme,
                self.settings,
            ))
    }
}

fn nav_item(
    id: &'static str,
    label: &'static str,
    active: bool,
    theme: &Theme,
    listener: Listener<ActivateEvent>,
) -> impl IntoElement + use<> {
    let background = if active { theme.ink } else { theme.paper };
    let foreground = if active { theme.paper } else { theme.ink };

    div()
        .id(id)
        .flex_1()
        .h(px(44))
        .flex()
        .items_center()
        .justify_center()
        .bg(background)
        .text_color(foreground)
        .border(px(1))
        .border_color(theme.ink)
        .rounded(px(5))
        .when_focused(|style| style.border(px(3)))
        .on_activate(listener)
        .child(label)
}
