use alloc::{format, string::String, vec::Vec};

use inkpaper_ui::prelude::*;

use crate::{
    InkPaperApp,
    app::{Back, Entry, Exit, ScreenInput, ScreenLifecycle, ScreenView, SideButton},
    components::{
        header::{BackHeader, BackHeaderProps, BatteryIndicator},
        option_picker::{OptionPicker, OptionPickerProps},
        page_number::{PageNumber, PageNumberProps},
        settings_row::{
            LIST_ROW_HEIGHT, SettingsToggleRow, SettingsToggleRowProps, SettingsValueRow,
            SettingsValueRowProps,
        },
    },
    fonts::{card_for, reader_family},
    paging::{self, PageTurn, Pager},
    reader::{
        PREVIEW_PADDING, PREVIEW_TEXT_HEIGHT, TextSetting, TextSettings, TextSettingsRow,
        TextSettingsTab, layout_preview, preview_text_left,
    },
    reader_page::paint_page,
};
use inkpaper_reader::Page;

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
        let list = paging::page_column(
            ("text-settings-list", 0u8),
            self.rows,
            self.page,
            ROWS_PER_PAGE,
            move |(index, row)| setting_row(index, row, text, on_row),
        );
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
) -> impl IntoElement {
    let id = ("text-settings-row", index);

    rsx! {
        {#match row}
            {:case TextSettingsRow::Setting(setting) if setting.is_toggle()}
                <SettingsToggleRow
                    id={id}
                    label={setting.screen_label()}
                    on={setting.is_on(text)}
                    on_activate={on_row}
                />

            {:case TextSettingsRow::Font { font, name }}
                <SettingsValueRow
                    id={id}
                    label={name.into()}
                    value={selected(font == text.font()).into()}
                    selected={false}
                    on_activate={Some(on_row)}
                />

            {:case TextSettingsRow::Size(size)}
                <SettingsValueRow
                    id={id}
                    label={format!("{size}").into()}
                    value={selected(size == text.font_size()).into()}
                    selected={false}
                    on_activate={Some(on_row)}
                />

            {:case TextSettingsRow::Setting(setting)}
                <SettingsValueRow
                    id={id}
                    label={setting.screen_label().into()}
                    value={setting.value(text).into()}
                    selected={false}
                    on_activate={Some(on_row)}
                />
        {/match}
    }
}

/// crosspoint marks the current font and size "Selected"
fn selected(current: bool) -> String {
    String::from(if current { "Selected" } else { "" })
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
        let ink = if self.active {
            Color::WHITE
        } else {
            Color::BLACK
        };

        rsx! {
            <div
                id={("text-settings-tab", self.index)}
                on:activate={self.on_activate}
                class="absolute left-{left} top-[3px] w-{width} h-[42px] flex items-center justify-center"
            >
                {#if self.active}
                    <div class="absolute left-0 top-0 w-full h-full rounded-md bg-black" />
                {/if}

                <text class="text-xl text-{color:ink} no-wrap max-lines-1">
                    {self.label}
                </text>
            </div>
        }
    }
}

/// What the Text Settings screen asks of the app.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TextSettingsEvent {
    Back,
    /// edit the settings to these, saved as the change is made like crosspoint
    Change(TextSettings),
}

/// the Text Settings screen. As its own entity it keeps its tab, page, open picker and
/// the preview laid out from the settings. The settings being edited are the app's,
/// and changes go to it as [`TextSettingsEvent`]s.
pub(crate) struct TextSettingsView {
    app: Entity<InkPaperApp>,
    tab: TextSettingsTab,
    /// which page of the tab's rows is shown
    pager: Pager,
    picker: Option<TextSetting>,
    /// how many rows the tab had at the last render
    row_count: usize,
    /// the preview's first page, and the settings it was laid out with
    preview: Option<Page<'static>>,
    /// the settings and whether the card font had loaded, as the preview was laid out
    preview_for: Option<(TextSettings, bool)>,
    preview_family: FontFamilyId,
}

impl EventEmitter<TextSettingsEvent> for TextSettingsView {}

impl TextSettingsView {
    pub(crate) fn new(app: Entity<InkPaperApp>) -> Self {
        Self {
            app,
            tab: TextSettingsTab::Font,
            pager: Pager::default(),
            picker: None,
            row_count: 0,
            preview: None,
            preview_for: None,
            preview_family: FontFamilyId::new(0),
        }
    }

    pub(crate) const fn picker_open(&self) -> bool {
        self.picker.is_some()
    }

    /// Returns whether a picker was open.
    pub(crate) fn close_picker(&mut self, cx: &mut Context<'_, Self>) -> bool {
        let closed = self.picker.take().is_some();

        if closed {
            cx.notify();
        }

        closed
    }

    /// While a picker is open, the rows behind it don't turn pages.
    pub(crate) fn turn_page(&mut self, turn: PageTurn, cx: &mut Context<'_, Self>) {
        if self.picker.is_none() && self.pager.turn(turn, self.row_count, ROWS_PER_PAGE) {
            cx.notify();
        }
    }

    fn change(&self, from: TextSettings, to: Option<TextSettings>, cx: &mut Context<'_, Self>) {
        if let Some(to) = to.filter(|&to| to != from) {
            cx.emit(TextSettingsEvent::Change(to));
        }
    }

    fn activate_back(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        cx.emit(TextSettingsEvent::Back);
    }

    fn activate_tab(&mut self, event: &ActivateEvent, cx: &mut Context<'_, Self>) {
        let Some(&tab) = event
            .index()
            .and_then(|index| TextSettingsTab::ALL.get(index))
        else {
            return;
        };

        if self.tab != tab {
            self.tab = tab;
            self.pager = Pager::default();
            self.picker = None;
            cx.notify();
        }
    }

