use inkpaper_ui::prelude::*;

use crate::{
    AppModel,
    components::{BottomNav, TopBar},
    screens::{HomeScreen, LibraryScreen, PlaceholderScreen},
    theme::Theme,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Route {
    Home,
    Library,
    Settings,
}

pub struct InkPaperApp {
    route: Route,
    model: AppModel,
    current_cover: Option<ImageSource>,
}

impl InkPaperApp {
    pub const fn new(model: AppModel) -> Self {
        Self {
            route: Route::Home,
            model,
            current_cover: None,
        }
    }

    pub const fn with_current_cover(mut self, cover: ImageSource) -> Self {
        self.current_cover = Some(cover);
        self
    }

    pub const fn route(&self) -> Route {
        self.route
    }

    pub const fn model(&self) -> &AppModel {
        &self.model
    }

    pub fn model_mut(&mut self) -> &mut AppModel {
        &mut self.model
    }

    pub const fn current_cover(&self) -> Option<ImageSource> {
        self.current_cover
    }

    fn open_home(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.navigate(Route::Home, cx);
    }

    fn open_library(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.navigate(Route::Library, cx);
    }

    fn open_settings(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.navigate(Route::Settings, cx);
    }

    pub fn navigate(&mut self, route: Route, cx: &mut Context<Self>) {
        if self.route == route {
            return;
        }

        self.route = route;
        cx.notify();
    }
}

impl Render for InkPaperApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let theme = cx.global::<Theme>();

        let open_home = cx.listener(Self::open_home);
        let open_library = cx.listener(Self::open_library);
        let open_settings = cx.listener(Self::open_settings);

        let route = self.route;
        let model = &self.model;
        let current_cover = self.current_cover;

        let screen = match route {
            Route::Home => Either::Left(HomeScreen::new(model, current_cover)),
            Route::Library => Either::Right(Either::Left(LibraryScreen::new(model.library()))),
            Route::Settings => Either::Right(Either::Right(PlaceholderScreen::new(
                "Settings",
                "Device and reader settings will live here.",
            ))),
        };

        div()
            .w_full()
            .h_full()
            .flex_col()
            .bg(theme.paper)
            .text_color(theme.ink)
            .child(TopBar::new(model.clock().label(), model.battery().label()))
            .child(
                div()
                    .id("screen")
                    .w_full()
                    .flex_1()
                    .overflow_y_scroll()
                    .child(screen),
            )
            .child(BottomNav::new(
                route,
                open_home,
                open_library,
                open_settings,
            ))
    }
}
