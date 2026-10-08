use inkpaper_ui::prelude::*;

use crate::{InkPaperApp, app::SideButton};

/// How far a touch must travel vertically to turn a page.
const SWIPE_DISTANCE: i32 = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageTurn {
    Previous,
    Next,
}

impl From<SideButton> for PageTurn {
    fn from(button: SideButton) -> Self {
        match button {
            SideButton::Previous => Self::Previous,
            SideButton::Next => Self::Next,
        }
    }
}

/// Which page of a list is shown. Lists turn pages rather than scroll, since
/// an e-ink panel redraws every step of a scroll.
///
/// It holds no lengths, so the list's owner passes its length and how many
/// rows its screen fits on a page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Pager {
    /// the row at the top of the page shown
    top: usize,
}

impl Pager {
    /// Shows the page holding row `index`.
    pub(crate) const fn showing(index: usize) -> Self {
        Self { top: index }
    }

    pub(crate) fn page(self, len: usize, rows_per_page: usize) -> usize {
        (self.top / rows_per_page).min(Self::page_count(len, rows_per_page) - 1)
    }

    pub(crate) fn page_count(len: usize, rows_per_page: usize) -> usize {
        len.div_ceil(rows_per_page).max(1)
    }

    /// Returns false at either end of the list.
    pub(crate) fn turn(&mut self, turn: PageTurn, len: usize, rows_per_page: usize) -> bool {
        let page = self.page(len, rows_per_page);
        let next = match turn {
            PageTurn::Previous => page.checked_sub(1),
            PageTurn::Next => {
                Some(page + 1).filter(|&next| next < Self::page_count(len, rows_per_page))
            }
        };

        let Some(next) = next else {
            return false;
        };

        self.top = next * rows_per_page;

        true
    }
}

/// Turns a page for a side button.
pub(crate) fn turn_page_by_button(
    app: &mut InkPaperApp,
    button: SideButton,
    cx: &mut Context<'_, InkPaperApp>,
    turn_page: impl FnOnce(&mut InkPaperApp, PageTurn) -> bool,
) {
    if turn_page(app, button.into()) {
        cx.notify();
    }
}

/// Turns a page for a vertical swipe: up for the next, down for the previous.
/// Returns whether the drag was taken; once a swipe turns a page, the rest of
/// the touch turns nothing more.
pub(crate) fn turn_page_by_swipe(
    app: &mut InkPaperApp,
    origin: Point,
    position: Point,
    cx: &mut Context<'_, InkPaperApp>,
    turn_page: impl FnOnce(&mut InkPaperApp, PageTurn) -> bool,
) -> bool {
    let dx = position.x.get() - origin.x.get();
    let dy = position.y.get() - origin.y.get();

    if dy.abs() < SWIPE_DISTANCE || dy.abs() <= dx.abs() {
        return false;
    }

    let turn = if dy < 0 {
        PageTurn::Next
    } else {
        PageTurn::Previous
    };

    if turn_page(app, turn) {
        cx.notify();
    }

    app.capture_pointer();

    true
}
