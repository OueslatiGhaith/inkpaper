use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus, InkPaperApp,
    app::{Exit, ScreenInput, ScreenLifecycle, ScreenView, SideButton},
    components::{
        header::battery_icon,
        icon::{Icon, IconKind, IconProps},
        reader_menu::{self, ReaderMenuView},
    },
    reader::{font_size_from_slider, reader_page_bounds},
};

#[component]
pub(crate) struct ReaderScreen<'a> {
    title: &'a str,
    creator: &'a str,
    status: &'a str,
    detail: &'a str,
    path: &'a str,

    page_canvas: Option<Canvas>,

    page_label: &'a str,
    progress_label: &'a str,
    battery: Entity<ReaderBatteryIcon>,

    menu_open: bool,
    menu: Entity<ReaderMenuView>,

    on_previous_page: Listener<ActivateEvent>,
    on_open_menu: Listener<ActivateEvent>,
    on_next_page: Listener<ActivateEvent>,
    on_close_menu: Listener<ActivateEvent>,
}

impl RenderOnce for ReaderScreen<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let page = reader_page_bounds();

        let page_left = px(page.left as i32);
        let page_top = px(page.top as i32);
        let page_size = Size::new(px(page.width as i32), px(page.height as i32));

        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                {#if self.page_canvas.is_some()}
                    <div class="absolute left-{page_left} top-{page_top} w-{page_size.width} h-{page_size.height} overflow-hidden">
                        {
                            self.page_canvas
                                .expect("reader page canvas was checked above")
                                .size(page_size)
                        }
                    </div>

                    // screen thirds above the status bar, whatever the margin.
                    // Taps pass through elements without listeners, so the
                    // zones are removed while the drawer covers them
                    {#if !self.menu_open}
                        <div
                            id="reader-previous-page"
                            on:activate={self.on_previous_page}
                            class="absolute left-0 top-0 w-[160px] h-[720px]"
                        />

                        <div
                            id="reader-open-menu"
                            on:activate={self.on_open_menu}
                            class="absolute left-[160px] top-0 w-[160px] h-[720px]"
                        />

                        <div
                            id="reader-next-page"
                            on:activate={self.on_next_page}
                            class="absolute left-[320px] top-0 w-[160px] h-[720px]"
                        />
                    {/if}

                    <ReaderStatusBar
                        page_label={self.page_label}
                        progress_label={self.progress_label}
                        battery={self.battery}
                    />

                    {#if self.menu_open}
                        // taps above the drawer close it. The drawer is its own entity
                        // and only covers its sheet, so it repaints only that
                        <div
                            id="reader-menu-dismiss"
                            on:activate={self.on_close_menu}
                            class="absolute left-0 top-0 w-full h-[371px]"
                        />

                        {self.menu}
                    {/if}
                {:else}
                    <div class="absolute left-5 top-[220px] w-[440px] flex flex-col items-center gap-2.5">
                        <text class="font-bold text-2xl text-center no-wrap max-lines-1 text-ellipsis">
                            {self.title}
                        </text>

                        <text class="text-xl text-center no-wrap max-lines-1 text-ellipsis">
                            {self.creator}
                        </text>

                        <text class="text-xl text-center">
                            {self.status}
                        </text>

                        <div class="mt-2.5 w-[420px]">
                            <text class="text-base text-center no-wrap max-lines-1 text-ellipsis">
                                {self.detail}
                            </text>
                        </div>

                        <div class="w-[420px]">
                            <text class="text-base text-center no-wrap max-lines-1 text-ellipsis">
                                {self.path}
                            </text>
                        </div>
                    </div>
                {/if}
            </div>
        }
    }
}

/// Status bar: page on the left, book progress and battery on the right.
#[component]
struct ReaderStatusBar<'a> {
    page_label: &'a str,
    progress_label: &'a str,
    battery: Entity<ReaderBatteryIcon>,
}

impl RenderOnce for ReaderStatusBar<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="absolute left-5 top-[735px] w-[440px] h-[45px] bg-white">
                <div class="absolute left-0 top-2 h-7 flex items-center">
                    <text class="text-base no-wrap">
                        {self.page_label}
                    </text>
                </div>

                <div class="absolute right-0 top-2 h-7 flex items-center gap-2">
                    <text class="text-base no-wrap">
                        {self.progress_label}
                    </text>

                    {self.battery}
                </div>
            </div>
        }
    }
}