    /// Picks a font or size, flips a toggle or opens a setting's picker.
    fn activate_row(&mut self, event: &ActivateEvent, cx: &mut Context<'_, Self>) {
        let Some(index) = event.index() else {
            return;
        };

        if self.picker.is_some() {
            return;
        }

        let tab = self.tab;
        let (draft, row) = self.app.read_with(cx, |app| {
            let state = &app.text_settings;

            (state.draft(), state.rows(tab).into_iter().nth(index))
        });

        match row {
            Some(TextSettingsRow::Font { font, .. }) => {
                self.change(draft, Some(draft.with_font(font)), cx);
            }
            Some(TextSettingsRow::Size(size)) => self.change(draft, draft.with_font_size(size), cx),
            Some(TextSettingsRow::Setting(setting)) if setting.is_toggle() => {
                self.change(draft, setting.toggle(draft), cx);
            }
            Some(TextSettingsRow::Setting(setting)) => {
                self.picker = Some(setting);
                cx.notify();
            }
            None => {}
        }
    }

    /// Applies option `index` of the open picker and closes it.
    fn activate_option(&mut self, event: &ActivateEvent, cx: &mut Context<'_, Self>) {
        let Some(index) = event.index() else {
            return;
        };

        let Some(setting) = self.picker.take() else {
            return;
        };
        cx.notify();

        let draft = self.app.read_with(cx, |app| app.text_settings.draft());
        self.change(draft, setting.choose(draft, index), cx);
    }

    fn activate_dismiss_picker(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.close_picker(cx);
    }

    fn paint_preview(&self, paint: &mut PaintCx<'_>) {
        if let (Some(page), Some((text, _))) = (&self.preview, self.preview_for) {
            paint_page(page, None, self.preview_family, text, paint);
        }
    }
}

impl Render for TextSettingsView {
    fn render(&mut self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let tab = self.tab;
        let app = self.app.read(cx);
        let state = &app.text_settings;
        let draft = state.draft();

        // lay the preview out again when the settings or the card font change
        let key = (draft, card_for(draft.font(), state.card_font()).is_some());
        if self.preview_for != Some(key) {
            self.preview = layout_preview(draft, state.card_font());
            self.preview_for = Some(key);
            self.preview_family = reader_family(draft.font(), state.card_font());
        }

        let rows = state.rows(tab);
        self.row_count = rows.len();

        TextSettingsScreen::from(TextSettingsScreenProps {
            tab,
            text: draft,
            font_name: state.font_name(draft.font()),
            page: self.pager.page(rows.len(), ROWS_PER_PAGE),
            page_count: Pager::page_count(rows.len(), ROWS_PER_PAGE),
            rows,
            picker: self.picker,
            preview: cx.canvas(|view: &Self, paint| view.paint_preview(paint)),
            battery: app.battery_indicator,
            on_back: cx.listener(Self::activate_back),
            on_tab: cx.listener(Self::activate_tab),
            on_row: cx.listener(Self::activate_row),
            on_option: cx.listener(Self::activate_option),
            on_dismiss_picker: cx.listener(Self::activate_dismiss_picker),
        })
    }
}

pub(crate) struct TextSettingsRoute;

impl ScreenLifecycle for TextSettingsRoute {
    fn enter(&self, app: &mut InkPaperApp, entry: Entry, cx: &mut Context<'_, InkPaperApp>) {
        if entry == Entry::Opened {
            app.text_settings.open(app.reader.menu_text_settings());

            // a new view opens on the Font tab, like crosspoint's screen
            let view =
                InkPaperApp::open_view(&mut app.text_settings_view, cx, TextSettingsView::new);
            cx.subscribe(view, InkPaperApp::on_text_settings_event);
        }
    }

    /// An open book is laid out again with the new settings.
    fn exit(&self, app: &mut InkPaperApp, exit: Exit, cx: &mut Context<'_, InkPaperApp>) {
        if exit == Exit::Closed {
            app.reader.request_text_settings(app.text_settings.draft());
            InkPaperApp::close_view(&mut app.text_settings_view, cx);
        }
    }

    /// Back closes an open picker first.
    fn back(&self, app: &mut InkPaperApp, cx: &mut Context<'_, InkPaperApp>) -> Back {
        let closed = app
            .text_settings_view
            .is_some_and(|view| view.update(cx, TextSettingsView::close_picker));

        if closed { Back::Handled } else { Back::Leave }
    }
}

impl ScreenInput for TextSettingsRoute {
    fn side_button(
        &self,
        app: &mut InkPaperApp,
        button: SideButton,
        cx: &mut Context<'_, InkPaperApp>,
    ) {
        if let Some(view) = app.text_settings_view {
            view.update(cx, |view, cx| view.turn_page(button.into(), cx));
        }
    }

    fn drag(
        &self,
        app: &mut InkPaperApp,
        origin: Point,
        position: Point,
        cx: &mut Context<'_, InkPaperApp>,
    ) {
        let Some(view) = app.text_settings_view else {
            return;
        };

        if view.read_with(cx, TextSettingsView::picker_open) {
            return;
        }

        // once a swipe turns a page, the rest of the touch turns nothing more
        if let Some(turn) = paging::swipe_turn(origin, position) {
            view.update(cx, |view, cx| view.turn_page(turn, cx));
            app.capture_pointer();
        }
    }
}

impl ScreenView for TextSettingsRoute {
    fn render<'a>(&self, app: &'a InkPaperApp, _: &mut Context<'_, InkPaperApp>) -> AnyElement<'a> {
        app.text_settings_view
            .map_or_else(|| div().into_any_element(), IntoElement::into_any_element)
    }
}
