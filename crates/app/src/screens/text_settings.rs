use alloc::{format, string::String, vec::Vec};

use inkpaper_ui::prelude::*;

use crate::{
    InkPaperApp,
    app::{Back, Entry, Exit, ScreenInput, ScreenLifecycle, ScreenView, SideButton},
    components::{
        header::{BackHeader, BackHeaderProps, BatteryIndicator},
        option_picker::{OptionPicker, OptionPickerProps},
        page_number::{PageNumber, PageNumberProps},
        settings_row::{LIST_ROW_HEIGHT, SettingsToggleRow, SettingsToggleRowProps},
    },
    paging,
    reader::{
        PREVIEW_PADDING, PREVIEW_TEXT_HEIGHT, TextSetting, TextSettings, TextSettingsRow,
        TextSettingsTab, preview_text_left,
    },
};

/// The preview sits between the header and the tabs, 30% of the screen below
/// the header like crosspoint's.
const PREVIEW_TOP: i32 = 82;
const PREVIEW_HEIGHT: i32 = 215;
const TABS_TOP: i32 = PREVIEW_TOP + PREVIEW_HEIGHT;
const TABS_HEIGHT: i32 = 50;
/// crossink's list starts 16 px below its tabs
const LIST_TOP: i32 = TABS_TOP + TABS_HEIGHT + 16;
const LIST_HEIGHT: i32 = 800 - LIST_TOP;
const ROWS_PER_PAGE: usize = (LIST_HEIGHT / LIST_ROW_HEIGHT) as usize;
const TAB_WIDTH: i32 = 120;

/// crosspoint's Text Settings: a live preview of the reader's text above
/// Font, Size, Layout and Style tabs of settings.
#[component]
pub(crate) struct TextSettingsScreen {
    tab: TextSettingsTab,
    text: TextSettings,
    /// the chosen family's name
    font_name: String,
    rows: Vec<TextSettingsRow>,
    page: usize,
    page_count: usize,
    picker: Option<TextSetting>,
    preview: Canvas,
    battery: Entity<BatteryIndicator>,
    on_back: Listener<ActivateEvent>,
    on_tab: Listener<ActivateEvent>,
    on_row: Listener<ActivateEvent>,
    on_option: Listener<ActivateEvent>,
    on_dismiss_picker: Listener<ActivateEvent>,
}

impl RenderOnce for TextSettingsScreen {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let text = self.text;

        let preview_left = px(preview_text_left(text) as i32);
        let preview_top = px(PREVIEW_PADDING as i32);
        let preview_size = Size::new(
            px(480 - 2 * (preview_text_left(text) as i32)),
            px(PREVIEW_TEXT_HEIGHT as i32),
        );
        let label_top = px(PREVIEW_HEIGHT - PREVIEW_PADDING as i32 - 20);
        let label = format!("Preview \"{}, {}\"", self.font_name, text.font_size());

        let tab = self.tab;
        let on_tab = self.on_tab;
        let tabs = TextSettingsTab::ALL
            .iter()
            .enumerate()
            .map(move |(index, &item)| {
                Tab::from(TabProps {
                    index,
                    label: item.label(),
                    active: item == tab,
                    on_activate: on_tab,
                })
            });
        let tabs = div().w_full().h_full().relative().children(tabs);

        // one page of the tab's rows
        let on_row = self.on_row;
        let rows = self
            .rows
            .into_iter()
            .enumerate()
            .skip(self.page * ROWS_PER_PAGE)
            .take(ROWS_PER_PAGE)
            .map(move |(index, row)| setting_row(index, row, text, on_row));
        let list = div()
            .id(("text-settings-list", 0u8))
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .children(rows);
        let paged = self.page_count > 1;

        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title="Text Settings"
                        battery={self.battery}
                        on_back={self.on_back}
                    />
                </div>

                <div class="absolute left-0 top-{px(PREVIEW_TOP)} w-[480px] h-{px(PREVIEW_HEIGHT)}">
                    <div class="absolute left-{preview_left} top-{preview_top} w-{preview_size.width} h-{preview_size.height} overflow-hidden">
                        {self.preview.size(preview_size)}
                    </div>

                    <div class="absolute left-3 top-{label_top} h-5 flex items-center">
                        <text class="text-base no-wrap max-lines-1">
                            {label}
                        </text>
                    </div>
                </div>

                // crossink's touch tabs: full width between two rules
                <div class="absolute left-0 top-{px(TABS_TOP)} w-[480px] h-{px(TABS_HEIGHT)}">
                    <div class="absolute left-0 top-0 w-full h-px bg-black" />
                    {tabs}
                    <div class="absolute left-0 bottom-0 w-full h-px bg-black" />
                </div>

                <div class="absolute left-0 top-{px(LIST_TOP)} w-[480px] h-{px(LIST_HEIGHT)}">
                    {list}
                </div>

                {#if paged}
                    <div class="absolute right-5 bottom-3">
                        <PageNumber page={self.page} page_count={self.page_count} />
                    </div>
                {/if}

                {#if let Some(setting) = self.picker}
                    <OptionPicker
                        title={setting.screen_label()}
                        options={setting.options()}
                        selected={setting.selected(text)}
                        on_option={self.on_option}
                        on_dismiss={self.on_dismiss_picker}
                    />
                {/if}
            </div>
        }
    }
}

