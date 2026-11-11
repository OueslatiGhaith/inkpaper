use alloc::string::String;

use inkpaper_ui::prelude::*;

use crate::{
    InkPaperApp,
    components::{
        drawer_handle::{DrawerHandle, DrawerHandleProps},
        option_picker::{OptionPicker, OptionPickerProps},
        settings_row::{ListRow, ListRowProps},
    },
    reader::{TextSetting, TextSettings},
};

/// The drawer's tabs, by their icons in its tab bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ReaderMenuTab {
    #[default]
    Text,
    More,
}

const CASE_SENSITIVE: SvgSource = include_svg!("assets/icons/lucide/case-sensitive.svg");
const ELLIPSIS: SvgSource = include_svg!("assets/icons/lucide/ellipsis.svg");

/// Reader drawer: a bottom sheet over the page with a handle, a content pane and
/// an icon tab bar. Tabs are added as their features exist. The reader screen closes
/// it on taps above the sheet.
#[component]
pub(crate) struct ReaderMenu {
    tab: ReaderMenuTab,
    text: TextSettings,
    /// the Font row's value: the family's name
    font_name: String,
    on_close: Listener<ActivateEvent>,
    on_text_tab: Listener<ActivateEvent>,
    on_more_tab: Listener<ActivateEvent>,
    /// a Text panel row; its index in [`TextSetting::PANEL`] is its element id
    on_text_row: Listener<ActivateEvent>,
    on_select_chapter: Listener<ActivateEvent>,
    /// whether the page has links, which Links and footnotes lists
    has_links: bool,
    on_links: Listener<ActivateEvent>,
}

impl RenderOnce for ReaderMenu {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let text = self.text;
        let font_name = self.font_name;
        let on_text_row = self.on_text_row;

        // crosspoint's Text panel: one row per setting, its value on the right
        let text_rows = TextSetting::PANEL
            .iter()
            .enumerate()
            .map(move |(index, &setting)| {
                TextRow::from(TextRowProps {
                    index,
                    label: setting.label(),
                    value: if setting == TextSetting::Font {
                        font_name.clone()
                    } else {
                        setting.value(text)
                    },
                    on_activate: on_text_row,
                })
            });
        let text_rows = div().w_full().flex().flex_col().children(text_rows);

        rsx! {
            // 429 px sheet with a 3 px top rule
            <div class="absolute left-0 top-[371px] w-[480px] h-[429px] bg-white">
                <div class="absolute left-0 top-0 w-full h-[3px] bg-black" />

                <div
                    id="reader-menu-handle"
                    on:activate={self.on_close}
                    class="absolute left-[188px] top-0 w-[104px] h-[29px]"
                >
                    <div class="absolute left-4 top-[13px]">
                        <DrawerHandle />
                    </div>
                </div>

                {#if self.tab == ReaderMenuTab::Text}
                    <div class="absolute left-0 top-8 w-[480px]">
                        {text_rows}
                    </div>
                {:else}
                    <div class="absolute left-0 top-8 w-[480px] flex flex-col">
                        <ListRow
                            id={("reader-menu-select-chapter", 0)}
                            label="Select Chapter"
                            depth={0}
                            selected={false}
                            chevron={true}
                            on_activate={Some(self.on_select_chapter)}
                        />

                        // like crosspoint, only on pages with links
                        {#if self.has_links}
                            <ListRow
                                id={("reader-menu-links", 0)}
                                label="Links and footnotes"
                                depth={0}
                                selected={false}
                                chevron={true}
                                on_activate={Some(self.on_links)}
                            />
                        {/if}
                    </div>
                {/if}

                // 66 px tab bar under a 1 px rule; the active tab is inverted
                <div class="absolute left-0 bottom-0 w-full h-[66px]">
                    <div class="absolute left-0 top-0 w-full h-px bg-black" />

                    <MenuTab
                        id="reader-menu-text-tab"
                        icon={CASE_SENSITIVE}
                        icon_size={px(32)}
                        left={px(4)}
                        active={self.tab == ReaderMenuTab::Text}
                        on_activate={self.on_text_tab}
                    />

                    <MenuTab
                        id="reader-menu-more-tab"
                        icon={ELLIPSIS}
                        icon_size={px(24)}
                        left={px(244)}
                        active={self.tab == ReaderMenuTab::More}
                        on_activate={self.on_more_tab}
                    />
                </div>
            </div>
        }
    }
}

/// What the reader's drawer asks of the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DrawerEvent {
    Close,
    /// the Font row opens the Text Settings screen, like crosspoint's
    OpenTextSettings,
    /// a toggle row of the Text panel flips in place, like crosspoint's checkboxes
    ToggleTextSetting(TextSetting),
    /// option `1` of the picker of setting `0` was chosen
    ChooseTextOption(TextSetting, usize),
    SelectChapter,
    /// entry `0` of Links and footnotes was chosen
    FollowLink(usize),
}

