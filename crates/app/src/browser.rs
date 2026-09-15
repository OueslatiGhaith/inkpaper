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
pub enum BrowseRequest {
    ListDirectory(String),
}

#[derive(Debug)]
pub(crate) struct BrowserState {
    path: String,
    entries: Vec<BrowseEntry>,
    pending: Option<BrowseRequest>,
    error: bool,
    revision: u64,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            path: "/".to_owned(),
            entries: Vec::new(),
            pending: None,
            error: false,
            revision: 0,
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

        self.path = path;
        self.entries = entries;
        self.error = false;
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn apply_error(&mut self) {
        self.error = true;
        self.revision = self.revision.wrapping_add(1);
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
