//! EPUB fixtures for tests and manual checks, written to `/fixtures`.
//!
//! most fixtures are well-formed books made with [`Book`]. Fixtures that test
//! unusual or broken files spell out their files with [`Epub`].

use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Result;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

mod book;
mod compat;
mod covers;
mod images;
mod navigation;
mod pixels;
mod text;

use book::Epub;

/// every generated fixture, in the order they are listed
const FIXTURES: &[fn() -> Result<Epub>] = &[
    text::basic_text,
    text::long_text,
    text::chapters,
    text::italics,
    text::typography,
    text::hyphenation,
    text::hyphenation_unsupported,
    images::mixed_content,
    images::image_layout,
    images::transparent_image,
    images::many_images,
    images::image_only,
    images::broken_image,
    covers::cover,
    covers::cover_epub2,
    covers::cover_guide,
    navigation::navigation_anchors,
    navigation::book_boundaries,
    navigation::broken_chapter,
    compat::encoded_container_path,
    compat::epub2_entities,
    compat::manifest_fallback,
    compat::css_descendant_selector,
];

pub fn generate() -> Result<()> {
    let directory = fixtures_directory();

    std::fs::create_dir_all(&directory)?;

    println!("Generated EPUB fixtures:");

    for fixture in FIXTURES {
        let epub = fixture()?;
        let path = directory.join(epub.file_name());

        write(&path, &epub)?;

        let size = std::fs::metadata(&path)?.len();

        println!("  {:<40} {size:>8} bytes", epub.file_name());
    }

    Ok(())
}

fn fixtures_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must live directly under in the /tools folder")
        .parent()
        .expect("/tools should live under the workspace root")
        .join("fixtures")
}

fn write(path: &Path, epub: &Epub) -> Result<()> {
    let file = File::create(path)?;
    let mut archive = ZipWriter::new(file);

    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    // EPUB readers expect an uncompressed mimetype as the first entry
    archive.start_file("mimetype", stored)?;
    archive.write_all(b"application/epub+zip")?;

    for (name, contents) in epub.files() {
        archive.start_file(name.as_str(), deflated)?;
        archive.write_all(contents)?;
    }

    archive.finish()?;

    Ok(())
}