/// The picker the drawer shows over the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DrawerPicker {
    Text(TextSetting),
    Links,
}

/// the reader's drawer. As its own entity, a tab switch or a picker renders and
/// paints only the drawer, not the page under it. It keeps its tab and its open
/// picker, and reads the book's settings and links from the app, so it renders again
/// when they change. It tells the app what was chosen with [`DrawerEvent`]s.
pub(crate) struct ReaderDrawer {
    app: Entity<InkPaperApp>,
    tab: ReaderMenuTab,
    picker: Option<DrawerPicker>,
}

impl EventEmitter<DrawerEvent> for ReaderDrawer {}

impl ReaderDrawer {
    pub(crate) const fn new(app: Entity<InkPaperApp>) -> Self {
        Self {
            app,
            tab: ReaderMenuTab::Text,
            picker: None,
        }
    }

    /// Shows the Text tab with no picker, as the drawer opens with.
    pub(crate) fn reset(&mut self, cx: &mut Context<'_, Self>) {
        if self.tab != ReaderMenuTab::Text || self.picker.is_some() {
            self.tab = ReaderMenuTab::Text;
            self.picker = None;
            cx.notify();
        }
    }

    fn select_tab(&mut self, tab: ReaderMenuTab, cx: &mut Context<'_, Self>) {
        if self.tab != tab {
            self.tab = tab;
            cx.notify();
        }
    }

    fn open_picker(&mut self, picker: DrawerPicker, cx: &mut Context<'_, Self>) {
        if self.picker.is_none() {
            self.picker = Some(picker);
            cx.notify();
        }
    }

    fn close_picker(&mut self, cx: &mut Context<'_, Self>) -> Option<DrawerPicker> {
        let closed = self.picker.take();

        if closed.is_some() {
            cx.notify();
        }

        closed
    }

    fn activate_text_tab(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.select_tab(ReaderMenuTab::Text, cx);
    }

    fn activate_more_tab(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.select_tab(ReaderMenuTab::More, cx);
    }

    fn activate_close(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        cx.emit(DrawerEvent::Close);
    }

    fn activate_text_row(&mut self, event: &ActivateEvent, cx: &mut Context<'_, Self>) {
        let Some(&setting) = event
            .index()
            .and_then(|index| TextSetting::PANEL.get(index))
        else {
            return;
        };

        if setting == TextSetting::Font {
            cx.emit(DrawerEvent::OpenTextSettings);
        } else if setting.is_toggle() {
            cx.emit(DrawerEvent::ToggleTextSetting(setting));
        } else {
            self.open_picker(DrawerPicker::Text(setting), cx);
        }
    }

    fn activate_select_chapter(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        cx.emit(DrawerEvent::SelectChapter);
    }

    /// With one link it follows it, like crosspoint, and with several it lists them.
    fn activate_links(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        match self.app.read(cx, |app| app.reader.page_link_labels().len()) {
            0 => {}
            1 => cx.emit(DrawerEvent::FollowLink(0)),
            _ => self.open_picker(DrawerPicker::Links, cx),
        }
    }

