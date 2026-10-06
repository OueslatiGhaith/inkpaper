use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use inkpaper_reader::{ReaderSettings, TextAlign, Viewport};

use super::{
    READER_BLOCK_SPACING, READER_FONT_SIZE_DEFAULT, READER_FONT_SIZE_MAX, READER_FONT_SIZE_MIN,
    READER_FONT_SIZE_STEP,
};

/// The screen the reader page sits on.
const SCREEN_WIDTH: u32 = 480;

/// The page ends above the status bar.
const PAGE_BOTTOM: u32 = 720;

/// The page starts this far below the margin, clear of the screen's top edge.
const PAGE_TOP_EXTRA: u32 = 15;

/// Line spacing steps, like crosspoint's. Each is a fixed multiple of the font
/// size: Libron's own line height is a tight 1 em, so scaling it like crosspoint
/// does would leave every step too tight.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineSpacing {
    Tight,
    #[default]
    Normal,
    Wide,
    ExtraWide,
}

impl LineSpacing {
    pub(crate) const ALL: [Self; 4] = [Self::Tight, Self::Normal, Self::Wide, Self::ExtraWide];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Tight => "Tight",
            Self::Normal => "Normal",
            Self::Wide => "Wide",
            Self::ExtraWide => "Extra Wide",
        }
    }

    /// The line height, in percent of the font size.
    pub(crate) const fn percent(self) -> u16 {
        match self {
            Self::Tight => 120,
            Self::Normal => 135,
            Self::Wide => 150,
            Self::ExtraWide => 170,
        }
    }

    /// The stored form, in order from tightest.
    pub(crate) const fn index(self) -> u8 {
        match self {
            Self::Tight => 0,
            Self::Normal => 1,
            Self::Wide => 2,
            Self::ExtraWide => 3,
        }
    }

    pub(crate) const fn from_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Self::Tight),
            1 => Some(Self::Normal),
            2 => Some(Self::Wide),
            3 => Some(Self::ExtraWide),
            _ => None,
        }
    }
}

/// How paragraphs line up, like crosspoint's paragraph alignment. Every choice
/// but Book's Style overrides the book's CSS; headings always keep the book's.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParagraphAlignment {
    #[default]
    Justify,
    Left,
    Center,
    Right,
    /// the book's CSS, justified where it sets no alignment
    BookStyle,
}

impl ParagraphAlignment {
    pub(crate) const ALL: [Self; 5] = [
        Self::Justify,
        Self::Left,
        Self::Center,
        Self::Right,
        Self::BookStyle,
    ];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Justify => "Justify",
            Self::Left => "Left",
            Self::Center => "Center",
            Self::Right => "Right",
            Self::BookStyle => "Book's Style",
        }
    }

    /// The stored form, in crosspoint's order.
    pub(crate) const fn index(self) -> u8 {
        match self {
            Self::Justify => 0,
            Self::Left => 1,
            Self::Center => 2,
            Self::Right => 3,
            Self::BookStyle => 4,
        }
    }

    pub(crate) const fn from_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Self::Justify),
            1 => Some(Self::Left),
            2 => Some(Self::Center),
            3 => Some(Self::Right),
            4 => Some(Self::BookStyle),
            _ => None,
        }
    }

    fn apply(self, settings: ReaderSettings) -> ReaderSettings {
        let align = match self {
            Self::Justify => TextAlign::Justify,
            Self::Left => TextAlign::Left,
            Self::Center => TextAlign::Center,
            Self::Right => TextAlign::Right,
            Self::BookStyle => return settings.with_undeclared_paragraph_align(TextAlign::Justify),
        };

        settings.with_paragraph_align(align)
    }
}

/// The space around the page, like crosspoint's screen margin: 5 to 40 px in
/// steps of 5. It sets the left and right edges, and the top with a little extra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScreenMargin(u8);

impl ScreenMargin {
    pub(crate) const MIN: u8 = 5;
    pub(crate) const MAX: u8 = 40;
    pub(crate) const STEP: u8 = 5;

    pub(crate) const fn new(px: u8) -> Option<Self> {
        if px < Self::MIN || px > Self::MAX || !px.is_multiple_of(Self::STEP) {
            return None;
        }

        Some(Self(px))
    }

    pub(crate) const fn px(self) -> u8 {
        self.0
    }

    /// Where the page sits on the screen.
    pub(crate) const fn page_bounds(self) -> PageBounds {
        let margin = self.0 as u32;
        let top = margin + PAGE_TOP_EXTRA;

        PageBounds {
            left: margin,
            top,
            width: SCREEN_WIDTH - 2 * margin,
            height: PAGE_BOTTOM - top,
        }
    }
}

