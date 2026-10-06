//! files that a well-formed [`Book`](super::book::Book) can't express, spelled
//! out in full

use anyhow::Result;
use indoc::indoc;

use super::book::{Epub, container};

pub fn encoded_container_path() -> Result<Epub> {
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

    Ok(Epub::new("compat-encoded-container-path")
        .file(
            "META-INF/container.xml",
            container("OEBPS/My%20Book/content.opf"),
        )
        .file("OEBPS/My Book/content.opf", PACKAGE)
        .file("OEBPS/My Book/chapter.xhtml", CHAPTER)
        .file("OEBPS/My Book/nav.xhtml", NAVIGATION))
}

pub fn epub2_entities() -> Result<Epub> {
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

    Ok(Epub::new("compat-epub2-entities")
        .file("META-INF/container.xml", container("OEBPS/content.opf"))
        .file("OEBPS/content.opf", PACKAGE)
        .file("OEBPS/chapter.xhtml", CHAPTER)
        .file("OEBPS/toc.ncx", NCX))
}

pub fn manifest_fallback() -> Result<Epub> {
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

    Ok(Epub::new("compat-manifest-fallback")
        .file("META-INF/container.xml", container("OEBPS/content.opf"))
        .file("OEBPS/content.opf", PACKAGE)
        .file("OEBPS/chapter.txt", TEXT)
        .file("OEBPS/chapter.xhtml", CHAPTER)
        .file("OEBPS/nav.xhtml", NAVIGATION))
}

pub fn css_descendant_selector() -> Result<Epub> {
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

    Ok(Epub::new("compat-css-descendant-selector")
        .file("META-INF/container.xml", container("OEBPS/content.opf"))
        .file("OEBPS/content.opf", PACKAGE)
        .file("OEBPS/chapter.xhtml", CHAPTER)
        .file("OEBPS/style.css", STYLESHEET)
        .file("OEBPS/nav.xhtml", NAVIGATION))
}
