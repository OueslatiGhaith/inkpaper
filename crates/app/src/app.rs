use inkpaper_ui::prelude::*;

use crate::{
    AppModel,
    components::{BottomNav, TopBar},
    reader::{ChapterRequest, ReaderSession},
    screens::{HomeScreen, LibraryScreen, PlaceholderScreen, ReaderScreen},
    theme::Theme,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Route {
    Home,
    Library,
    Reader,
    Settings,
}

pub struct InkPaperApp {
    route: Route,
    model: AppModel,
    current_cover: Option<ImageSource>,
    reader: Option<ReaderSession>,
}

impl InkPaperApp {
    pub const fn new(model: AppModel) -> Self {
        Self {
            route: Route::Home,
            model,
            current_cover: None,
            reader: None,
        }
    }

    pub const fn with_current_cover(mut self, cover: ImageSource) -> Self {
        self.current_cover = Some(cover);
        self
    }

    pub fn with_reader(mut self, reader: ReaderSession) -> Self {
        self.reader = Some(reader);
        self.route = Route::Reader;

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

    pub const fn reader(&self) -> Option<&ReaderSession> {
        self.reader.as_ref()
    }

    fn open_home(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.navigate(Route::Home, cx);
    }

    fn open_library(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.navigate(Route::Library, cx);
    }

    fn open_reader(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.navigate(Route::Reader, cx);
    }

    fn open_settings(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.navigate(Route::Settings, cx);
    }

    fn previous_reader_page(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.turn_reader_previous(cx);
    }

    fn next_reader_page(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.turn_reader_next(cx);
    }

    pub(crate) fn turn_reader_previous(&mut self, cx: &mut Context<Self>) -> bool {
        if self.route != Route::Reader {
            return false;
        }

        if let Some(reader) = self.reader.as_mut()
            && reader.previous_page()
        {
            cx.notify();
        }

        true
    }

    pub(crate) fn turn_reader_next(&mut self, cx: &mut Context<Self>) -> bool {
        if self.route != Route::Reader {
            return false;
        }

        if let Some(reader) = self.reader.as_mut()
            && reader.next_page()
        {
            cx.notify();
        }

        true
    }

    pub(crate) fn take_reader_chapter_request(&mut self) -> Option<ChapterRequest> {
        if self.route != Route::Reader {
            return None;
        }

        self.reader.as_mut()?.take_chapter_request()
    }

    /// complete each platform request before processing the next input event.
    ///
    /// pass `None` at the book boundary or when loading failed
    pub fn complete_reader_chapter(
        &mut self,
        request: ChapterRequest,
        replacement: Option<ReaderSession>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(reader) = self.reader.as_mut() else {
            return false;
        };
        if self.route != Route::Reader {
            return false;
        }

        let changed = reader.complete_chapeter_request(request, replacement);
        if changed {
            cx.notify();
        }

        changed
    }

    pub fn navigate(&mut self, route: Route, cx: &mut Context<Self>) {
        if route != Route::Reader
            && let Some(reader) = self.reader.as_mut()
        {
            reader.cancel_chapter_request();
        }
        if route == Route::Reader && self.reader.is_none() {
            return;
        }

        if self.route == route {
            return;
        }

        self.route = route;
        cx.notify();
    }
}

impl Render for InkPaperApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let theme = *cx.global::<Theme>();

        let open_home = cx.listener(Self::open_home);
        let open_library = cx.listener(Self::open_library);
        let open_reader = cx.listener(Self::open_reader);
        let open_settings = cx.listener(Self::open_settings);

        let previous_reader_page = cx.listener(Self::previous_reader_page);
        let next_reader_page = cx.listener(Self::next_reader_page);

        let route = self.route;

        if route == Route::Reader {
            let reader = self
                .reader
                .as_ref()
                .expect("reader route requires an installed reader session");

            return Either::Left(
                div()
                    .w_full()
                    .h_full()
                    .bg(theme.paper)
                    .text_color(theme.ink)
                    .child(ReaderScreen::new(
                        reader,
                        previous_reader_page,
                        next_reader_page,
                    )),
            );
        }

        let model = &self.model;
        let current_cover = self.current_cover;

        let continue_reading = self.reader.as_ref().map(|_| open_reader);

        let screen = match route {
            Route::Home => Either::Left(HomeScreen::new(model, current_cover, continue_reading)),
            Route::Library => Either::Right(Either::Left(LibraryScreen::new(model.library()))),
            Route::Settings => Either::Right(Either::Right(PlaceholderScreen::new(
                "Settings",
                "Device and reader settings will live here.",
            ))),
            Route::Reader => unreachable!("reader route returned above"),
        };

        Either::Right(
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
                )),
        )
    }
}
