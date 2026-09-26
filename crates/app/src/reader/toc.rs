use alloc::{string::String, vec::Vec};

use inkpaper_epub::{Navigation, NavigationEntry, Package, SpineIndex};

/// Where a table of contents entry leads in the book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TocTarget {
    pub(crate) spine: SpineIndex,
    pub(crate) anchor: Option<String>,
}

/// One row of the flattened table of contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TocEntry {
    label: String,
    depth: u8,
    target: Option<TocTarget>,
}

impl TocEntry {
    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) const fn depth(&self) -> u8 {
        self.depth
    }

    pub(crate) fn target(&self) -> Option<&TocTarget> {
        self.target.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum TableOfContents {
    #[default]
    NotLoaded,
    Loading,
    Loaded(Vec<TocEntry>),
    Failed,
}

/// Flattens the navigation tree depth-first and resolves each target to a
/// spine entry. Targets outside the reading order have no target.
pub(crate) fn toc_entries(navigation: &Navigation, package: &Package) -> Vec<TocEntry> {
    let mut entries = Vec::new();

    push_entries(navigation.entries(), 0, package, &mut entries);

    entries
}

fn push_entries(
    navigation: &[NavigationEntry],
    depth: u8,
    package: &Package,
    entries: &mut Vec<TocEntry>,
) {
    for entry in navigation {
        let target = entry.target().and_then(|target| {
            let index = package.spine_index_for_path(target.path())?;

            Some(TocTarget {
                spine: SpineIndex::try_from_usize(index)?,
                anchor: target.fragment().map(String::from),
            })
        });

        entries.push(TocEntry {
            label: String::from(entry.label()),
            depth,
            target,
        });

        push_entries(entry.children(), depth.saturating_add(1), package, entries);
    }
}

/// The entry for the chapter at `spine`: the first entry pointing into it,
/// otherwise the last entry before it.
pub(crate) fn current_toc_index(entries: &[TocEntry], spine: SpineIndex) -> Option<usize> {
    let spine_of = |entry: &TocEntry| entry.target().map(|target| target.spine);

    entries
        .iter()
        .position(|entry| spine_of(entry) == Some(spine))
        .or_else(|| {
            entries
                .iter()
                .rposition(|entry| spine_of(entry).is_some_and(|entry_spine| entry_spine < spine))
        })
}
