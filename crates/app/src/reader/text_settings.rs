use inkpaper_reader::Viewport;

/// The screen the reader page sits on.
const SCREEN_WIDTH: u32 = 480;

/// The page ends above the status bar.
const PAGE_BOTTOM: u32 = 720;

/// The page starts this far below the margin, clear of the screen's top edge.
const PAGE_TOP_EXTRA: u32 = 15;

/// The default line height, in percent of the font size. Libron's own line
/// height is a tight 1 em, so lines get a fixed multiple of the font size instead.
pub(crate) const DEFAULT_LINE_HEIGHT_PERCENT: u16 = 135;

/// The space around the page, like crosspoint's screen margin: 5 to 40 px in
/// steps of 5. It sets the left and right edges, and the top with a little extra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScreenMargin(u8);

impl ScreenMargin {
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
