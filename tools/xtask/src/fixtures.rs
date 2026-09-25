use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Result;
use indoc::{formatdoc, indoc};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

pub fn generate() -> Result<()> {
    let fixtures = fixtures_directory();

    std::fs::create_dir_all(&fixtures)?;

    remove_generated_fixtures(&fixtures)?;

    generate_basic_text(&fixtures)?;
    generate_long_text(&fixtures)?;
    generate_chapters(&fixtures)?;
    generate_mixed_content(&fixtures)?;
    generate_image_layout(&fixtures)?;
    generate_transparent_image(&fixtures)?;
    generate_many_images(&fixtures)?;
    generate_image_only(&fixtures)?;

    generate_compat_encoded_container_path(&fixtures)?;
    generate_compat_epub2_entities(&fixtures)?;
    generate_compat_manifest_fallback(&fixtures)?;
    generate_compat_css_descendant_selector(&fixtures)?;

    println!("Generated EPUB fixtures:");

    for name in GENERATED_FIXTURES {
        let path = fixtures.join(name);
        let size = std::fs::metadata(&path)?.len();

        println!("  {name:<40} {size:>8} bytes");
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

const GENERATED_FIXTURES: &[&str] = &[
    "basic-text.epub",
    "long-text.epub",
    "chapters.epub",
    "mixed-content.epub",
    "image-layout.epub",
    "transparent-image.epub",
    "many-images.epub",
    "image-only.epub",
    "compat-encoded-container-path.epub",
    "compat-epub2-entities.epub",
    "compat-manifest-fallback.epub",
    "compat-css-descendant-selector.epub",
];

const CONTAINER_XML: &str = indoc! {r#"
    <?xml version="1.0" encoding="UTF-8"?>
    <container
        version="1.0"
        xmlns="urn:oasis:names:tc:opendocument:xmlns:container"
    >
        <rootfiles>
            <rootfile
                full-path="OEBPS/content.opf"
                media-type="application/oebps-package+xml"
            />
        </rootfiles>
    </container>
"#};

const STYLESHEET: &str = indoc! {r#"
    body {
        margin: 0;
        padding: 0;
    }

    h1 {
        margin: 0 0 16px 0;
    }

    p {
        margin: 0 0 12px 0;
    }

    img {
        display: block;
    }
"#};

struct Chapter {
    name: String,
    title: String,
    body: String,
}

impl Chapter {
    fn new(name: impl Into<String>, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            title: title.into(),
            body: body.into(),
        }
    }
}

struct ImageAsset {
    name: String,
    bytes: Vec<u8>,
}

impl ImageAsset {
    fn new(name: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            bytes,
        }
    }
}

fn remove_generated_fixtures(directory: &Path) -> anyhow::Result<()> {
    for name in GENERATED_FIXTURES {
        let path = directory.join(name);

        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }

    Ok(())
}

fn generate_basic_text(directory: &Path) -> Result<()> {
    let chapter = Chapter::new(
        "chapter.xhtml",
        "Basic Text",
        indoc! {r#"
            <h1>Basic Text</h1>
            <p>This is the smallest known-good InkPaper EPUB fixture.</p>
            <p>
                It verifies opening, pagination, rendering and reading progress
                without unusual content.
            </p>
        "#},
    );

    write_epub(
        &directory.join("basic-text.epub"),
        "Fixture — Basic Text",
        "basic-text",
        &[chapter],
        &[],
    )
}

fn generate_long_text(directory: &Path) -> Result<()> {
    let mut body = String::from("<h1>Long Text</h1>\n");

    for index in 1..=32 {
        body.push_str(&formatdoc! {r#"
            <p>
                Paragraph {index}. InkPaper should paginate this text
                deterministically. This sentence deliberately contains enough
                words to wrap over multiple lines on the X4 Pro viewport.
                Moving forward and backward should preserve page boundaries.
            </p>
        "#});
    }

    let chapter = Chapter::new("chapter.xhtml", "Long Text", body);

    write_epub(
        &directory.join("long-text.epub"),
        "Fixture — Long Text",
        "long-text",
        &[chapter],
        &[],
    )
}

fn generate_chapters(directory: &Path) -> Result<()> {
    let mut chapters = Vec::new();

    for chapter_index in 1..=4 {
        let mut body = format!("<h1>Chapter {chapter_index}</h1>\n");

        for paragraph_index in 1..=8 {
            body.push_str(&formatdoc! {r#"
                <p>
                    Chapter {chapter_index}, paragraph {paragraph_index}.
                    This content exercises crossing spine boundaries in both
                    directions and preserving whole-book progress.
                </p>
            "#});
        }

        chapters.push(Chapter::new(
            format!("chapter-{chapter_index}.xhtml"),
            format!("Chapter {chapter_index}"),
            body,
        ));
    }

    write_epub(
        &directory.join("chapters.epub"),
        "Fixture — Chapters",
        "chapters",
        &chapters,
        &[],
    )
}

fn generate_mixed_content(directory: &Path) -> Result<()> {
    let image = checkerboard_png(320, 180, 20)?;

    let chapter = Chapter::new(
        "chapter.xhtml",
        "Mixed Content",
        indoc! {r#"
            <h1>Mixed Content</h1>
            <p>This paragraph must appear before the image.</p>
            <p>The image below is a 320 x 180 checkerboard.</p>
            <p>
                <img src="mixed.png" alt="checkerboard"/>
            </p>
            <p>
                This paragraph must appear after the image. The image must
                consume pagination space rather than painting over text.
            </p>
            <p>
                Page navigation should remain stable with the image present.
            </p>
        "#},
    );

    let images = [ImageAsset::new("mixed.png", image)];

    write_epub(
        &directory.join("mixed-content.epub"),
        "Fixture — Mixed Content",
        "mixed-content",
        &[chapter],
        &images,
    )
}

fn generate_image_layout(directory: &Path) -> Result<()> {
    let images = [
        ImageAsset::new("small.png", checkerboard_png(96, 96, 12)?),
        ImageAsset::new("wide.png", gradient_png(900, 240)?),
        ImageAsset::new("tall.png", gradient_png(240, 1000)?),
        ImageAsset::new("large.png", checkerboard_png(900, 1000, 40)?),
    ];

    let chapter = Chapter::new(
        "chapter.xhtml",
        "Image Layout",
        indoc! {r#"
            <h1>Image Layout</h1>

            <p>Small image: 96 x 96. It should not be upscaled.</p>
            <p>
                <img src="small.png" alt="small"/>
            </p>

            <p>Wide image: 900 x 240. It should fit the reader width.</p>
            <p>
                <img src="wide.png" alt="wide"/>
            </p>

            <p>Tall image: 240 x 1000. It should fit the page height.</p>
            <p>
                <img src="tall.png" alt="tall"/>
            </p>

            <p>
                Oversized image: 900 x 1000. Both constraints apply and
                its aspect ratio must remain intact.
            </p>
            <p>
                <img src="large.png" alt="large"/>
            </p>

            <p>This text must remain reachable after all four images.</p>
    "#},
    );

    write_epub(
        &directory.join("image-layout.epub"),
        "Fixture — Image Layout",
        "image-layout",
        &[chapter],
        &images,
    )
}

fn generate_transparent_image(directory: &Path) -> Result<()> {
    let width = 256;
    let height = 128;

    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);

    for y in 0..height {
        for x in 0..width {
            let alpha = ((x * 255) / (width - 1)) as u8;

            let alpha = if y < height / 2 { alpha } else { 255 - alpha };

            pixels.extend_from_slice(&[0, 0, 0, alpha]);
        }
    }

    let image = encode_png(width, height, png::ColorType::Rgba, &pixels)?;

    let chapter = Chapter::new(
        "chapter.xhtml",
        "Transparent PNG",
        indoc! {r#"
            <h1>Transparent PNG</h1>

            <p>
                The image below contains a horizontal alpha gradient.
                Transparent pixels should composite against the white page.
            </p>
            <p>
                <img src="alpha.png" alt="alpha gradient"/>
            </p>

            <p>There should be no unexpected black rectangle around it.</p>
        "#},
    );

    let images = [ImageAsset::new("alpha.png", image)];

    write_epub(
        &directory.join("transparent-image.epub"),
        "Fixture — Transparent PNG",
        "transparent-image",
        &[chapter],
        &images,
    )
}

fn generate_many_images(directory: &Path) -> Result<()> {
    let mut images = Vec::new();
    let mut body = String::from(indoc! { r#"
        <h1>Many Images</h1>

        <p>
            Every image below is a distinct EPUB resource. This fixture
            exercises image registration, lookup and resource lifetime.
        </p>
    "#});

    for index in 0..16 {
        let name = format!("image-{index:02}.png");
        let shade = ((index * 255) / 15) as u8;

        let mut pixels = Vec::with_capacity(96 * 64 * 3);

        for y in 0..64 {
            for x in 0..96 {
                let light = ((x / 12) + (y / 12)) % 2 == 0;

                let value = if light { shade } else { 255 - shade };

                pixels.extend_from_slice(&[value, value, value]);
            }
        }

        images.push(ImageAsset::new(
            &name,
            encode_png(96, 64, png::ColorType::Rgb, &pixels)?,
        ));

        body.push_str(&formatdoc! {r#"
            <p>Image {} of 16:</p>
            <p><img src="{name}" alt="resource {}"/></p>
        "#,
            index + 1, index + 1
        });
    }

    body.push_str(indoc! {r#"
        <p>
            If this paragraph remains reachable, the complete sequence
            paginated successfully.
        </p>
    "#});

    let chapter = Chapter::new("chapter.xhtml", "Many Images", body);

    write_epub(
        &directory.join("many-images.epub"),
        "Fixture — Many Images",
        "many-images",
        &[chapter],
        &images,
    )
}

fn generate_image_only(directory: &Path) -> Result<()> {
    let cover = checkerboard_png(480, 720, 40)?;

    let chapter = Chapter::new(
        "cover.xhtml",
        "Image Only",
        r#"<img src="cover.png" alt="fixture cover"/>"#,
    );

    let images = [ImageAsset::new("cover.png", cover)];

    write_epub(
        &directory.join("image-only.epub"),
        "Fixture — Image Only",
        "image-only",
        &[chapter],
        &images,
    )
}

fn generate_compat_encoded_container_path(directory: &Path) -> Result<()> {
    const CONTAINER: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <container
            version="1.0"
            xmlns="urn:oasis:names:tc:opendocument:xmlns:container"
        >
            <rootfiles>
                <rootfile
                    full-path="OEBPS/My%20Book/content.opf"
                    media-type="application/oebps-package+xml"
                />
            </rootfiles>
        </container>
    "#};

    const PACKAGE: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <package
            xmlns="http://www.idpf.org/2007/opf"
            version="3.0"
            unique-identifier="book-id"
        >
            <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                <dc:identifier id="book-id">
                    urn:inkpaper:fixture:compat-encoded-container-path
                </dc:identifier>
                <dc:title>Compatibility — Encoded Container Path</dc:title>
                <dc:language>en</dc:language>

                <meta property="dcterms:modified">
                    2026-09-25T00:00:00Z
                </meta>
            </metadata>

            <manifest>
                <item
                    id="chapter"
                    href="chapter.xhtml"
                    media-type="application/xhtml+xml"
                />

                <item
                    id="nav"
                    href="nav.xhtml"
                    media-type="application/xhtml+xml"
                    properties="nav"
                />
            </manifest>

            <spine>
                <itemref idref="chapter"/>
            </spine>
        </package>
    "#};

    const CHAPTER: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <html xmlns="http://www.w3.org/1999/xhtml">
            <head>
                <title>Encoded Container Path</title>
            </head>

            <body>
                <h1>Encoded Container Path</h1>

                <p>
                    If you can read this, InkPaper correctly resolved a
                    percent-encoded package path from container.xml.
                </p>
            </body>
        </html>
    "#};

    const NAVIGATION: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <html
            xmlns="http://www.w3.org/1999/xhtml"
            xmlns:epub="http://www.idpf.org/2007/ops"
        >
            <head>
                <title>Contents</title>
            </head>

            <body>
                <nav epub:type="toc">
                    <ol>
                        <li>
                            <a href="chapter.xhtml">
                                Encoded Container Path
                            </a>
                        </li>
                    </ol>
                </nav>
            </body>
        </html>
    "#};

    write_text_epub(
        &directory.join("compat-encoded-container-path.epub"),
        &[
            ("META-INF/container.xml", CONTAINER),
            ("OEBPS/My Book/content.opf", PACKAGE),
            ("OEBPS/My Book/chapter.xhtml", CHAPTER),
            ("OEBPS/My Book/nav.xhtml", NAVIGATION),
        ],
    )
}

fn generate_compat_epub2_entities(directory: &Path) -> Result<()> {
    const CONTAINER: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <container
            version="1.0"
            xmlns="urn:oasis:names:tc:opendocument:xmlns:container"
        >
            <rootfiles>
                <rootfile
                    full-path="OEBPS/content.opf"
                    media-type="application/oebps-package+xml"
                />
            </rootfiles>
        </container>
    "#};

    const PACKAGE: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <package
            xmlns="http://www.idpf.org/2007/opf"
            version="2.0"
            unique-identifier="book-id"
        >
            <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                <dc:identifier id="book-id">
                    urn:inkpaper:fixture:compat-epub2-entities
                </dc:identifier>
                <dc:title>Compatibility — EPUB 2 Entities</dc:title>
                <dc:language>en</dc:language>
            </metadata>

            <manifest>
                <item
                    id="chapter"
                    href="chapter.xhtml"
                    media-type="application/xhtml+xml"
                />

                <item
                    id="ncx"
                    href="toc.ncx"
                    media-type="application/x-dtbncx+xml"
                />
            </manifest>

            <spine toc="ncx">
                <itemref idref="chapter"/>
            </spine>
        </package>
    "#};

    const CHAPTER: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <!DOCTYPE html
            PUBLIC "-//W3C//DTD XHTML 1.1//EN"
            "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd"
        >
        <html xmlns="http://www.w3.org/1999/xhtml">
            <head>
                <title>EPUB 2 Entities</title>
            </head>

            <body>
                <h1>EPUB 2 Entities</h1>

                <p>Fish&nbsp;Chips &copy; 2026 &mdash; EPUB 2</p>
            </body>
        </html>
    "#};

    const NCX: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <ncx
            xmlns="http://www.daisy.org/z3986/2005/ncx/"
            version="2005-1"
        >
            <head/>

            <docTitle>
                <text>Compatibility — EPUB 2 Entities</text>
            </docTitle>

            <navMap>
                <navPoint id="chapter" playOrder="1">
                    <navLabel>
                        <text>EPUB 2 Entities</text>
                    </navLabel>

                    <content src="chapter.xhtml"/>
                </navPoint>
            </navMap>
        </ncx>
    "#};

    write_text_epub(
        &directory.join("compat-epub2-entities.epub"),
        &[
            ("META-INF/container.xml", CONTAINER),
            ("OEBPS/content.opf", PACKAGE),
            ("OEBPS/chapter.xhtml", CHAPTER),
            ("OEBPS/toc.ncx", NCX),
        ],
    )
}

fn generate_compat_manifest_fallback(directory: &Path) -> Result<()> {
    const CONTAINER: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <container
            version="1.0"
            xmlns="urn:oasis:names:tc:opendocument:xmlns:container"
        >
            <rootfiles>
                <rootfile
                    full-path="OEBPS/content.opf"
                    media-type="application/oebps-package+xml"
                />
            </rootfiles>
        </container>
    "#};

    const PACKAGE: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <package
            xmlns="http://www.idpf.org/2007/opf"
            version="3.0"
            unique-identifier="book-id"
        >
            <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                <dc:identifier id="book-id">
                    urn:inkpaper:fixture:compat-manifest-fallback
                </dc:identifier>
                <dc:title>Compatibility — Manifest Fallback</dc:title>
                <dc:language>en</dc:language>

                <meta property="dcterms:modified">
                    2026-09-25T00:00:00Z
                </meta>
            </metadata>

            <manifest>
                <item
                    id="chapter-text"
                    href="chapter.txt"
                    media-type="text/plain"
                    fallback="chapter-xhtml"
                />

                <item
                    id="chapter-xhtml"
                    href="chapter.xhtml"
                    media-type="application/xhtml+xml"
                />

                <item
                    id="nav"
                    href="nav.xhtml"
                    media-type="application/xhtml+xml"
                    properties="nav"
                />
            </manifest>

            <spine>
                <itemref idref="chapter-text"/>
            </spine>
        </package>
    "#};

    const TEXT: &str = "Foreign content document";

    const CHAPTER: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <html xmlns="http://www.w3.org/1999/xhtml">
            <head>
                <title>Manifest Fallback</title>
            </head>

            <body>
                <h1>Manifest Fallback</h1>

                <p>
                    InkPaper should render this XHTML document after rejecting
                    the unsupported text/plain spine resource.
                </p>
            </body>
        </html>
    "#};

    const NAVIGATION: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <html
            xmlns="http://www.w3.org/1999/xhtml"
            xmlns:epub="http://www.idpf.org/2007/ops"
        >
            <head>
                <title>Contents</title>
            </head>

            <body>
                <nav epub:type="toc">
                    <ol>
                        <li>
                            <a href="chapter.xhtml">Manifest Fallback</a>
                        </li>
                    </ol>
                </nav>
            </body>
        </html>
    "#};

    write_text_epub(
        &directory.join("compat-manifest-fallback.epub"),
        &[
            ("META-INF/container.xml", CONTAINER),
            ("OEBPS/content.opf", PACKAGE),
            ("OEBPS/chapter.txt", TEXT),
            ("OEBPS/chapter.xhtml", CHAPTER),
            ("OEBPS/nav.xhtml", NAVIGATION),
        ],
    )
}

fn generate_compat_css_descendant_selector(directory: &Path) -> Result<()> {
    const CONTAINER: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <container
            version="1.0"
            xmlns="urn:oasis:names:tc:opendocument:xmlns:container"
        >
            <rootfiles>
                <rootfile
                    full-path="OEBPS/content.opf"
                    media-type="application/oebps-package+xml"
                />
            </rootfiles>
        </container>
    "#};

    const PACKAGE: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <package
            xmlns="http://www.idpf.org/2007/opf"
            version="3.0"
            unique-identifier="book-id"
        >
            <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                <dc:identifier id="book-id">
                    urn:inkpaper:fixture:compat-css-descendant-selector
                </dc:identifier>
                <dc:title>Compatibility — CSS Descendant Selector</dc:title>
                <dc:language>en</dc:language>

                <meta property="dcterms:modified">
                    2026-09-25T00:00:00Z
                </meta>
            </metadata>

            <manifest>
                <item
                    id="chapter"
                    href="chapter.xhtml"
                    media-type="application/xhtml+xml"
                />

                <item
                    id="style"
                    href="style.css"
                    media-type="text/css"
                />

                <item
                    id="nav"
                    href="nav.xhtml"
                    media-type="application/xhtml+xml"
                    properties="nav"
                />
            </manifest>

            <spine>
                <itemref idref="chapter"/>
            </spine>
        </package>
    "#};

    const CHAPTER: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <html xmlns="http://www.w3.org/1999/xhtml">
            <head>
                <title>CSS Descendant Selector</title>

                <link
                    rel="stylesheet"
                    type="text/css"
                    href="style.css"
                />
            </head>

            <body class="book">
                <h1>CSS Descendant Selector</h1>

                <p>THIS PARAGRAPH SHOULD BE CENTERED AND ITALIC.</p>
            </body>
        </html>
    "#};

    const STYLESHEET: &str = indoc! {r#"
        .book p {
            font-style: italic;
            text-align: center;
        }
    "#};

    const NAVIGATION: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <html
            xmlns="http://www.w3.org/1999/xhtml"
            xmlns:epub="http://www.idpf.org/2007/ops"
        >
            <head>
                <title>Contents</title>
            </head>

            <body>
                <nav epub:type="toc">
                    <ol>
                        <li>
                            <a href="chapter.xhtml">
                                CSS Descendant Selector
                            </a>
                        </li>
                    </ol>
                </nav>
            </body>
        </html>
    "#};

    write_text_epub(
        &directory.join("compat-css-descendant-selector.epub"),
        &[
            ("META-INF/container.xml", CONTAINER),
            ("OEBPS/content.opf", PACKAGE),
            ("OEBPS/chapter.xhtml", CHAPTER),
            ("OEBPS/style.css", STYLESHEET),
            ("OEBPS/nav.xhtml", NAVIGATION),
        ],
    )
}

fn checkerboard_png(width: u32, height: u32, cell_size: u32) -> Result<Vec<u8>> {
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 3);

    for y in 0..height {
        for x in 0..width {
            let light = ((x / cell_size) + (y / cell_size)).is_multiple_of(2);
            let value = if light { 224 } else { 32 };

            pixels.extend_from_slice(&[value, value, value]);
        }
    }

    encode_png(width, height, png::ColorType::Rgb, &pixels)
}

fn gradient_png(width: u32, height: u32) -> Result<Vec<u8>> {
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 3);

    for y in 0..height {
        for x in 0..width {
            let horizontal = if width <= 1 {
                0
            } else {
                ((x * 255) / (width - 1)) as u8
            };

            let vertical = if height <= 1 {
                0
            } else {
                ((y * 255) / (height - 1)) as u8
            };

            let mixed = ((u16::from(horizontal) + u16::from(vertical)) / 2) as u8;

            pixels.extend_from_slice(&[horizontal, vertical, mixed]);
        }
    }

    encode_png(width, height, png::ColorType::Rgb, &pixels)
}

fn encode_png(
    width: u32,
    height: u32,
    color_type: png::ColorType,
    pixels: &[u8],
) -> Result<Vec<u8>> {
    let mut output = Vec::new();

    {
        let mut encoder = png::Encoder::new(&mut output, width, height);

        encoder.set_color(color_type);
        encoder.set_depth(png::BitDepth::Eight);

        let mut writer = encoder.write_header()?;

        writer.write_image_data(pixels)?;
        writer.finish()?;
    }

    Ok(output)
}

fn write_epub(
    path: &Path,
    title: &str,
    identifier: &str,
    chapters: &[Chapter],
    images: &[ImageAsset],
) -> Result<()> {
    let file = File::create(path)?;
    let mut archive = ZipWriter::new(file);

    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    archive.start_file("mimetype", stored)?;
    archive.write_all(b"application/epub+zip")?;

    archive.start_file("META-INF/container.xml", deflated)?;
    archive.write_all(CONTAINER_XML.as_bytes())?;

    archive.start_file("OEBPS/styles.css", deflated)?;
    archive.write_all(STYLESHEET.as_bytes())?;

    let package = package_document(title, identifier, chapters, images);

    archive.start_file("OEBPS/content.opf", deflated)?;
    archive.write_all(package.as_bytes())?;

    let navigation = navigation_document(title, chapters);

    archive.start_file("OEBPS/nav.xhtml", deflated)?;
    archive.write_all(navigation.as_bytes())?;

    for chapter in chapters {
        let document = chapter_document(&chapter.title, &chapter.body);

        archive.start_file(format!("OEBPS/{}", chapter.name), deflated)?;
        archive.write_all(document.as_bytes())?;
    }

    for image in images {
        archive.start_file(format!("OEBPS/{}", image.name), deflated)?;
        archive.write_all(&image.bytes)?;
    }

    archive.finish()?;

    Ok(())
}

fn write_text_epub(path: &Path, entries: &[(&str, &str)]) -> Result<()> {
    let file = File::create(path)?;
    let mut archive = ZipWriter::new(file);

    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    archive.start_file("mimetype", stored)?;
    archive.write_all(b"application/epub+zip")?;

    for (name, content) in entries {
        archive.start_file(*name, deflated)?;
        archive.write_all(content.as_bytes())?;
    }

    archive.finish()?;

    Ok(())
}

fn package_document(
    title: &str,
    identifier: &str,
    chapters: &[Chapter],
    images: &[ImageAsset],
) -> String {
    let mut manifest = String::from(indoc! {r#"
        <item
            id="style"
            href="styles.css"
            media-type="text/css"
        />

        <item
            id="nav"
            href="nav.xhtml"
            media-type="application/xhtml+xml"
            properties="nav"
        />
    "#});

    let mut spine = String::new();

    for (index, chapter) in chapters.iter().enumerate() {
        manifest.push_str(&formatdoc! {r#"
            <item
                id="chapter-{index}"
                href="{}"
                media-type="application/xhtml+xml"
            />
            "#,
            chapter.name
        });

        spine.push_str(&format!(r#"<itemref idref="chapter-{index}"/>"#));
    }

    for (index, image) in images.iter().enumerate() {
        manifest.push_str(&formatdoc! {r#"
            <item
                id="image-{index}"
                href="{}"
                media-type="image/png"
            />
            "#,
            image.name
        });
    }

    formatdoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <package
            xmlns="http://www.idpf.org/2007/opf"
            version="3.0"
            unique-identifier="book-id"
        >
            <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                <dc:identifier id="book-id">
                    urn:inkpaper:fixture:{identifier}
                </dc:identifier>

                <dc:title>{title}</dc:title>
                <dc:creator>InkPaper Fixtures</dc:creator>
                <dc:language>en</dc:language>

                <meta property="dcterms:modified">
                    2026-09-17T00:00:00Z
                </meta>
            </metadata>

            <manifest>
                {manifest}
            </manifest>

            <spine>
                {spine}
            </spine>
        </package>
"#}
}

fn navigation_document(title: &str, chapters: &[Chapter]) -> String {
    let mut items = String::new();

    for chapter in chapters {
        items.push_str(&format!(
            r#"<li><a href="{}">{}</a></li>"#,
            chapter.name, chapter.title,
        ));
    }

    formatdoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <html
            xmlns="http://www.w3.org/1999/xhtml"
            xmlns:epub="http://www.idpf.org/2007/ops"
            xml:lang="en"
        >
        <head>
            <title>{title}</title>
        </head>

        <body>
            <nav epub:type="toc">
                <h1>{title}</h1>

                <ol>
                    {items}
                </ol>
            </nav>
        </body>
        </html>
"#
    }
}

fn chapter_document(title: &str, body: &str) -> String {
    formatdoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en">
        <head>
            <title>{title}</title>

            <link
                rel="stylesheet"
                type="text/css"
                href="styles.css"
            />
        </head>

        <body>
            {body}
        </body>
        </html>
    "#
    }
}
