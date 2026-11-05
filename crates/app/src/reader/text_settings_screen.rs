use alloc::{string::String, vec::Vec};

use inkpaper_epub::{ArchivePath, Chapter, ChapterStyles, SpineIndex};
use inkpaper_reader::{Page, Viewport, paginate_chapter};

use super::{
    ReaderFont, TextSetting, TextSettings,
    measurer::{ReaderFonts, ReaderMeasurer},
    text_settings::font_sizes,
};
use crate::{
    fonts::{CardFont, card_for},
    typography::READER_FONT_NAME,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TextSettingsRow {
    /// a font family: the built-in one, then the card's
    Font {
        font: ReaderFont,
        name: String,
    },
    Size(u16),
    Setting(TextSetting),
}

/// What the Text Settings screen edits: the settings being edited, and the font
/// families and card font it offers. The screen's tab, picker and preview are its
/// own, in its view.
#[derive(Debug, Default)]
pub(crate) struct TextSettingsState {
    draft: TextSettings,
    /// the font families on the card, by name
    families: Vec<String>,
    /// the card font read into memory, if any
    card: Option<CardFont>,
}

impl TextSettingsState {
    /// Starts editing `text`.
    pub(crate) fn open(&mut self, text: TextSettings) {
        self.draft = text;
    }

    pub(crate) const fn draft(&self) -> TextSettings {
        self.draft
    }

    /// Returns whether the settings changed.
    pub(crate) fn set_draft(&mut self, text: TextSettings) -> bool {
        if text == self.draft {
            return false;
        }

        self.draft = text;

        true
    }

    /// Lists the card's font families after the built-in one. Returns
    /// whether they changed.
    pub(crate) fn set_families(&mut self, families: Vec<String>) -> bool {
        if self.families == families {
            return false;
        }

        self.families = families;

        true
    }

    /// The name `font` goes by in the settings.
    pub(crate) fn font_name(&self, font: ReaderFont) -> String {
        match font {
            ReaderFont::Card(index) => self
                .families
                .get(usize::from(index))
                .map_or(READER_FONT_NAME, String::as_str)
                .into(),
            ReaderFont::BuiltIn => String::from(READER_FONT_NAME),
        }
    }

    pub(crate) fn card_font(&self) -> Option<&CardFont> {
        self.card.as_ref()
    }

    /// Keeps the card font read into memory.
    pub(crate) fn set_card_font(&mut self, card: Option<CardFont>) {
        self.card = card;
    }

    /// The rows of `tab`.
    pub(crate) fn rows(&self, tab: TextSettingsTab) -> Vec<TextSettingsRow> {
        match tab {
            TextSettingsTab::Font => core::iter::once(ReaderFont::BuiltIn)
                .chain(
                    (0..self.families.len())
                        .filter_map(|index| u8::try_from(index).ok().map(ReaderFont::Card)),
                )
                .map(|font| TextSettingsRow::Font {
                    font,
                    name: self.font_name(font),
                })
                .collect(),
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

/// The preview's first page, laid out by the reader like a book's, in the
/// card font when it is the one chosen and has loaded.
pub(crate) fn layout_preview(text: TextSettings, card: Option<&CardFont>) -> Option<Page<'static>> {
    let path = ArchivePath::new("preview.xhtml").ok()?;
    let chapter = Chapter::parse(PREVIEW_XHTML, path).ok()?;
    let styles = ChapterStyles::defaults(&chapter);

    let fonts = ReaderFonts::new(card_for(text.font(), card)).ok()?;
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
