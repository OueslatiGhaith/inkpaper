use alloc::{string::String, vec::Vec};
use core::mem;
use inkpaper_epub::LinkTarget;
use inkpaper_reader::{Page, PageItem, Rect};

use super::{JumpTarget, ReaderState};

const MAX_LINK_RETURNS: usize = 3;

impl ReaderState {
    /// The internal links on the page, each with its text's bounds on the
    /// page. A link broken across lines appears once per piece.
    pub(crate) fn page_links(&self) -> Vec<(Rect, &LinkTarget)> {
        let Some(page) = self.page() else {
            return Vec::new();
        };

        page.items()
            .iter()
            .filter_map(|item| match item {
                PageItem::Text(text) => text
                    .link()
                    .filter(|link| !link.is_external())
                    .map(|link| (text.bounds(), link)),
                PageItem::Image(_) => None,
            })
            .collect()
    }

    /// The page's links once each, as Links and footnotes lists them: the
    /// [`Self::page_links`] index of each link's first piece, and its text.
    /// The pieces of a link broken across lines read as one.
    pub(crate) fn page_link_entries(&self) -> Vec<(usize, String)> {
        let Some(page) = self.page() else {
            return Vec::new();
        };

        let mut entries: Vec<(usize, String, &LinkTarget)> = Vec::new();
        let mut index = 0;
        // the last piece, if the item before this one was a piece of a link
        let mut previous: Option<(&LinkTarget, u32, bool)> = None;

        for item in page.items() {
            let piece = match item {
                PageItem::Text(text) => text
                    .link()
                    .filter(|link| !link.is_external())
                    .map(|link| (text, link)),
                PageItem::Image(_) => None,
            };

            let Some((text, link)) = piece else {
                previous = None;
                continue;
            };

            let continues = previous.filter(|(target, ..)| *target == link);

            match (continues, entries.last_mut()) {
                (Some((_, y, hyphenated)), Some((_, label, _))) => {
                    if text.bounds().y() != y {
                        if hyphenated {
                            label.pop();
                        } else {
                            label.push(' ');
                        }
                    }

                    label.push_str(text.text());
                }

                _ => entries.push((index, String::from(text.text()), link)),
            }

            previous = Some((link, text.bounds().y(), text.hyphenated()));
            index += 1;
        }

        entries
            .into_iter()
            .map(|(index, label, _)| (index, String::from(label.trim())))
            .collect()
    }

    /// Opens Links and footnotes from the drawer: with one link it follows
    /// it, like crosspoint, and with several it lists them.
    pub(crate) fn open_links(&mut self) -> bool {
        if !self.chrome.menu_open {
            return false;
        }

        match self.page_link_entries().as_slice() {
            [] => false,

            &[(index, _)] => {
                self.close_menu();
                self.follow_link(index);
                true
            }

            _ => {
                self.chrome.link_picker = true;
                true
            }
        }
    }

    /// The open Links and footnotes picker's options.
    pub(crate) fn link_picker(&self) -> Option<Vec<String>> {
        self.chrome.link_picker.then(|| {
            self.page_link_entries()
                .into_iter()
                .map(|(_, label)| label)
                .collect()
        })
    }

    /// Follows entry `index` of the picker, closing it and the drawer.
    pub(crate) fn choose_link(&mut self, index: usize) -> bool {
        if !self.chrome.link_picker {
            return false;
        }

        let link = self.page_link_entries().get(index).map(|&(link, _)| link);

        self.close_menu();

        if let Some(link) = link {
            self.follow_link(link);
        }

        true
    }

    pub(crate) fn close_link_picker(&mut self) -> bool {
        mem::take(&mut self.chrome.link_picker)
    }

    /// Follows link `index` of [`Self::page_links`], saving this page to come
    /// back to.
    pub(crate) fn follow_link(&mut self, index: usize) -> bool {
        let Some(LinkTarget::Internal { path, fragment }) = self
            .page_links()
            .get(index)
            .map(|(_, link)| (*link).clone())
        else {
            return false;
        };

        let Some(position) = self.page().map(Page::position) else {
            return false;
        };

        let target = JumpTarget::Link {
            resource: path,
            anchor: fragment,
        };

        if !self.jump(target) {
            return false;
        }

        if self.link_returns.len() == MAX_LINK_RETURNS {
            self.link_returns.remove(0);
        }

        self.link_returns.push(position);

        true
    }

    /// Goes back to where the latest link was followed from. `false` with
    /// nowhere to go back to.
    pub(crate) fn return_from_link(&mut self) -> bool {
        let Some(&position) = self.link_returns.last() else {
            return false;
        };

        if !self.jump(JumpTarget::Position(position)) {
            // busy: keep the position, but the press is spent
            return true;
        }

        self.link_returns.pop();

        true
    }
}
