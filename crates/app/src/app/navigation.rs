use alloc::{vec, vec::Vec};
use inkpaper_ui::prelude::*;

use super::{InkPaperApp, Screen};

// deeper histories drop their oldest entry above Home
const MAX_DEPTH: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Entry {
    /// The screen was pushed onto the stack.
    Opened,
    /// The screen became visible again after the screen above it closed.
    Returned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Exit {
    /// Another screen was opened on top of this one.
    Covered,
    /// The screen was removed from the stack.
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Back {
    /// The screen consumed the back action.
    Handled,
    /// The navigator should close the screen.
    Leave,
}

/// Side effects a screen runs as it enters and leaves the navigation stack.
pub(crate) trait ScreenLifecycle {
    fn enter(&self, _app: &mut InkPaperApp, _entry: Entry) {}

    fn exit(&self, _app: &mut InkPaperApp, _exit: Exit) {}

    fn back(&self, _app: &mut InkPaperApp, _cx: &mut Context<'_, InkPaperApp>) -> Back {
        Back::Leave
    }
}

/// Screens the user can go back through. Home is always at the bottom.
#[derive(Debug)]
pub(super) struct NavigationStack {
    screens: Vec<Screen>,
}

impl Default for NavigationStack {
    fn default() -> Self {
        Self {
            screens: vec![Screen::Home],
        }
    }
}

impl NavigationStack {
    fn current(&self) -> Screen {
        *self
            .screens
            .last()
            .expect("navigation stack always keeps Home")
    }

    fn push(&mut self, screen: Screen) -> bool {
        if self.current() == screen {
            return false;
        }

        if self.screens.len() == MAX_DEPTH {
            self.screens.remove(1);
        }

        self.screens.push(screen);

        true
    }

    fn pop(&mut self) -> Option<Screen> {
        if self.screens.len() == 1 {
            return None;
        }

        self.screens.pop()
    }
}

impl InkPaperApp {
    pub(super) fn screen(&self) -> Screen {
        self.navigation.current()
    }

    pub(super) fn open_screen(&mut self, screen: Screen, cx: &mut Context<'_, Self>) {
        let covered = self.screen();

        if !self.navigation.push(screen) {
            return;
        }

        covered.route().exit(self, Exit::Covered);
        screen.route().enter(self, Entry::Opened);

        cx.notify();
    }

    pub(super) fn navigate_back(&mut self, cx: &mut Context<'_, Self>) {
        if self.screen().route().back(self, cx) == Back::Handled {
            return;
        }

        let Some(closed) = self.navigation.pop() else {
            return;
        };

        closed.route().exit(self, Exit::Closed);
        self.screen().route().enter(self, Entry::Returned);

        cx.notify();
    }

    pub fn navigate_home(&mut self, cx: &mut Context<'_, Self>) {
        let mut changed = false;

        while let Some(closed) = self.navigation.pop() {
            closed.route().exit(self, Exit::Closed);
            changed = true;
        }

        if !changed {
            return;
        }

        Screen::Home.route().enter(self, Entry::Returned);

        cx.notify();
    }

    pub(super) fn activate_back(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.navigate_back(cx);
    }

    pub(super) fn show_browse_files(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.open_screen(Screen::BrowseFiles, cx);
    }

    pub(super) fn show_recent_books(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.open_screen(Screen::RecentBooks, cx);
    }

    pub(super) fn show_file_transfer(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.open_screen(Screen::FileTransfer, cx);
    }

    pub(super) fn show_settings(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.open_screen(Screen::Settings, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pop_never_removes_home() {
        let mut stack = NavigationStack::default();

        assert_eq!(stack.pop(), None);

        assert!(stack.push(Screen::BrowseFiles));
        assert!(stack.push(Screen::Reader));
        assert!(!stack.push(Screen::Reader));

        assert_eq!(stack.pop(), Some(Screen::Reader));
        assert_eq!(stack.pop(), Some(Screen::BrowseFiles));
        assert_eq!(stack.pop(), None);
        assert_eq!(stack.current(), Screen::Home);
    }

    #[test]
    fn depth_limit_drops_oldest_screen_above_home() {
        let mut stack = NavigationStack::default();
        let screens = [Screen::BrowseFiles, Screen::Reader];

        for index in 0..MAX_DEPTH {
            stack.push(screens[index % 2]);
        }

        assert_eq!(stack.screens.len(), MAX_DEPTH);
        assert_eq!(stack.screens[0], Screen::Home);
        assert_eq!(stack.current(), screens[(MAX_DEPTH - 1) % 2]);
    }
}