    fn activate_option(&mut self, event: &ActivateEvent, cx: &mut Context<'_, Self>) {
        let Some(index) = event.index() else {
            return;
        };

        match self.close_picker(cx) {
            Some(DrawerPicker::Text(setting)) => {
                cx.emit(DrawerEvent::ChooseTextOption(setting, index));
            }
            Some(DrawerPicker::Links) => cx.emit(DrawerEvent::FollowLink(index)),
            None => {}
        }
    }

    fn activate_dismiss_picker(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.close_picker(cx);
    }
}

impl Render for ReaderDrawer {
    fn render(&mut self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let picker = self.picker;
        let (text, font_name, has_links, link_labels) = self.app.read(cx, |app| {
            let links = app.reader.page_link_labels();

            (
                app.reader.menu_text_settings(),
                app.reader_font_name(),
                !links.is_empty(),
                (picker == Some(DrawerPicker::Links)).then_some(links),
            )
        });

        let menu = ReaderMenu::from(ReaderMenuProps {
            tab: self.tab,
            text,
            font_name,
            on_close: cx.listener(Self::activate_close),
            on_text_tab: cx.listener(Self::activate_text_tab),
            on_more_tab: cx.listener(Self::activate_more_tab),
            on_text_row: cx.listener(Self::activate_text_row),
            on_select_chapter: cx.listener(Self::activate_select_chapter),
            has_links,
            on_links: cx.listener(Self::activate_links),
        });

        let picker = match picker {
            Some(DrawerPicker::Text(setting)) => Some(OptionPicker::from(OptionPickerProps {
                title: setting.label(),
                options: setting.options(),
                selected: setting.selected(text),
                on_option: cx.listener(Self::activate_option),
                on_dismiss: cx.listener(Self::activate_dismiss_picker),
            })),
            Some(DrawerPicker::Links) => Some(OptionPicker::from(OptionPickerProps {
                title: "Links and footnotes",
                options: link_labels.unwrap_or_default(),
                selected: None,
                on_option: cx.listener(Self::activate_option),
                on_dismiss: cx.listener(Self::activate_dismiss_picker),
            })),
            None => None,
        };

        // a zero-size anchor at the screen's corner: the sheet and a picker place
        // themselves on the screen, and only what they paint is damaged
        div()
            .absolute()
            .left(px(0))
            .top(px(0))
            .child(menu)
            .children(picker)
    }
}

/// A Text panel row: the setting on the left, its value on the right, like
/// crosspoint's panel rows.
#[component]
struct TextRow {
    index: usize,
    label: &'static str,
    value: String,
    on_activate: Listener<ActivateEvent>,
}

impl RenderOnce for TextRow {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div
                id={("reader-menu-text-row", self.index)}
                on:activate={self.on_activate}
                class="w-full h-16 relative"
            >
                <div class="absolute left-8 top-0 w-[280px] h-16 flex items-center">
                    <text class="text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.label}
                    </text>
                </div>

                <div class="absolute right-8 top-0 w-[120px] h-16 flex items-center justify-end">
                    <text class="text-xl font-bold no-wrap max-lines-1">
                        {self.value}
                    </text>
                </div>
            </div>
        }
    }
}

#[component]
struct MenuTab {
    id: &'static str,
    icon: SvgSource,
    icon_size: Pixels,
    left: Pixels,
    active: bool,
    on_activate: Listener<ActivateEvent>,
}

impl RenderOnce for MenuTab {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let (background, foreground) = if self.active {
            (Color::BLACK, Color::WHITE)
        } else {
            (Color::WHITE, Color::BLACK)
        };

        rsx! {
            <div
                id={self.id}
                on:activate={self.on_activate}
                class="absolute left-{self.left} top-2 w-[232px] h-[46px] rounded-sm bg-{background} flex items-center justify-center"
            >
                {
                    svg(self.icon)
                        .size(Size::new(self.icon_size, self.icon_size))
                        .text_color(foreground)
                }
            </div>
        }
    }
}
