use alloc::format;
use inkpaper_ui::prelude::*;

use crate::{
    InkPaperApp,
    app::{ScreenInput, ScreenLifecycle, ScreenView, SideButton},
    components::icon::{Icon, IconKind, IconProps},
    reader::reader_viewport,
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
    section_label: &'a str,

    controls_visible: bool,

    font_size: u16,

    on_decrease_font_size: Listener<ActivateEvent>,
    on_increase_font_size: Listener<ActivateEvent>,
    on_previous_page: Listener<ActivateEvent>,
    on_toggle_controls: Listener<ActivateEvent>,
    on_next_page: Listener<ActivateEvent>,
    on_back: Listener<ActivateEvent>,
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

                        <div
                            id="reader-previous-page"
                            on:activate={self.on_previous_page}
                            class="absolute left-0 top-0 w-[147px] h-full"
                        />

                        <div
                            id="reader-toggle-controls"
                            on:activate={self.on_toggle_controls}
                            class="absolute left-[147px] top-0 w-[146px] h-full"
                        />

                        <div
                            id="reader-next-page"
                            on:activate={self.on_next_page}
                            class="absolute right-0 top-0 w-[147px] h-full"
                        />
                    </div>

                    <ReaderStatusBar
                        title={self.title}
                        page_label={self.page_label}
                        section_label={self.section_label}
                    />

                    {#if self.controls_visible}
                        <ReaderTopControls
                            title={self.title}
                            font_size={self.font_size}
                            show_font_controls={true}
                            on_back={self.on_back}
                            on_decrease_font_size={self.on_decrease_font_size}
                            on_increase_font_size={self.on_increase_font_size}
                        />
                    {/if}

                {:else}
                    <ReaderTopControls
                        title={self.title}
                        font_size={self.font_size}
                        show_font_controls={false}
                        on_back={self.on_back}
                        on_decrease_font_size={self.on_decrease_font_size}
                        on_increase_font_size={self.on_increase_font_size}
                    />

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

#[component]
struct ReaderTopControls<'a> {
    title: &'a str,
    font_size: u16,
    show_font_controls: bool,

    on_back: Listener<ActivateEvent>,
    on_decrease_font_size: Listener<ActivateEvent>,
    on_increase_font_size: Listener<ActivateEvent>,
}

impl RenderOnce for ReaderTopControls<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let font_size = format!("{}", self.font_size);

        rsx! {
            <div class="absolute left-0 top-0 w-[480px] h-[82px] bg-white">
                <div
                    id="reader-back"
                    on:activate={self.on_back}
                    class="absolute left-0 top-3 w-[52px] h-[52px] flex items-center justify-center focus:bg-[#aaaaaa]"
                >
                    <Icon kind={IconKind::ChevronLeft} size={px(32)} />
                </div>

                <div class="absolute left-[60px] top-3 w-[235px] h-[52px] flex items-center">
                    <text class="font-bold text-2xl no-wrap max-lines-1 text-ellipsis">
                        {self.title}
                    </text>
                </div>

                {#if self.show_font_controls}
                    <div
                        id="reader-font-size-decrease"
                        on:activate={self.on_decrease_font_size}
                        class="absolute left-[305px] top-3 w-[50px] h-[52px] flex items-center justify-center focus:bg-[#aaaaaa]"
                    >
                        <text class="font-bold text-xl">
                            {"A-"}
                        </text>
                    </div>

                    <div class="absolute left-[355px] top-3 w-[70px] h-[52px] flex items-center justify-center">
                        <text class="text-lg">
                            {font_size}
                        </text>
                    </div>

                    <div
                        id="reader-font-size-increase"
                        on:activate={self.on_increase_font_size}
                        class="absolute right-0 top-3 w-[55px] h-[52px] flex items-center justify-center focus:bg-[#aaaaaa]"
                    >
                        <text class="font-bold text-xl">
                            {"A+"}
                        </text>
                    </div>
                {/if}

                <div class="absolute left-5 bottom-0 w-[440px] h-[1px] bg-black" />
            </div>
        }
    }
}

#[component]
struct ReaderStatusBar<'a> {
    title: &'a str,
    page_label: &'a str,
    section_label: &'a str,
}

impl RenderOnce for ReaderStatusBar<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="absolute left-5 top-[735px] w-[440px] h-[45px] bg-white">
                <div class="absolute left-0 top-0 w-full h-[1px] bg-black" />

                <div class="absolute left-0 top-2 w-[190px] h-7 flex items-center">
                    <text class="text-base no-wrap max-lines-1 text-ellipsis">
                        {self.title}
                    </text>
                </div>

                <div class="absolute left-[190px] top-2 w-[100px] h-7 flex items-center justify-center">
                    <text class="text-base">
                        {self.page_label}
                    </text>
                </div>

                <div class="absolute right-0 top-2 w-[150px] h-7 flex items-center justify-end">
                    <text class="text-base no-wrap max-lines-1">
                        {self.section_label}
                    </text>
                </div>
            </div>
        }
    }
}

pub(crate) struct ReaderRoute;

impl ScreenLifecycle for ReaderRoute {}

impl ScreenInput for ReaderRoute {
    fn side_button(
        &self,
        app: &mut InkPaperApp,
        button: SideButton,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> bool {
        match button {
            SideButton::Previous => app.reader_previous_page(cx),
            SideButton::Next => app.reader_next_page(cx),
        }

        true
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
            section_label: app.reader.section_label(),
            controls_visible: app.reader.controls_visible(),
            font_size: app.reader.font_size(),
            on_decrease_font_size: cx.listener(InkPaperApp::activate_decrease_reader_font_size),
            on_increase_font_size: cx.listener(InkPaperApp::activate_increase_reader_font_size),
            on_previous_page: cx.listener(InkPaperApp::activate_previous_reader_page),
            on_toggle_controls: cx.listener(InkPaperApp::activate_toggle_reader_controls),
            on_next_page: cx.listener(InkPaperApp::activate_next_reader_page),
            on_back: cx.listener(InkPaperApp::activate_back),
        })
        .into_any_element()
    }
}
