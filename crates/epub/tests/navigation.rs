use epub::{Epub, SliceSource};
use futures_lite::future;

struct TestEntry<'a> {
    name: &'a str,
    data: &'a [u8],
}

#[test]
fn loads_nested_epub3_navigation() {
    let bytes = build_epub3();

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    let navigation = future::block_on(epub.load_navigation()).unwrap().unwrap();

    assert_eq!(navigation.entries().len(), 2);

    let chapter = &navigation.entries()[0];

    assert_eq!(chapter.label(), "Chapter One");

    let target = chapter.target().unwrap();

    assert_eq!(target.path().as_str(), "OPS/Text/chapter1.xhtml");
    assert_eq!(target.fragment(), Some("start"));
    assert_eq!(chapter.children().len(), 1);
    assert_eq!(chapter.children()[0].label(), "First Section");

    let section_target = chapter.children()[0].target().unwrap();

    assert_eq!(section_target.path().as_str(), "OPS/Text/chapter1.xhtml");
    assert_eq!(section_target.fragment(), Some("section-1"));

    let part = &navigation.entries()[1];

    assert_eq!(part.label(), "Part Two");
    assert!(part.target().is_none());
    assert_eq!(part.children().len(), 1);
    assert_eq!(part.children()[0].label(), "Chapter Two");
    assert_eq!(
        part.children()[0].target().unwrap().path().as_str(),
        "OPS/Text/chapter2.xhtml",
    );
}

#[test]
fn loads_nested_epub2_ncx_navigation() {
    let bytes = build_epub2();

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    let navigation = future::block_on(epub.load_navigation()).unwrap().unwrap();

    assert_eq!(navigation.entries().len(), 2);

    let chapter = &navigation.entries()[0];

    assert_eq!(chapter.label(), "Chapter One");
    assert_eq!(
        chapter.target().unwrap().path().as_str(),
        "OPS/Text/chapter1.xhtml",
    );
    assert_eq!(chapter.target().unwrap().fragment(), Some("start"));
    assert_eq!(chapter.children().len(), 1);
    assert_eq!(chapter.children()[0].label(), "First Section");
    assert_eq!(
        chapter.children()[0].target().unwrap().fragment(),
        Some("section-1"),
    );

    let second = &navigation.entries()[1];

    assert_eq!(second.label(), "Chapter Two");
    assert_eq!(
        second.target().unwrap().path().as_str(),
        "OPS/Text/chapter2.xhtml",
    );
}

#[test]
fn epub_without_navigation_opens_normally() {
    let bytes = build_epub_without_navigation();

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    assert_eq!(epub.metadata().title(), Some("No TOC"));
    assert!(future::block_on(epub.load_navigation()).unwrap().is_none());
}

fn build_epub3() -> Vec<u8> {
    const CONTAINER: &str = r#"<?xml version="1.0"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
    <rootfiles>
        <rootfile
            full-path="OPS/package.opf"
            media-type="application/oebps-package+xml"
        />
    </rootfiles>
</container>
"#;

    const PACKAGE: &str = r#"<?xml version="1.0"?>
<package
    xmlns="http://www.idpf.org/2007/opf"
    version="3.0"
>
    <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
        <dc:title>EPUB 3 Navigation</dc:title>
    </metadata>

    <manifest>
        <item
            id="nav"
            href="nav.xhtml"
            media-type="application/xhtml+xml"
            properties="nav"
        />

        <item
            id="chapter-1"
            href="Text/chapter1.xhtml"
            media-type="application/xhtml+xml"
        />

        <item
            id="chapter-2"
            href="Text/chapter2.xhtml"
            media-type="application/xhtml+xml"
        />
    </manifest>

    <spine>
        <itemref idref="chapter-1"/>
        <itemref idref="chapter-2"/>
    </spine>
</package>
"#;

    const NAV: &str = r#"<?xml version="1.0"?>
<html
    xmlns="http://www.w3.org/1999/xhtml"
    xmlns:epub="http://www.idpf.org/2007/ops"
>
    <body>
        <nav epub:type="toc">
            <h1>Contents</h1>

            <ol>
                <li>
                    <a href="Text/chapter1.xhtml#start">
                        Chapter <em>One</em>
                    </a>

                    <ol>
                        <li>
                            <a href="Text/chapter1.xhtml#section-1">
                                First Section
                            </a>
                        </li>
                    </ol>
                </li>

                <li>
                    <span>Part Two</span>

                    <ol>
                        <li>
                            <a href="Text/chapter2.xhtml">
                                Chapter Two
                            </a>
                        </li>
                    </ol>
                </li>
            </ol>
        </nav>

        <nav epub:type="landmarks">
            <ol>
                <li>
                    <a href="Text/chapter1.xhtml">
                        Start
                    </a>
                </li>
            </ol>
        </nav>
    </body>
</html>
"#;

    build_stored_zip(&[
        TestEntry {
            name: "mimetype",
            data: b"application/epub+zip",
        },
        TestEntry {
            name: "META-INF/container.xml",
            data: CONTAINER.as_bytes(),
        },
        TestEntry {
            name: "OPS/package.opf",
            data: PACKAGE.as_bytes(),
        },
        TestEntry {
            name: "OPS/nav.xhtml",
            data: NAV.as_bytes(),
        },
    ])
}