impl Default for ScreenMargin {
    /// today's page: 20 px at the sides and 35 at the top
    fn default() -> Self {
        Self(20)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageBounds {
    pub(crate) left: u32,
    pub(crate) top: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl PageBounds {
    pub(crate) fn viewport(self) -> Viewport {
        Viewport::new(self.width, self.height).expect("every screen margin leaves a page")
    }
}

/// Like crosspoint: indentation is off or 1 to 5 spaces, 2 by default.
const PARAGRAPH_INDENT_MAX: u8 = 5;
const PARAGRAPH_INDENT_DEFAULT: u8 = 2;

/// The reader settings that lay out text: changing any of them repaginates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextSettings {
    font_size: u16,
    line_spacing: LineSpacing,
    margin: ScreenMargin,
    alignment: ParagraphAlignment,
    /// first-line indent of paragraphs, in spaces; 0 turns it off
    paragraph_indent: u8,
    /// half a line after every paragraph, like crosspoint's extra spacing
    paragraph_spacing: bool,
    /// whether the book's own CSS applies, like crosspoint's embedded style
    embedded_style: bool,
}

impl TextSettings {
    pub(crate) const fn new(
        font_size: u16,
        line_spacing: LineSpacing,
        margin: ScreenMargin,
    ) -> Option<Self> {
        if font_size < READER_FONT_SIZE_MIN || font_size > READER_FONT_SIZE_MAX {
            return None;
        }

        Some(Self {
            font_size,
            line_spacing,
            margin,
            alignment: ParagraphAlignment::Justify,
            paragraph_indent: PARAGRAPH_INDENT_DEFAULT,
            paragraph_spacing: true,
            embedded_style: true,
        })
    }

    pub(crate) const fn font_size(self) -> u16 {
        self.font_size
    }

    pub(crate) const fn line_spacing(self) -> LineSpacing {
        self.line_spacing
    }

    pub(crate) const fn margin(self) -> ScreenMargin {
        self.margin
    }

    pub(crate) const fn alignment(self) -> ParagraphAlignment {
        self.alignment
    }

    pub(crate) const fn with_alignment(self, alignment: ParagraphAlignment) -> Self {
        Self { alignment, ..self }
    }

    pub(crate) const fn paragraph_indent(self) -> u8 {
        self.paragraph_indent
    }

    pub(crate) const fn paragraph_spacing(self) -> bool {
        self.paragraph_spacing
    }

    pub(crate) const fn embedded_style(self) -> bool {
        self.embedded_style
    }

    pub(crate) const fn with_embedded_style(self, embedded_style: bool) -> Self {
        Self {
            embedded_style,
            ..self
        }
    }

    pub(crate) const fn with_font_size(self, font_size: u16) -> Option<Self> {
        if font_size < READER_FONT_SIZE_MIN || font_size > READER_FONT_SIZE_MAX {
            return None;
        }

        Some(Self { font_size, ..self })
    }

    /// `spaces` from 0, which turns indentation off, to 5
    pub(crate) const fn with_paragraph_indent(self, spaces: u8) -> Option<Self> {
        if spaces > PARAGRAPH_INDENT_MAX {
            return None;
        }

        Some(Self {
            paragraph_indent: spaces,
            ..self
        })
    }

    pub(crate) const fn with_paragraph_spacing(self, paragraph_spacing: bool) -> Self {
        Self {
            paragraph_spacing,
            ..self
        }
    }

    pub(crate) fn reader_settings(self) -> ReaderSettings {
        let settings = ReaderSettings::new(self.font_size, READER_BLOCK_SPACING)
            .and_then(|settings| settings.with_line_height_percent(self.line_spacing.percent()))
            .expect("text settings only hold valid font sizes")
            .with_paragraph_indent(self.paragraph_indent)
            .with_extra_block_spacing(self.paragraph_spacing);

        self.alignment.apply(settings)
    }

    pub(crate) fn viewport(self) -> Viewport {
        self.margin.page_bounds().viewport()
    }
}

impl Default for TextSettings {
    fn default() -> Self {
        Self {
            font_size: READER_FONT_SIZE_DEFAULT,
            line_spacing: LineSpacing::default(),
            margin: ScreenMargin::default(),
            alignment: ParagraphAlignment::default(),
            paragraph_indent: PARAGRAPH_INDENT_DEFAULT,
            paragraph_spacing: true,
            embedded_style: true,
        }
    }
}

/// A row of the reader's Text panel, in the order they are listed. Like
/// crosspoint's, each opens a picker of its values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextSetting {
    FontSize,
    LineSpacing,
    ParagraphAlignment,
    ScreenMargin,
    ParagraphIndent,
    ParagraphSpacing,
    EmbeddedStyle,
}

impl TextSetting {
    pub(crate) const ALL: [Self; 7] = [
        Self::FontSize,
        Self::LineSpacing,
        Self::ParagraphAlignment,
        Self::ScreenMargin,
        Self::ParagraphIndent,
        Self::ParagraphSpacing,
        Self::EmbeddedStyle,
    ];

