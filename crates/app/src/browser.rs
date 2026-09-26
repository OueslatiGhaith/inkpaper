use core::cmp::Ordering;

use alloc::{borrow::ToOwned, string::String, vec::Vec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseEntryKind {
    Directory,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseEntry {
    name: String,
    kind: BrowseEntryKind,
}

impl BrowseEntry {
    pub fn directory(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: BrowseEntryKind::Directory,
        }
    }

    pub fn file(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: BrowseEntryKind::File,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> BrowseEntryKind {
        self.kind
    }

    pub(crate) fn display_name(&self) -> &str {
        match self.kind {
            BrowseEntryKind::Directory => &self.name,
            BrowseEntryKind::File => split_name_extension(&self.name).0,
        }
    }

    pub(crate) fn extension(&self) -> &str {
        match self.kind {
            BrowseEntryKind::Directory => "",
            BrowseEntryKind::File => split_name_extension(&self.name).1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrowseFile {
    path: String,
    name: String,
}

impl BrowseFile {
    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn title(&self) -> &str {
        split_name_extension(&self.name).0
    }

    pub(crate) fn is_epub(&self) -> bool {
        split_name_extension(&self.name)
            .1
            .eq_ignore_ascii_case(".epub")
    }

    pub(crate) fn into_reader_parts(self) -> (String, String) {
        let title = split_name_extension(&self.name).0.to_owned();

        (self.path, title)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseListing {
    path: String,
    entries: Vec<BrowseEntry>,
}

impl BrowseListing {
    pub fn new(path: impl Into<String>, mut entries: Vec<BrowseEntry>) -> Self {
        entries.sort_unstable_by(|left, right| match (left.kind, right.kind) {
            (BrowseEntryKind::Directory, BrowseEntryKind::File) => Ordering::Less,
            (BrowseEntryKind::File, BrowseEntryKind::Directory) => Ordering::Greater,
            _ => left.name.cmp(&right.name),
        });

        Self {
            path: path.into(),
            entries,
        }
    }

    pub(crate) fn into_parts(self) -> (String, Vec<BrowseEntry>) {
        (self.path, self.entries)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BrowseRequest {
    ListDirectory(String),
}

#[derive(Debug)]
pub(crate) struct BrowserState {
    path: String,
    entries: Vec<BrowseEntry>,
    pending: Option<BrowseRequest>,
    error: bool,
    revision: u64,
    return_to: Option<usize>,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            path: "/".to_owned(),
            entries: Vec::new(),
            pending: None,
            error: false,
            revision: 0,
            return_to: None,
        }
    }
}

impl BrowserState {
    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn title(&self) -> &str {
        if self.path == "/" {
            return "SD Card";
        }

        self.path
            .rsplit('/')
            .find(|component| !component.is_empty())
            .unwrap_or("SD Card")
    }

    pub(crate) fn entries(&self) -> &[BrowseEntry] {
        &self.entries
    }

    pub(crate) fn error(&self) -> bool {
        self.error
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// after going up a level, the entry for the folder that was just left
    pub(crate) fn return_to(&self) -> Option<usize> {
        self.return_to
    }

    pub(crate) fn request_current_directory(&mut self) {
        self.pending = Some(BrowseRequest::ListDirectory(self.path.clone()));
        self.error = false;
    }

    pub(crate) fn request_entry(&mut self, index: usize) -> bool {
        let Some(entry) = self.entries.get(index) else {
            return false;
        };

        if entry.kind != BrowseEntryKind::Directory {
            return false;
        }

        let path = join_path(&self.path, &entry.name);

        self.pending = Some(BrowseRequest::ListDirectory(path));
        self.error = false;

        true
    }

    pub(crate) fn request_parent(&mut self) -> bool {
        if self.path == "/" {
            return false;
        }

        self.pending = Some(BrowseRequest::ListDirectory(parent_path(&self.path)));
        self.error = false;

        true
    }

    pub(crate) fn take_request(&mut self) -> Option<BrowseRequest> {
        self.pending.take()
    }

    pub(crate) fn apply_listing(&mut self, listing: BrowseListing) {
        let (path, entries) = listing.into_parts();

        self.return_to = child_name(&path, &self.path).and_then(|name| {
            entries
                .iter()
                .position(|entry| entry.kind == BrowseEntryKind::Directory && entry.name == name)
        });
        self.path = path;
        self.entries = entries;
        self.error = false;
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn apply_error(&mut self) {
        self.error = true;
        self.return_to = None;
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn file_at(&self, index: usize) -> Option<BrowseFile> {
        let entry = self.entries.get(index)?;

        if entry.kind != BrowseEntryKind::File {
            return None;
        }

        Some(BrowseFile {
            path: join_path(&self.path, &entry.name),
            name: entry.name.clone(),
        })
    }
}

fn split_name_extension(name: &str) -> (&str, &str) {
    let Some(index) = name.rfind('.') else {
        return (name, "");
    };

    if index == 0 || index + 1 >= name.len() {
        return (name, "");
    }

    name.split_at(index)
}

fn join_path(parent: &str, child: &str) -> String {
    if parent == "/" {
        let mut path = String::with_capacity(child.len() + 1);
        path.push('/');
        path.push_str(child);
        return path;
    }

    let mut path = String::with_capacity(parent.len() + child.len() + 1);
    path.push_str(parent);
    path.push('/');
    path.push_str(child);
    path
}

/// the last component of `child` when it lives directly inside `parent`
fn child_name<'a>(parent: &str, child: &'a str) -> Option<&'a str> {
    if child == parent || parent_path(child) != parent {
        return None;
    }

    child.trim_end_matches('/').rsplit('/').next()
}

fn parent_path(path: &str) -> String {
    let path = path.trim_end_matches('/');

    let Some(index) = path.rfind('/') else {
        return "/".to_owned();
    };

    if index == 0 {
        "/".to_owned()
    } else {
        path[..index].to_owned()
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    fn list(path: &str, entries: Vec<BrowseEntry>) -> BrowseListing {
        BrowseListing::new(path, entries)
    }

    #[test]
    fn going_up_remembers_the_folder_that_was_left() {
        let mut browser = BrowserState::default();
        browser.apply_listing(list("/books/sci-fi", vec![BrowseEntry::file("a.epub")]));

        browser.apply_listing(list(
            "/books",
            vec![
                BrowseEntry::file("sci-fi.epub"),
                BrowseEntry::directory("fantasy"),
                BrowseEntry::directory("sci-fi"),
            ],
        ));

        // directories sort first: fantasy, sci-fi, then the file
        assert_eq!(browser.return_to(), Some(1));
    }

    #[test]
    fn going_up_to_root_remembers_the_folder_that_was_left() {
        let mut browser = BrowserState::default();
        browser.apply_listing(list("/books", vec![]));

        browser.apply_listing(list(
            "/",
            vec![BrowseEntry::directory("art"), BrowseEntry::directory("books")],
        ));

        assert_eq!(browser.return_to(), Some(1));
    }

    #[test]
    fn opening_or_reloading_a_folder_starts_at_the_top() {
        let mut browser = BrowserState::default();
        browser.apply_listing(list("/", vec![BrowseEntry::directory("books")]));
        browser.apply_listing(list("/books", vec![BrowseEntry::directory("books")]));
        assert_eq!(browser.return_to(), None);

        browser.apply_listing(list("/books", vec![BrowseEntry::directory("books")]));
        assert_eq!(browser.return_to(), None);
    }
}
