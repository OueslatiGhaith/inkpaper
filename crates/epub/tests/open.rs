use futures_lite::future;
use inkpaper_epub::{
    ArchivePath, BlockKind, BookLocation, ContentOffset, Epub, FontStyle, FontWeight, Inline,
    SliceSource, SpineIndex, TextAlign,
};

use support::{DEFLATED, STORED, ZipEntry, build_zip};

mod support;

const TEST_JPEG: &[u8] = &[
    0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x20, 0x00, 0x40, 0x03, 0x01, 0x11, 0x00, 0x02,
    0x11, 0x00, 0x03, 0x11, 0x00,
];

#[test]
fn opens_epub_metadata_manifest_and_spine() {
    for xml_compression in [STORED, DEFLATED] {
        let bytes = build_test_epub(xml_compression);

        let epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();
        let package = epub.package();

        assert_eq!(package.path().as_str(), "OPS/package.opf");
        assert_eq!(package.version(), Some("3.0"));
        assert_eq!(package.unique_identifier(), Some("book-id"));
        assert_eq!(epub.metadata().title(), Some("Fish & Chips — EPUB Test"));
        assert_eq!(
            epub.metadata().creators(),
            &[std::string::String::from("Alice & Bob"),],
        );
        assert_eq!(epub.metadata().language(), Some("en"));
        assert_eq!(epub.metadata().identifier(), Some("urn:uuid:test-book"));

        let chapter = package.manifest_item("chapter-1").unwrap();

        assert_eq!(chapter.href(), "Text/./chapter1.xhtml");
        assert_eq!(chapter.path().as_str(), "OPS/Text/chapter1.xhtml");
        assert_eq!(chapter.media_type(), "application/xhtml+xml");

        let cover = package.manifest_item("cover-image").unwrap();

        assert_eq!(cover.path().as_str(), "OPS/Images/cover.jpg");
        assert!(cover.has_property("cover-image"));
        assert_eq!(epub.spine().toc(), Some("ncx"));
        assert_eq!(epub.spine().items().len(), 2);
        assert_eq!(epub.spine().items()[0].idref(), "chapter-1");
        assert!(epub.spine().items()[0].linear());
        assert_eq!(epub.spine().items()[1].idref(), "chapter-2");
        assert!(!epub.spine().items()[1].linear());
        assert_eq!(package.spine_manifest_item(0).unwrap().id(), "chapter-1");
    }
}

#[test]
fn reads_declared_manifest_and_spine_resources() {
    let bytes = build_test_epub(DEFLATED);

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    let chapter_one = future::block_on(epub.read_manifest_resource("chapter-1"))
        .unwrap()
        .unwrap();

    assert_eq!(chapter_one, b"<html><body><p>One</p></body></html>");

    let chapter_two = future::block_on(epub.read_spine_resource(1))
        .unwrap()
        .unwrap();

    assert_eq!(chapter_two, b"<html><body><p>Two</p></body></html>");

    let cover_path = ArchivePath::new("OPS/Images/cover.jpg").unwrap();
    let cover = future::block_on(epub.read_resource(&cover_path))
        .unwrap()
        .unwrap();

    assert_eq!(cover, TEST_JPEG);
    assert!(
        future::block_on(epub.read_manifest_resource("does-not-exist"))
            .unwrap()
            .is_none(),
    );
    assert!(
        future::block_on(epub.read_spine_resource(99))
            .unwrap()
            .is_none(),
    );

    let mimetype = ArchivePath::new("mimetype").unwrap();

    // `mimetype` exists in the ZIP, but it is not an OPF manifest resource. The public
    // resource API must not expose arbitrary archive entries.
    assert!(
        future::block_on(epub.read_resource(&mimetype))
            .unwrap()
            .is_none(),
    );
}

#[test]
fn loads_spine_xhtml_as_normalized_chapter() {
    let bytes = build_test_epub(DEFLATED);

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();

    assert_eq!(chapter.path().as_str(), "OPS/Text/chapter1.xhtml");
    assert_eq!(chapter.blocks().len(), 1);

    let block = &chapter.blocks()[0];

    assert_eq!(block.kind(), BlockKind::Paragraph);
    assert_eq!(block.inlines().len(), 1);

    let Inline::Text(text) = &block.inlines()[0] else {
        panic!("expected chapter text");
    };

    assert_eq!(text.text(), "One");
    assert!(!text.style().bold());
    assert!(!text.style().italic());
    assert!(text.link().is_none());
    assert!(
        future::block_on(epub.load_spine_chapter(99))
            .unwrap()
            .is_none(),
    );
}

