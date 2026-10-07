use alloc::string::String;

use inkpaper_ui::prelude::*;

use crate::{
    InkPaperApp,
    components::{
        drawer_handle::{DrawerHandle, DrawerHandleProps},
        settings_row::{ListRow, ListRowProps},
    },
    reader::{ReaderMenuTab, TextSetting, TextSettings},
};

const CASE_SENSITIVE: SvgSource = include_svg!("assets/icons/lucide/case-sensitive.svg");
const ELLIPSIS: SvgSource = include_svg!("assets/icons/lucide/ellipsis.svg");

/// Reader drawer: a bottom sheet over the page with a handle, a content pane and
/// an icon tab bar. Tabs are added as their features exist. The reader screen closes
/// it on taps above the sheet.
#[component]
pub(crate) struct ReaderMenu {
    tab: ReaderMenuTab,
    text: TextSettings,
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
        let on_text_row = self.on_text_row;

        // crosspoint's Text panel: one row per setting, its value on the right
        let text_rows = TextSetting::PANEL
            .iter()
            .enumerate()
            .map(move |(index, &setting)| {
                TextRow::from(TextRowProps {
                    index,
                    label: setting.label(),
                    value: setting.value(text),
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

/// the reader's drawer. As its own entity, a font size preview or a tab switch renders
/// and paints only the drawer, not the page under it. Its state stays in the app, which
/// renders it again through [`InkPaperApp::refresh_reader_menu`].
pub(crate) struct ReaderMenuView {
    app: Entity<InkPaperApp>,
}

impl ReaderMenuView {
    pub(crate) const fn new(app: Entity<InkPaperApp>) -> Self {
        Self { app }
    }
}

impl Render for ReaderMenuView {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        // the listeners target the app, so its handlers can render this drawer again
        // without borrowing it
        let props = self
            .app
            .update(cx, |app, cx| ReaderMenuProps {
                tab: app.reader.menu_tab(),
                text: app.reader.menu_text_settings(),
                on_close: cx.listener(InkPaperApp::activate_close_reader_menu),
                on_text_tab: cx.listener(InkPaperApp::activate_reader_text_tab),
                on_more_tab: cx.listener(InkPaperApp::activate_reader_more_tab),
                on_text_row: cx.listener(InkPaperApp::activate_reader_text_row),
                on_select_chapter: cx.listener(InkPaperApp::show_table_of_contents),
                has_links: !app.reader.page_links().is_empty(),
                on_links: cx.listener(InkPaperApp::activate_reader_links),
            })
            .expect("the app outlives its reader menu");

        ReaderMenu::from(props)
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
