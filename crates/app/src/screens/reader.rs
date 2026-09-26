use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus, InkPaperApp,
    app::{Exit, ScreenInput, ScreenLifecycle, ScreenView, SideButton},
    components::{
        header::battery_icon,
        icon::{Icon, IconProps},
        reader_menu::{self, ReaderMenu, ReaderMenuProps},
    },
    reader::{font_size_from_slider, reader_viewport},
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
    battery: Option<BatteryStatus>,

    menu_open: bool,
    font_size: u16,

    on_previous_page: Listener<ActivateEvent>,
    on_open_menu: Listener<ActivateEvent>,
    on_next_page: Listener<ActivateEvent>,
    on_close_menu: Listener<ActivateEvent>,
    on_decrease_font_size: Listener<ActivateEvent>,
    on_increase_font_size: Listener<ActivateEvent>,
}

impl RenderOnce for ReaderScreen<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let viewport = reader_viewport();

        let page_size = Size::new(
            px(i32::try_from(viewport.width()).unwrap_or(i32::MAX)),
            px(i32::try_from(viewport.height()).unwrap_or(i32::MAX)),
        );

        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                {#if self.page_canvas.is_some()}
                    <div class="absolute left-5 top-[35px] w-[440px] h-[685px] overflow-hidden">
                        {
                            self.page_canvas
                                .expect("reader page canvas was checked above")
                                .size(page_size)
                        }

                        // taps pass through elements without listeners, so the
                        // page zones are removed while the drawer covers them
                        {#if !self.menu_open}
                            <div
                                id="reader-previous-page"
                                on:activate={self.on_previous_page}
                                class="absolute left-0 top-0 w-[147px] h-full"
                            />

                            <div
                                id="reader-open-menu"
                                on:activate={self.on_open_menu}
                                class="absolute left-[147px] top-0 w-[146px] h-full"
                            />

                            <div
                                id="reader-next-page"
                                on:activate={self.on_next_page}
                                class="absolute right-0 top-0 w-[147px] h-full"
                            />
                        {/if}
                    </div>

                    <ReaderStatusBar
                        page_label={self.page_label}
                        progress_label={self.progress_label}
                        battery={self.battery}
                    />

                    {#if self.menu_open}
                        <ReaderMenu
                            font_size={self.font_size}
                            on_close={self.on_close_menu}
                            on_decrease_font_size={self.on_decrease_font_size}
                            on_increase_font_size={self.on_increase_font_size}
                        />
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
    battery: Option<BatteryStatus>,
}

impl RenderOnce for ReaderStatusBar<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let battery_icon = self.battery.map(battery_icon);

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

                    {#if let Some(icon) = battery_icon}
                        <Icon kind={icon} size={px(24)} />
                    {/if}
                </div>
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
    ) -> bool {
        // with the drawer open the side buttons move focus through its controls
        if app.reader.menu_open() {
            return false;
        }

        match button {
            SideButton::Previous => app.reader_previous_page(cx),
            SideButton::Next => app.reader_next_page(cx),
        }

        true
    }

    fn drag(
        &self,
        app: &mut InkPaperApp,
        origin: Point,
        position: Point,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> bool {
        // dragging along the font slider previews a size; it applies on release
        if app.reader.menu_open() && reader_menu::font_slider_contains(origin) {
            let value = reader_menu::font_slider_value_at(position.x.get());

            if app.reader.preview_font_size(font_size_from_slider(value)) {
                cx.notify();
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
        if reader_menu::font_slider_contains(position) {
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
            battery: app.system_status.battery(),
            menu_open: app.reader.menu_open(),
            font_size: app.reader.menu_font_size(),
            on_previous_page: cx.listener(InkPaperApp::activate_previous_reader_page),
            on_open_menu: cx.listener(InkPaperApp::activate_open_reader_menu),
            on_next_page: cx.listener(InkPaperApp::activate_next_reader_page),
            on_close_menu: cx.listener(InkPaperApp::activate_close_reader_menu),
            on_decrease_font_size: cx.listener(InkPaperApp::activate_decrease_reader_font_size),
            on_increase_font_size: cx.listener(InkPaperApp::activate_increase_reader_font_size),
        })
        .into_any_element()
    }
}