#[test]
fn loads_and_resolves_external_chapter_styles() {
    let bytes = build_styled_test_epub();

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();

    let styles = future::block_on(epub.load_chapter_styles(&chapter)).unwrap();

    let text = chapter.blocks()[0]
        .inlines()
        .iter()
        .find_map(|inline| {
            let Inline::Text(text) = inline else {
                return None;
            };

            if text.text() == "One" {
                Some(text)
            } else {
                None
            }
        })
        .unwrap();

    let style = styles.style(text.style_node()).unwrap();

    assert_eq!(style.font_weight(), FontWeight::Bold);
    assert_eq!(style.font_style(), FontStyle::Italic);
    // the embedded stylesheet occurs after the external stylesheet and has equal
    // specificity, so source order wins.
    assert_eq!(style.text_align(), TextAlign::Right);
}

#[test]
fn probes_manifest_image_dimensions_without_decoding_pixels() {
    let bytes = build_test_epub(DEFLATED);

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    let cover = ArchivePath::new("OPS/Images/cover.jpg").unwrap();

    let dimensions = future::block_on(epub.image_dimensions(&cover))
        .unwrap()
        .unwrap();

    assert_eq!(dimensions.width(), 64);
    assert_eq!(dimensions.height(), 32);

    let chapter = ArchivePath::new("OPS/Text/chapter1.xhtml").unwrap();

    assert!(
        future::block_on(epub.image_dimensions(&chapter))
            .unwrap()
            .is_none(),
    );
}

#[test]
fn builds_stable_location_from_spine_and_normalized_content_offset() {
    let bytes = build_test_epub(DEFLATED);

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();

    assert_eq!(chapter.content_len(), ContentOffset::new(3));

    let location = BookLocation::new(SpineIndex::new(0), ContentOffset::new(2));

    assert_eq!(location.spine(), SpineIndex::new(0));
    assert_eq!(location.offset(), ContentOffset::new(2));
    assert!(chapter.contains_offset(location.offset()));
}

fn build_test_epub(xml_compression: u16) -> std::vec::Vec<u8> {
    const CONTAINER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container
    version="1.0"
    xmlns="urn:oasis:names:tc:opendocument:xmlns:container"
>
    <rootfiles>
        <rootfile
            full-path="OPS/package.opf"
            media-type="application/oebps-package+xml"
        />
    </rootfiles>
</container>
"#;

    const PACKAGE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<package
    xmlns="http://www.idpf.org/2007/opf"
    version="3.0"
    unique-identifier="book-id"
>
    <metadata
        xmlns:dc="http://purl.org/dc/elements/1.1/"
    >
        <dc:identifier id="book-id">urn:uuid:test-book</dc:identifier>
        <dc:title>Fish &amp; Chips &#x2014; EPUB Test</dc:title>
        <dc:creator>Alice &amp; Bob</dc:creator>
        <dc:language>en</dc:language>
    </metadata>

    <manifest>
        <item
            id="chapter-1"
            href="Text/./chapter1.xhtml"
            media-type="application/xhtml+xml"
        />

        <item
            id="chapter-2"
            href="Text/chapter2.xhtml"
            media-type="application/xhtml+xml"
        />

        <item
            id="cover-image"
            href="Images/cover.jpg"
            media-type="image/jpeg"
            properties="cover-image"
        />

        <item
            id="ncx"
            href="toc.ncx"
            media-type="application/x-dtbncx+xml"
        />
    </manifest>

    <spine toc="ncx">
        <itemref idref="chapter-1" />
        <itemref
            idref="chapter-2"
            linear="no"
        />
    </spine>
</package>
"#;

    const CHAPTER_ONE: &str = "<html><body><p>One</p></body></html>";
    const CHAPTER_TWO: &str = "<html><body><p>Two</p></body></html>";

    build_zip(&[
        ZipEntry {
            name: "mimetype",
            data: b"application/epub+zip",
            compression: STORED,
        },
        ZipEntry {
            name: "META-INF/container.xml",
            data: CONTAINER.as_bytes(),
            compression: xml_compression,
        },
        ZipEntry {
            name: "OPS/package.opf",
            data: PACKAGE.as_bytes(),
            compression: xml_compression,
        },
        ZipEntry {
            name: "OPS/Text/chapter1.xhtml",
            data: CHAPTER_ONE.as_bytes(),
            compression: DEFLATED,
        },
        ZipEntry {
            name: "OPS/Text/chapter2.xhtml",
            data: CHAPTER_TWO.as_bytes(),
            compression: DEFLATED,
        },
        ZipEntry {
            name: "OPS/Images/cover.jpg",
            data: TEST_JPEG,
            compression: STORED,
        },
        ZipEntry {
            name: "OPS/toc.ncx",
            data: b"<ncx/>",
            compression: DEFLATED,
        },
    ])
}

