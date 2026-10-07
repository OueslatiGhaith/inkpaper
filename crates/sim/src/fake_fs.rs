use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use inkpaper_app::PlatformEntry;

pub(super) fn simulator_listing(path: &str) -> Option<Vec<PlatformEntry>> {
    if let Some(fonts) = fonts_path(path) {
        return host_listing(&fonts);
    }

    let entries = match path {
        "/" => vec![
            PlatformEntry::directory("Books"),
            PlatformEntry::directory("Documents"),
            PlatformEntry::directory("Fixtures"),
            PlatformEntry::directory("Read"),
            PlatformEntry::file("A Fire Upon the Deep.epub"),
            PlatformEntry::file("Blindsight.epub"),
            PlatformEntry::file("Children of Time.epub"),
            PlatformEntry::file("Dune.epub"),
            PlatformEntry::file("Foundation.epub"),
            PlatformEntry::file("Hyperion.epub"),
            PlatformEntry::file("Neuromancer.epub"),
            PlatformEntry::file("Project Hail Mary.epub"),
            PlatformEntry::file("Snow Crash.epub"),
            PlatformEntry::file("The Dispossessed.epub"),
            PlatformEntry::file("The Left Hand of Darkness.epub"),
            PlatformEntry::file("The Three-Body Problem.epub"),
        ],

        "/Books" => vec![
            PlatformEntry::directory("Sci-Fi"),
            PlatformEntry::file("Neuromancer.epub"),
            PlatformEntry::file("The Dispossessed.epub"),
            PlatformEntry::file("Hyperion.epub"),
        ],

        "/Books/Sci-Fi" => vec![
            PlatformEntry::file("Children of Time.epub"),
            PlatformEntry::file("The Left Hand of Darkness.epub"),
            PlatformEntry::file("Foundation.epub"),
        ],

        "/Documents" => vec![
            PlatformEntry::file("Distributed Systems.pdf"),
            PlatformEntry::file("Cloud Computing.pdf"),
        ],

        "/Fixtures" => fixture_entries(),

        "/Read" => Vec::new(),

        _ => return None,
    };

    Some(entries)
}

fn fixture_entries() -> Vec<PlatformEntry> {
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

    names.into_iter().map(PlatformEntry::file).collect()
}

fn fixtures_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// `/fonts` is the repository's font fixtures, so the reader has card fonts
/// to offer
fn fonts_path(path: &str) -> Option<PathBuf> {
    let relative = path.strip_prefix("/fonts")?;

    if !(relative.is_empty() || relative.starts_with('/')) {
        return None;
    }

    let relative = Path::new(relative.trim_start_matches('/'));

    if !relative
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return None;
    }

    Some(fixtures_directory().join("fonts").join(relative))
}

fn host_listing(directory: &Path) -> Option<Vec<PlatformEntry>> {
    let mut entries: Vec<PlatformEntry> = fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let file_type = entry.file_type().ok()?;

            Some(if file_type.is_dir() {
                PlatformEntry::directory(name)
            } else {
                PlatformEntry::file(name)
            })
        })
        .collect();

    entries.sort_by(|left, right| left.name().cmp(right.name()));

    Some(entries)
}