/// the battery icon of the reader's status bar. It keeps only the icon, so a new
/// percentage paints nothing while reading unless the icon changes
#[derive(Default)]
pub(crate) struct ReaderBatteryIcon {
    icon: Option<IconKind>,
}

impl ReaderBatteryIcon {
    pub(crate) fn set_battery(&mut self, battery: BatteryStatus, cx: &mut Context<'_, Self>) {
        let icon = Some(battery_icon(battery));

        if self.icon != icon {
            self.icon = icon;
            cx.notify();
        }
    }
}

impl Render for ReaderBatteryIcon {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let icon = self.icon;

        rsx! {
            <div class="flex items-center">
                {#if let Some(icon) = icon}
                    <Icon kind={icon} size={px(24)} />
                {/if}
            </div>
        }
    }
}

// Gesture thresholds
const SWIPE_DISTANCE: i32 = 40;
const BACK_EDGE_WIDTH: i32 = 30;

pub(crate) struct ReaderRoute;

impl ScreenLifecycle for ReaderRoute {
    fn exit(&self, app: &mut InkPaperApp, exit: Exit) {
        if exit == Exit::Closed {
            app.reader.close_menu();
        }
    }
}

impl ScreenInput for ReaderRoute {
    fn side_button(
        &self,
        app: &mut InkPaperApp,
        button: SideButton,
        cx: &mut Context<'_, InkPaperApp>,
    ) {
        // the drawer covers the page, so it does not turn underneath it
        if app.reader.menu_open() {
            return;
        }

        match button {
            SideButton::Previous => app.reader_previous_page(cx),
            SideButton::Next => app.reader_next_page(cx),
        }
    }

    fn drag(
        &self,
        app: &mut InkPaperApp,
        origin: Point,
        position: Point,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> bool {
        // dragging along the font slider previews a size; it applies on release
        if app.reader.font_slider_shown() && reader_menu::font_slider_contains(origin) {
            let value = reader_menu::font_slider_value_at(position.x.get());

            // the preview shows only in the drawer
            if app.reader.preview_font_size(font_size_from_slider(value)) {
                app.refresh_reader_menu(cx);
            }

            return true;
        }

        let dx = position.x.get() - origin.x.get();
        let dy = position.y.get() - origin.y.get();

        // a swipe right from the left edge goes back
        if origin.x.get() < BACK_EDGE_WIDTH && dx >= SWIPE_DISTANCE && dx.abs() > dy.abs() {
            app.navigate_back(cx);
            return true;
        }

        // vertical swipes open (up) and close (down) the drawer
        if dy.abs() < SWIPE_DISTANCE || dy.abs() <= dx.abs() {
            return false;
        }

        if dy < 0 {
            app.open_reader_menu(cx);
        } else {
            app.close_reader_menu(cx);
        }

        true
    }

    fn release(
        &self,
        app: &mut InkPaperApp,
        position: Point,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> bool {
        if !app.reader.menu_open() {
            return false;
        }

        // repaginating is too slow to follow a drag, so the size applies here
        if app.reader.commit_font_preview() {
            cx.notify();
            return true;
        }

        // a tap on the track jumps to that size
        if app.reader.font_slider_shown() && reader_menu::font_slider_contains(position) {
            let value = reader_menu::font_slider_value_at(position.x.get());

            if app.reader.set_font_size(font_size_from_slider(value)) {
                cx.notify();
            }

            return true;
        }

        false
    }
}

impl ScreenView for ReaderRoute {
    fn render<'a>(
        &self,
        app: &'a InkPaperApp,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> AnyElement<'a> {
        let page_canvas = app
            .reader
            .page()
            .is_some()
            .then(|| cx.canvas(|app: &InkPaperApp, paint| app.paint_reader_page(paint)));

        ReaderScreen::from(ReaderScreenProps {
            title: app.reader.title(),
            creator: app.reader.creator(),
            status: app.reader.status(),
            detail: app.reader.detail(),
            path: app.reader.path(),
            page_canvas,
            page_label: app.reader.page_label(),
            progress_label: app.reader.progress_label(),
            battery: app.reader_battery_icon,
            menu_open: app.reader.menu_open(),
            menu: app.reader_menu,
            on_close_menu: cx.listener(InkPaperApp::activate_close_reader_menu),
            on_previous_page: cx.listener(InkPaperApp::activate_previous_reader_page),
            on_open_menu: cx.listener(InkPaperApp::activate_open_reader_menu),
            on_next_page: cx.listener(InkPaperApp::activate_next_reader_page),
        })
        .into_any_element()
    }
}
