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

        "/Fixtures" => vec![
            BrowseEntry::file("book-boundaries.epub"),
            BrowseEntry::file("broken-chapter.epub"),
            BrowseEntry::file("broken-image.epub"),
        ],

        "/Read" => vec![],

        _ => return None,
    };

    Some(BrowseListing::new(path, entries))
}