fn build_styled_test_epub() -> std::vec::Vec<u8> {
    const CONTAINER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container
    xmlns="urn:oasis:names:tc:opendocument:xmlns:container"
>
    <rootfiles>
        <rootfile
            full-path="OPS/package.opf"
            media-type="application/oebps-package+xml"
        />
    </rootfiles>
</container>
"#;

    const PACKAGE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<package
    xmlns="http://www.idpf.org/2007/opf"
    version="3.0"
>
    <metadata
        xmlns:dc="http://purl.org/dc/elements/1.1/"
    >
        <dc:title>Styled EPUB</dc:title>
    </metadata>

    <manifest>
        <item
            id="chapter"
            href="Text/chapter.xhtml"
            media-type="application/xhtml+xml"
        />

        <item
            id="style"
            href="Styles/book.css"
            media-type="text/css"
        />
    </manifest>

    <spine>
        <itemref idref="chapter"/>
    </spine>
</package>
"#;

    const CHAPTER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
    <head>
        <link
            rel="stylesheet"
            href="../Styles/book.css"
        />

        <style>
            .lead {
                text-align: right;
            }
        </style>
    </head>

    <body>
        <p class="lead">One</p>
    </body>
</html>
"#;

    const CSS: &str = r#"
.lead {
    font-weight: bold;
    font-style: italic;
    text-align: center;
}
"#;

    build_zip(&[
        ZipEntry {
            name: "mimetype",
            data: b"application/epub+zip",
            compression: STORED,
        },
        ZipEntry {
            name: "META-INF/container.xml",
            data: CONTAINER.as_bytes(),
            compression: DEFLATED,
        },
        ZipEntry {
            name: "OPS/package.opf",
            data: PACKAGE.as_bytes(),
            compression: DEFLATED,
        },
        ZipEntry {
            name: "OPS/Text/chapter.xhtml",
            data: CHAPTER.as_bytes(),
            compression: DEFLATED,
        },
        ZipEntry {
            name: "OPS/Styles/book.css",
            data: CSS.as_bytes(),
            compression: DEFLATED,
        },
    ])
}

#[test]
fn reports_uncompressed_spine_resource_sizes() {
    let bytes = build_test_epub(DEFLATED);

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    let sizes = future::block_on(epub.spine_resource_sizes()).unwrap();

    assert_eq!(
        sizes,
        vec![
            Some(b"<html><body><p>One</p></body></html>".len() as u64),
            Some(b"<html><body><p>Two</p></body></html>".len() as u64),
        ],
    );
}

#[test]
fn loads_percent_encoded_manifest_resource_path() {
    let bytes = build_percent_encoded_path_epub();

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    let item = epub.package().manifest_item("chapter").unwrap();

    assert_eq!(item.href(), "Text/Caf%C3%A9%20Chapter.xhtml");
    assert_eq!(item.path().as_str(), "OPS/Text/Café Chapter.xhtml");

    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();

    assert_eq!(chapter.path().as_str(), "OPS/Text/Café Chapter.xhtml");

    let Inline::Text(text) = &chapter.blocks()[0].inlines()[0] else {
        panic!("expected chapter text");
    };

    assert_eq!(text.text(), "Encoded path works");
}

fn build_percent_encoded_path_epub() -> std::vec::Vec<u8> {
    const CONTAINER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container
    xmlns="urn:oasis:names:tc:opendocument:xmlns:container"
>
    <rootfiles>
        <rootfile
            full-path="OPS/package.opf"
            media-type="application/oebps-package+xml"
        />
    </rootfiles>
</container>
"#;

    const PACKAGE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<package
    xmlns="http://www.idpf.org/2007/opf"
    version="3.0"
>
    <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
        <dc:title>Encoded Paths</dc:title>
    </metadata>

    <manifest>
        <item
            id="chapter"
            href="Text/Caf%C3%A9%20Chapter.xhtml"
            media-type="application/xhtml+xml"
        />
    </manifest>

    <spine>
        <itemref idref="chapter" />
    </spine>
</package>
"#;

    const CHAPTER: &str = "<html><body><p>Encoded path works</p></body></html>";

    build_zip(&[
        ZipEntry {
            name: "mimetype",
            data: b"application/epub+zip",
            compression: STORED,
        },
        ZipEntry {
            name: "META-INF/container.xml",
            data: CONTAINER.as_bytes(),
            compression: DEFLATED,
        },
        ZipEntry {
            name: "OPS/package.opf",
            data: PACKAGE.as_bytes(),
            compression: DEFLATED,
        },
        ZipEntry {
            name: "OPS/Text/Café Chapter.xhtml",
            data: CHAPTER.as_bytes(),
            compression: DEFLATED,
        },
    ])
}