    pub(crate) fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::FontSize => "Font Size",
            Self::LineSpacing => "Line Spacing",
            Self::ParagraphAlignment => "Paragraph Alignment",
            Self::ScreenMargin => "Screen Margin",
            Self::ParagraphIndent => "Paragraph Indentation",
            Self::ParagraphSpacing => "Extra Paragraph Spacing",
            Self::EmbeddedStyle => "Embedded Style",
        }
    }

    /// Whether a tap flips the setting instead of opening a picker, like
    /// crosspoint's checkbox rows.
    pub(crate) const fn is_toggle(self) -> bool {
        matches!(self, Self::ParagraphSpacing | Self::EmbeddedStyle)
    }

    /// The setting's value in `text`, as its row shows it.
    pub(crate) fn value(self, text: TextSettings) -> String {
        match self {
            Self::FontSize => text.font_size.to_string(),
            Self::LineSpacing => String::from(text.line_spacing.label()),
            Self::ParagraphAlignment => String::from(text.alignment.label()),
            Self::ScreenMargin => text.margin.px().to_string(),
            Self::ParagraphIndent => indent_label(text.paragraph_indent),
            Self::ParagraphSpacing => String::from(on_off(text.paragraph_spacing)),
            Self::EmbeddedStyle => String::from(on_off(text.embedded_style)),
        }
    }

    /// Every value the picker offers, in order.
    pub(crate) fn options(self) -> Vec<String> {
        match self {
            Self::FontSize => font_sizes().map(|size| format!("{size}")).collect(),
            Self::LineSpacing => LineSpacing::ALL
                .iter()
                .map(|spacing| String::from(spacing.label()))
                .collect(),
            Self::ParagraphAlignment => ParagraphAlignment::ALL
                .iter()
                .map(|alignment| String::from(alignment.label()))
                .collect(),
            Self::ScreenMargin => margins().map(|margin| format!("{}", margin.px())).collect(),
            Self::ParagraphIndent => (0..=PARAGRAPH_INDENT_MAX).map(indent_label).collect(),
            Self::ParagraphSpacing | Self::EmbeddedStyle => [false, true]
                .iter()
                .map(|&on| String::from(on_off(on)))
                .collect(),
        }
    }

    /// Which of [`Self::options`] `text` holds.
    pub(crate) fn selected(self, text: TextSettings) -> Option<usize> {
        match self {
            Self::FontSize => font_sizes().position(|size| size == text.font_size),
            Self::LineSpacing => LineSpacing::ALL
                .iter()
                .position(|&spacing| spacing == text.line_spacing),
            Self::ParagraphAlignment => ParagraphAlignment::ALL
                .iter()
                .position(|&alignment| alignment == text.alignment),
            Self::ScreenMargin => margins().position(|margin| margin == text.margin),
            Self::ParagraphIndent => Some(usize::from(text.paragraph_indent)),
            Self::ParagraphSpacing => Some(usize::from(text.paragraph_spacing)),
            Self::EmbeddedStyle => Some(usize::from(text.embedded_style)),
        }
    }

    /// `text` with this setting changed to option `index`.
    pub(crate) fn choose(self, text: TextSettings, index: usize) -> Option<TextSettings> {
        match self {
            Self::FontSize => text.with_font_size(font_sizes().nth(index)?),
            Self::LineSpacing => Some(TextSettings {
                line_spacing: *LineSpacing::ALL.get(index)?,
                ..text
            }),
            Self::ParagraphAlignment => {
                Some(text.with_alignment(*ParagraphAlignment::ALL.get(index)?))
            }
            Self::ScreenMargin => Some(TextSettings {
                margin: margins().nth(index)?,
                ..text
            }),
            Self::ParagraphIndent => text.with_paragraph_indent(u8::try_from(index).ok()?),
            Self::ParagraphSpacing => match index {
                0 => Some(text.with_paragraph_spacing(false)),
                1 => Some(text.with_paragraph_spacing(true)),
                _ => None,
            },
            Self::EmbeddedStyle => match index {
                0 => Some(text.with_embedded_style(false)),
                1 => Some(text.with_embedded_style(true)),
                _ => None,
            },
        }
    }

    /// `text` with a toggle row flipped.
    pub(crate) fn toggle(self, text: TextSettings) -> Option<TextSettings> {
        match self {
            Self::ParagraphSpacing => Some(text.with_paragraph_spacing(!text.paragraph_spacing)),
            Self::EmbeddedStyle => Some(text.with_embedded_style(!text.embedded_style)),
            _ => None,
        }
    }
}

fn font_sizes() -> impl Iterator<Item = u16> {
    (READER_FONT_SIZE_MIN..=READER_FONT_SIZE_MAX).step_by(usize::from(READER_FONT_SIZE_STEP))
}

fn margins() -> impl Iterator<Item = ScreenMargin> {
    (ScreenMargin::MIN..=ScreenMargin::MAX)
        .step_by(usize::from(ScreenMargin::STEP))
        .filter_map(ScreenMargin::new)
}

fn indent_label(spaces: u8) -> String {
    if spaces == 0 {
        String::from(on_off(false))
    } else {
        spaces.to_string()
    }
}

const fn on_off(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}
