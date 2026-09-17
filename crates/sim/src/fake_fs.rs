use std::{
    fs,
    path::{Path, PathBuf},
};

use inkpaper_app::{BrowseEntry, BrowseListing};

pub(super) fn simulator_listing(path: &str) -> Option<BrowseListing> {
    let entries = match path {
        "/" => vec![
            BrowseEntry::directory("Books"),
            BrowseEntry::directory("Documents"),
            BrowseEntry::directory("Fixtures"),
            BrowseEntry::directory("Read"),
            BrowseEntry::file("A Fire Upon the Deep.epub"),
            BrowseEntry::file("Blindsight.epub"),
            BrowseEntry::file("Children of Time.epub"),
            BrowseEntry::file("Dune.epub"),
            BrowseEntry::file("Foundation.epub"),
            BrowseEntry::file("Hyperion.epub"),
            BrowseEntry::file("Neuromancer.epub"),
            BrowseEntry::file("Project Hail Mary.epub"),
            BrowseEntry::file("Snow Crash.epub"),
            BrowseEntry::file("The Dispossessed.epub"),
            BrowseEntry::file("The Left Hand of Darkness.epub"),
            BrowseEntry::file("The Three-Body Problem.epub"),
        ],

        "/Books" => vec![
            BrowseEntry::directory("Sci-Fi"),
            BrowseEntry::file("Neuromancer.epub"),
            BrowseEntry::file("The Dispossessed.epub"),
            BrowseEntry::file("Hyperion.epub"),
        ],

        "/Books/Sci-Fi" => vec![
            BrowseEntry::file("Children of Time.epub"),
            BrowseEntry::file("The Left Hand of Darkness.epub"),
            BrowseEntry::file("Foundation.epub"),
        ],

        "/Documents" => vec![
            BrowseEntry::file("Distributed Systems.pdf"),
            BrowseEntry::file("Cloud Computing.pdf"),
        ],

        "/Fixtures" => fixture_entries(),

        "/Read" => Vec::new(),

        _ => return None,
    };

    Some(BrowseListing::new(path, entries))
}

fn fixture_entries() -> Vec<BrowseEntry> {
    let Ok(entries) = fs::read_dir(fixtures_directory()) else {
        return Vec::new();
    };

    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;

            if !file_type.is_file() {
                return None;
            }

            let path = entry.path();

            if path.extension().and_then(|extension| extension.to_str()) != Some("epub") {
                return None;
            }

            entry.file_name().into_string().ok()
        })
        .collect();

    names.sort();

    names.into_iter().map(BrowseEntry::file).collect()
}

fn fixtures_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}
