//! StarDict dictionaries on the card, found like crosspoint's
//! `DictionaryRegistry`: a folder each.

use alloc::{format, string::String, vec::Vec};

use crate::{AppPlatform, PlatformEntry, fonts::hidden};

/// Scanned in order, so a dictionary in `/.dictionaries` hides one of the same
/// name in `/dictionaries`.
const DICTIONARY_ROOTS: [&str; 2] = ["/.dictionaries", "/dictionaries"];

/// As many as the picker fits on the screen.
const MAX_DICTIONARIES: usize = 10;

/// A dictionary on the card, named after its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DictionaryFolder {
    name: String,
}

impl DictionaryFolder {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

/// The dictionary folders, sorted by name.
pub(crate) async fn find_dictionaries<P: AppPlatform>(platform: &mut P) -> Vec<DictionaryFolder> {
    let mut dictionaries: Vec<DictionaryFolder> = Vec::new();

    for root in DICTIONARY_ROOTS {
        // a card without the folder has no dictionaries in it
        let Ok(entries) = platform.list_directory(root).await else {
            continue;
        };

        for entry in entries {
            let name = entry.name();

            if !entry.is_directory()
                || hidden(name)
                || dictionaries.iter().any(|other| other.name == name)
            {
                continue;
            }

            let Ok(files) = platform.list_directory(&format!("{root}/{name}")).await else {
                continue;
            };

            if dictionary_stem(&files).is_some() {
                dictionaries.push(DictionaryFolder {
                    name: String::from(name),
                });
            }
        }
    }

    dictionaries.sort_by(|left, right| left.name.cmp(&right.name));
    dictionaries.truncate(MAX_DICTIONARIES);

    dictionaries
}

/// The name shared by the one dictionary among a folder's `files`: its
/// `.ifo`, its `.idx`, and its `.dict` or `.dict.dz`, in any case. Like
/// crosspoint, a folder of several dictionaries is skipped as ambiguous.
fn dictionary_stem(files: &[PlatformEntry]) -> Option<&str> {
    let files = || {
        files
            .iter()
            .filter(|entry| !entry.is_directory() && !hidden(entry.name()))
            .map(PlatformEntry::name)
    };

    let mut stems = files().filter_map(|name| strip_extension(name, ".idx"));

    let stem = stems.next()?;

    if stems.next().is_some() {
        return None;
    }

    let has = |extension: &str| files().any(|name| strip_extension(name, extension) == Some(stem));

    (has(".ifo") && (has(".dict") || has(".dict.dz"))).then_some(stem)
}

/// The name without `extension`, in any case.
fn strip_extension<'a>(name: &'a str, extension: &str) -> Option<&'a str> {
    let split = name.len().checked_sub(extension.len())?;
    let (stem, tail) = (name.get(..split)?, name.get(split..)?);

    (!stem.is_empty() && tail.eq_ignore_ascii_case(extension)).then_some(stem)
}

/// The dictionaries Settings offers, and whether its picker is open.
#[derive(Debug, Default)]
pub(crate) struct DictionarySettings {
    names: Vec<String>,
    picker_open: bool,
}

impl DictionarySettings {
    /// Returns whether the names changed.
    pub(crate) fn set_names(&mut self, names: Vec<String>) -> bool {
        if self.names == names {
            return false;
        }

        self.names = names;

        true
    }

    pub(crate) fn names(&self) -> &[String] {
        &self.names
    }

    pub(crate) const fn picker_open(&self) -> bool {
        self.picker_open
    }

    /// Returns whether the picker opened.
    pub(crate) fn open_picker(&mut self) -> bool {
        !core::mem::replace(&mut self.picker_open, true)
    }

    /// Returns whether the picker closed.
    pub(crate) fn close_picker(&mut self) -> bool {
        core::mem::take(&mut self.picker_open)
    }

    /// The picker's options: none, then each dictionary.
    pub(crate) fn options(&self) -> Vec<String> {
        core::iter::once(String::from("None"))
            .chain(self.names.iter().cloned())
            .collect()
    }
}