fn build_epub2() -> Vec<u8> {
    const CONTAINER: &str = r#"<?xml version="1.0"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
    <rootfiles>
        <rootfile
            full-path="OPS/package.opf"
            media-type="application/oebps-package+xml"
        />
    </rootfiles>
</container>
"#;

    const PACKAGE: &str = r#"<?xml version="1.0"?>
<package
    xmlns="http://www.idpf.org/2007/opf"
    version="2.0"
>
    <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
        <dc:title>EPUB 2 Navigation</dc:title>
    </metadata>

    <manifest>
        <item
            id="ncx"
            href="toc.ncx"
            media-type="application/x-dtbncx+xml"
        />

        <item
            id="chapter-1"
            href="Text/chapter1.xhtml"
            media-type="application/xhtml+xml"
        />

        <item
            id="chapter-2"
            href="Text/chapter2.xhtml"
            media-type="application/xhtml+xml"
        />
    </manifest>

    <spine toc="ncx">
        <itemref idref="chapter-1"/>
        <itemref idref="chapter-2"/>
    </spine>
</package>
"#;

    const NCX: &str = r#"<?xml version="1.0"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/">
    <navMap>
        <navPoint id="chapter-1">
            <navLabel>
                <text>Chapter One</text>
            </navLabel>

            <content src="Text/chapter1.xhtml#start"/>

            <navPoint id="section-1">
                <navLabel>
                    <text>First Section</text>
                </navLabel>

                <content src="Text/chapter1.xhtml#section-1"/>
            </navPoint>
        </navPoint>

        <navPoint id="chapter-2">
            <navLabel>
                <text>Chapter Two</text>
            </navLabel>

            <content src="Text/chapter2.xhtml"/>
        </navPoint>
    </navMap>
</ncx>
"#;

    build_stored_zip(&[
        TestEntry {
            name: "mimetype",
            data: b"application/epub+zip",
        },
        TestEntry {
            name: "META-INF/container.xml",
            data: CONTAINER.as_bytes(),
        },
        TestEntry {
            name: "OPS/package.opf",
            data: PACKAGE.as_bytes(),
        },
        TestEntry {
            name: "OPS/toc.ncx",
            data: NCX.as_bytes(),
        },
    ])
}

fn build_epub_without_navigation() -> Vec<u8> {
    const CONTAINER: &str = r#"<?xml version="1.0"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
    <rootfiles>
        <rootfile
            full-path="package.opf"
            media-type="application/oebps-package+xml"
        />
    </rootfiles>
</container>
"#;

    const PACKAGE: &str = r#"<?xml version="1.0"?>
<package
    xmlns="http://www.idpf.org/2007/opf"
    version="3.0"
>
    <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
        <dc:title>No TOC</dc:title>
    </metadata>

    <manifest>
        <item
            id="chapter"
            href="chapter.xhtml"
            media-type="application/xhtml+xml"
        />
    </manifest>

    <spine>
        <itemref idref="chapter"/>
    </spine>
</package>
"#;

    build_stored_zip(&[
        TestEntry {
            name: "mimetype",
            data: b"application/epub+zip",
        },
        TestEntry {
            name: "META-INF/container.xml",
            data: CONTAINER.as_bytes(),
        },
        TestEntry {
            name: "package.opf",
            data: PACKAGE.as_bytes(),
        },
    ])
}

fn build_stored_zip(entries: &[TestEntry<'_>]) -> Vec<u8> {
    struct CentralEntry<'a> {
        name: &'a str,
        size: u32,
        local_offset: u32,
    }

    let mut output = Vec::new();
    let mut central = Vec::new();

    for entry in entries {
        let local_offset = u32::try_from(output.len()).unwrap();
        let size = u32::try_from(entry.data.len()).unwrap();
        let name_len = u16::try_from(entry.name.len()).unwrap();

        push_u32(&mut output, 0x0403_4b50);
        push_u16(&mut output, 20);
        push_u16(&mut output, 0x0800);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u32(&mut output, 0);
        push_u32(&mut output, size);
        push_u32(&mut output, size);
        push_u16(&mut output, name_len);
        push_u16(&mut output, 0);

        output.extend_from_slice(entry.name.as_bytes());
        output.extend_from_slice(entry.data);

        central.push(CentralEntry {
            name: entry.name,
            size,
            local_offset,
        });
    }

    let central_offset = u32::try_from(output.len()).unwrap();

    for entry in &central {
        let name_len = u16::try_from(entry.name.len()).unwrap();

        push_u32(&mut output, 0x0201_4b50);
        push_u16(&mut output, 20);
        push_u16(&mut output, 20);
        push_u16(&mut output, 0x0800);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u32(&mut output, 0);
        push_u32(&mut output, entry.size);
        push_u32(&mut output, entry.size);
        push_u16(&mut output, name_len);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u32(&mut output, 0);
        push_u32(&mut output, entry.local_offset);

        output.extend_from_slice(entry.name.as_bytes());
    }

    let central_size = u32::try_from(output.len() - central_offset as usize).unwrap();
    let entry_count = u16::try_from(central.len()).unwrap();

    push_u32(&mut output, 0x0605_4b50);
    push_u16(&mut output, 0);
    push_u16(&mut output, 0);
    push_u16(&mut output, entry_count);
    push_u16(&mut output, entry_count);
    push_u32(&mut output, central_size);
    push_u32(&mut output, central_offset);
    push_u16(&mut output, 0);

    output
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}