fn setting_row(
    index: usize,
    row: TextSettingsRow,
    text: TextSettings,
    on_row: Listener<ActivateEvent>,
) -> AnyElement<'static> {
    let id = ("text-settings-row", index);

    let (label, value) = match row {
        TextSettingsRow::Font { font, name } => (name, selected(font == text.font())),

        TextSettingsRow::Size(size) => (format!("{size}"), selected(size == text.font_size())),

        TextSettingsRow::Setting(setting) if setting.is_toggle() => {
            return SettingsToggleRow::from(SettingsToggleRowProps {
                id,
                label: setting.screen_label(),
                on: setting.is_on(text),
                on_activate: on_row,
            })
            .into_any_element();
        }

        TextSettingsRow::Setting(setting) => {
            (String::from(setting.screen_label()), setting.value(text))
        }
    };

    ValueRow::from(ValueRowProps {
        id,
        label,
        value,
        on_activate: on_row,
    })
    .into_any_element()
}

/// crosspoint marks the current font and size "Selected"
fn selected(current: bool) -> String {
    String::from(if current { "Selected" } else { "" })
}

/// [`SettingsValueRow`]'s look, owning its strings since sizes and values are
/// formatted as the rows are built.
#[component]
struct ValueRow {
    id: (&'static str, usize),
    label: String,
    value: String,
    on_activate: Listener<ActivateEvent>,
}

impl RenderOnce for ValueRow {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let height = px(LIST_ROW_HEIGHT);

        rsx! {
            <div class="w-full h-{height} relative">
                <div
                    id={self.id}
                    on:activate={self.on_activate}
                    class="absolute left-5 top-0 w-[440px] h-{height} rounded-md"
                />

                <div class="absolute left-7 top-0 w-[290px] h-{height} flex items-center">
                    <text class="text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.label}
                    </text>
                </div>

                <div class="absolute right-7 top-0 w-[130px] h-{height} flex items-center justify-end">
                    <text class="text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.value}
                    </text>
                </div>
            </div>
        }
    }
}

/// A tab: a quarter of the width, inverted when active.
#[component]
struct Tab {
    index: usize,
    label: &'static str,
    active: bool,
    on_activate: Listener<ActivateEvent>,
}

impl RenderOnce for Tab {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let left = px(self.index as i32 * TAB_WIDTH + 2);
        let width = px(TAB_WIDTH - 4);

        rsx! {
            <div
                id={("text-settings-tab", self.index)}
                on:activate={self.on_activate}
                class="absolute left-{left} top-[3px] w-{width} h-[42px] flex items-center justify-center"
            >
                {#if self.active}
                    <div class="absolute left-0 top-0 w-full h-full rounded-md bg-black" />
                    <text class="text-xl text-white no-wrap max-lines-1">
                        {self.label}
                    </text>
                {:else}
                    <text class="text-xl no-wrap max-lines-1">
                        {self.label}
                    </text>
                {/if}
            </div>
        }
    }
}

pub(crate) struct TextSettingsRoute;

impl ScreenLifecycle for TextSettingsRoute {
    fn enter(&self, app: &mut InkPaperApp, entry: Entry) {
        if entry == Entry::Opened {
            app.text_settings.open(app.reader.menu_text_settings());
        }
    }

    /// An open book is laid out again with the new settings.
    fn exit(&self, app: &mut InkPaperApp, exit: Exit) {
        if exit == Exit::Closed {
            app.reader.request_text_settings(app.text_settings.draft());
        }
    }

    /// Back closes an open picker first.
    fn back(&self, app: &mut InkPaperApp, cx: &mut Context<'_, InkPaperApp>) -> Back {
        if app.text_settings.close_picker() {
            cx.notify();
            return Back::Handled;
        }

        Back::Leave
    }
}

// while a picker is open, the rows behind it don't turn pages
impl ScreenInput for TextSettingsRoute {
    fn side_button(
        &self,
        app: &mut InkPaperApp,
        button: SideButton,
        cx: &mut Context<'_, InkPaperApp>,
    ) {
        if app.text_settings.picker().is_some() {
            return;
        }

        paging::turn_page_by_button(app, button, cx, |app, turn| {
            app.text_settings.turn_page(turn, ROWS_PER_PAGE)
        });
    }

    fn drag(
        &self,
        app: &mut InkPaperApp,
        origin: Point,
        position: Point,
        cx: &mut Context<'_, InkPaperApp>,
    ) {
        if app.text_settings.picker().is_some() {
            return;
        }

        paging::turn_page_by_swipe(app, origin, position, cx, |app, turn| {
            app.text_settings.turn_page(turn, ROWS_PER_PAGE)
        });
    }
}

impl ScreenView for TextSettingsRoute {
    fn render<'a>(
        &self,
        app: &'a InkPaperApp,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> AnyElement<'a> {
        let state = &app.text_settings;

        TextSettingsScreen::from(TextSettingsScreenProps {
            tab: state.tab(),
            text: state.draft(),
            font_name: state.font_name(state.draft().font()),
            rows: state.rows(),
            page: state.page(ROWS_PER_PAGE),
            page_count: state.page_count(ROWS_PER_PAGE),
            picker: state.picker(),
            preview: cx.canvas(|app: &InkPaperApp, paint| app.paint_text_settings_preview(paint)),
            battery: app.battery_indicator,
            on_back: cx.listener(InkPaperApp::activate_back),
            on_tab: cx.listener(InkPaperApp::activate_text_settings_tab),
            on_row: cx.listener(InkPaperApp::activate_text_settings_row),
            on_option: cx.listener(InkPaperApp::activate_text_settings_option),
            on_dismiss_picker: cx.listener(InkPaperApp::activate_dismiss_text_settings_picker),
        })
        .into_any_element()
    }
}
