use alloc::{vec, vec::Vec};

use inkpaper_epub::{ArchivePath, Chapter, ChapterStyles, SpineIndex};
use inkpaper_reader::{Page, Viewport, paginate_chapter};

use super::{
    TextSetting, TextSettings,
    measurer::{ReaderMeasurer, reader_fonts},
    text_settings::font_sizes,
};

/// The preview's inner padding, like crosspoint's.
pub(crate) const PREVIEW_PADDING: u32 = 12;

/// The height the preview text gets, above its label.
pub(crate) const PREVIEW_TEXT_HEIGHT: u32 = 163;

const PREVIEW_WIDTH: u32 = 480 - 2 * PREVIEW_PADDING;

/// crosspoint's preview sentence, with a second paragraph so indentation,
/// paragraph spacing, alignment and hyphenation show too
const PREVIEW_XHTML: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
<p>The quick brown fox jumps over the lazy dog. Typography arranges type so
written language is legible, readable and appealing on the page.</p>
<p>Justification and hyphenation work together, so extraordinarily long words
leave even gaps between the others.</p>
</body></html>"#;

/// The preview is English, so hyphenation shows whatever the book's language.
const PREVIEW_LANGUAGE: &str = "en";

/// crosspoint's Text Settings tabs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextSettingsTab {
    #[default]
    Font,
    Size,
    Layout,
    Style,
}

impl TextSettingsTab {
    pub(crate) const ALL: [Self; 4] = [Self::Font, Self::Size, Self::Layout, Self::Style];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Font => "Font",
            Self::Size => "Size",
            Self::Layout => "Layout",
            Self::Style => "Style",
        }
    }
}

/// A row of a Text Settings tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextSettingsRow {
    /// the reader font, the only one until custom fonts
    Font,
    Size(u16),
    Setting(TextSetting),
}

/// The Text Settings screen: its tab, the settings being edited, the open
/// picker and the preview laid out from them.
#[derive(Debug, Default)]
pub(crate) struct TextSettingsState {
    tab: TextSettingsTab,
    draft: TextSettings,
    picker: Option<TextSetting>,
    preview: Option<Page<'static>>,
}

impl TextSettingsState {
    /// Starts editing `text`, on the Font tab like crosspoint.
    pub(crate) fn open(&mut self, text: TextSettings) {
        self.tab = TextSettingsTab::Font;
        self.picker = None;
        self.draft = text;
        self.preview = layout_preview(text);
    }

    pub(crate) const fn tab(&self) -> TextSettingsTab {
        self.tab
    }

    pub(crate) const fn draft(&self) -> TextSettings {
        self.draft
    }

    pub(crate) const fn picker(&self) -> Option<TextSetting> {
        self.picker
    }

    pub(crate) fn preview(&self) -> Option<&Page<'static>> {
        self.preview.as_ref()
    }

    pub(crate) fn select_tab(&mut self, tab: TextSettingsTab) -> bool {
        if self.tab == tab {
            return false;
        }

        self.tab = tab;
        self.picker = None;

        true
    }

    pub(crate) fn rows(&self) -> Vec<TextSettingsRow> {
        match self.tab {
            TextSettingsTab::Font => vec![TextSettingsRow::Font],
            TextSettingsTab::Size => font_sizes().map(TextSettingsRow::Size).collect(),
            TextSettingsTab::Layout => TextSetting::LAYOUT
                .iter()
                .map(|&setting| TextSettingsRow::Setting(setting))
                .collect(),
            TextSettingsTab::Style => TextSetting::STYLE
                .iter()
                .map(|&setting| TextSettingsRow::Setting(setting))
                .collect(),
        }
    }

    /// Picks a size, flips a toggle or opens a setting's picker. Returns
    /// whether the screen changed.
    pub(crate) fn activate_row(&mut self, index: usize) -> bool {
        if self.picker.is_some() {
            return false;
        }

        match self.rows().get(index) {
            Some(&TextSettingsRow::Size(size)) => self
                .draft
                .with_font_size(size)
                .is_some_and(|text| self.set_draft(text)),

            Some(&TextSettingsRow::Setting(setting)) if setting.is_toggle() => setting
                .toggle(self.draft)
                .is_some_and(|text| self.set_draft(text)),

            Some(&TextSettingsRow::Setting(setting)) => {
                self.picker = Some(setting);
                true
            }

            Some(TextSettingsRow::Font) | None => false,
        }
    }

    /// Applies option `index` of the open picker and closes it.
    pub(crate) fn choose_option(&mut self, index: usize) -> bool {
        let Some(setting) = self.picker.take() else {
            return false;
        };

        if let Some(text) = setting.choose(self.draft, index) {
            self.set_draft(text);
        }

        true
    }

    pub(crate) fn close_picker(&mut self) -> bool {
        self.picker.take().is_some()
    }

    fn set_draft(&mut self, text: TextSettings) -> bool {
        if text == self.draft {
            return false;
        }

        self.draft = text;
        self.preview = layout_preview(text);

        true
    }
}

/// Where the preview text sits inside the preview: the screen margin within
/// the preview's padding, like crosspoint.
pub(crate) fn preview_text_left(text: TextSettings) -> u32 {
    PREVIEW_PADDING + u32::from(text.margin().px())
}

fn preview_viewport(text: TextSettings) -> Option<Viewport> {
    let margin = u32::from(text.margin().px());

    Viewport::new(
        PREVIEW_WIDTH.saturating_sub(2 * margin),
        PREVIEW_TEXT_HEIGHT,
    )
}

/// The preview's first page, laid out by the reader like a book's.
fn layout_preview(text: TextSettings) -> Option<Page<'static>> {
    let path = ArchivePath::new("preview.xhtml").ok()?;
    let chapter = Chapter::parse(PREVIEW_XHTML, path).ok()?;
    let styles = ChapterStyles::defaults(&chapter);

    let fonts = reader_fonts().ok()?;
    let mut measurer = ReaderMeasurer::new(&fonts, Default::default());

    let pagination = paginate_chapter(
        &chapter,
        &styles,
        SpineIndex::ZERO,
        preview_viewport(text)?,
        text.reader_settings(Some(PREVIEW_LANGUAGE)),
        &mut measurer,
    )
    .ok()?;

    pagination.into_owned().pages().first().cloned()
}
