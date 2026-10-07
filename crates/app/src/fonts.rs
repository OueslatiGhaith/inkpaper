//! Reader fonts on the card, found like crosspoint's `SdCardFontRegistry`.

use alloc::{format, string::String, vec::Vec};

use crate::{AppPlatform, PlatformEntry};

/// Scanned in order, so a family in `/.fonts` hides one of the same name in
/// `/fonts`.
const FONT_ROOTS: [&str; 2] = ["/.fonts", "/fonts"];

const MAX_FONT_FAMILIES: usize = 128;

const FONT_EXTENSIONS: [&str; 3] = [".ttf", ".otf", ".ttc"];

/// The families in the font folders, sorted by name. A loose font file is a
/// family named after the file; a folder of font files is a family named
/// after the folder.
pub(crate) async fn find_font_families<P: AppPlatform>(platform: &mut P) -> Vec<String> {
    let mut families = Vec::new();

    for root in FONT_ROOTS {
        // a card without the folder has no fonts in it
        let Ok(entries) = platform.list_directory(root).await else {
            continue;
        };

        for entry in entries {
            let Some(name) = family_name(platform, root, &entry).await else {
                continue;
            };

            if !families.contains(&name) {
                families.push(name);
            }
        }
    }

    families.sort();
    families.truncate(MAX_FONT_FAMILIES);

    families
}

async fn family_name<P: AppPlatform>(
    platform: &mut P,
    root: &str,
    entry: &PlatformEntry,
) -> Option<String> {
    let name = entry.name();

    if hidden(name) {
        return None;
    }

    if !entry.is_directory() {
        return font_file_stem(name).map(String::from);
    }

    let entries = platform
        .list_directory(&format!("{root}/{name}"))
        .await
        .ok()?;

    entries
        .iter()
        .any(|entry| !entry.is_directory() && !hidden(entry.name()) && is_font_file(entry.name()))
        .then(|| String::from(name))
}

/// crosspoint skips names starting with `.` or `_`, like macOS's `._` files
fn hidden(name: &str) -> bool {
    name.starts_with(['.', '_'])
}

fn is_font_file(name: &str) -> bool {
    font_file_stem(name).is_some()
}

/// The name without its `.ttf`, `.otf` or `.ttc`, in any case.
fn font_file_stem(name: &str) -> Option<&str> {
    FONT_EXTENSIONS.iter().find_map(|extension| {
        let split = name.len().checked_sub(extension.len())?;
        let (stem, tail) = (name.get(..split)?, name.get(split..)?);

        (!stem.is_empty() && tail.eq_ignore_ascii_case(extension)).then_some(stem)
    })
}
