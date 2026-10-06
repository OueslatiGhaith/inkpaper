use inkpaper_reader::{ReaderSettings, Viewport};

use super::{
    READER_BLOCK_SPACING, READER_FONT_SIZE_DEFAULT, READER_FONT_SIZE_MAX, READER_FONT_SIZE_MIN,
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

/// The reader settings that lay out text: changing any of them repaginates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextSettings {
    font_size: u16,
    line_spacing: LineSpacing,
    margin: ScreenMargin,
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

    pub(crate) const fn with_font_size(self, font_size: u16) -> Option<Self> {
        Self::new(font_size, self.line_spacing, self.margin)
    }

    pub(crate) fn reader_settings(self) -> ReaderSettings {
        ReaderSettings::new(self.font_size, READER_BLOCK_SPACING)
            .and_then(|settings| settings.with_line_height_percent(self.line_spacing.percent()))
            .expect("text settings only hold valid font sizes")
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
        }
    }
}
