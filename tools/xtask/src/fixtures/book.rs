use indoc::{formatdoc, indoc};

/// the files of one fixture, without the `mimetype` entry every EPUB starts with
pub struct Epub {
    name: String,
    files: Vec<(String, Vec<u8>)>,
}

impl Epub {
    /// `name` is the file name without `.epub`
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            files: Vec::new(),
        }
    }

    pub fn file(mut self, path: impl Into<String>, contents: impl Into<Vec<u8>>) -> Self {
        self.files.push((path.into(), contents.into()));
        self
    }

    pub fn file_name(&self) -> String {
        format!("{}.epub", self.name)
    }

    pub fn files(&self) -> &[(String, Vec<u8>)] {
        &self.files
    }
}

/// a well-formed EPUB 3 book under `OEBPS/`, with a package, a navigation
/// document and a shared stylesheet
pub struct Book {
    name: String,
    title: String,
    chapters: Vec<Chapter>,
    images: Vec<Image>,
    toc: Option<Vec<TocEntry>>,
    omitted: Vec<String>,
}

struct Chapter {
    name: String,
    title: String,
    body: String,
}

struct Image {
    name: String,
    bytes: Vec<u8>,
}

pub struct TocEntry {
    href: String,
    title: String,
    children: Vec<TocEntry>,
}

impl TocEntry {
    pub fn new(href: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            href: href.into(),
            title: title.into(),
            children: Vec::new(),
        }
    }

    pub fn child(mut self, entry: TocEntry) -> Self {
        self.children.push(entry);
        self
    }
}

impl Book {
    /// `name` is both the file name and the identifier. The title reads
    /// "Fixture — {title}"
    pub fn new(name: impl Into<String>, title: &str) -> Self {
        Self {
            name: name.into(),
            title: format!("Fixture — {title}"),
            chapters: Vec::new(),
            images: Vec::new(),
            toc: None,
            omitted: Vec::new(),
        }
    }

    /// `body` goes inside `<body>`. Chapters are read in the order they are added
    pub fn chapter(
        mut self,
        name: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        self.chapters.push(Chapter {
            name: name.into(),
            title: title.into(),
            body: body.into(),
        });
        self
    }

    /// a PNG listed in the manifest
    pub fn image(mut self, name: impl Into<String>, bytes: Vec<u8>) -> Self {
        self.images.push(Image {
            name: name.into(),
            bytes,
        });
        self
    }

    /// replaces the default table of contents, one entry per chapter
    pub fn toc(mut self, entries: Vec<TocEntry>) -> Self {
        self.toc = Some(entries);
        self
    }

    /// keeps a file listed in the package but leaves it out of the archive
    pub fn omit(mut self, path: impl Into<String>) -> Self {
        self.omitted.push(path.into());
        self
    }

    pub fn build(self) -> Epub {
        let toc = self.toc.unwrap_or_else(|| {
            self.chapters
                .iter()
                .map(|chapter| TocEntry::new(&chapter.name, &chapter.title))
                .collect()
        });

        let mut epub = Epub::new(&self.name)
            .file("META-INF/container.xml", container("OEBPS/content.opf"))
            .file("OEBPS/styles.css", STYLESHEET)
            .file(
                "OEBPS/content.opf",
                package_document(&self.title, &self.name, &self.chapters, &self.images),
            )
            .file("OEBPS/nav.xhtml", navigation_document(&self.title, &toc));

        for chapter in &self.chapters {
            epub = epub.file(
                format!("OEBPS/{}", chapter.name),
                chapter_document(&chapter.title, &chapter.body),
            );
        }

        for image in self.images {
            epub = epub.file(format!("OEBPS/{}", image.name), image.bytes);
        }

        epub.files.retain(|(path, _)| !self.omitted.contains(path));

        epub
    }
}

/// `META-INF/container.xml` pointing at the package document
pub fn container(package_path: &str) -> String {
    formatdoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <container
            version="1.0"
            xmlns="urn:oasis:names:tc:opendocument:xmlns:container"
        >
            <rootfiles>
                <rootfile
                    full-path="{package_path}"
                    media-type="application/oebps-package+xml"
                />
            </rootfiles>
        </container>
    "#}
}

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

fn package_document(
    title: &str,
    identifier: &str,
    chapters: &[Chapter],
    images: &[Image],
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

fn navigation_document(title: &str, toc: &[TocEntry]) -> String {
    let mut items = String::new();

    toc_items(toc, &mut items);

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

fn toc_items(entries: &[TocEntry], output: &mut String) {
    for entry in entries {
        output.push_str(&format!(
            r#"<li><a href="{}">{}</a>"#,
            entry.href, entry.title
        ));

        if !entry.children.is_empty() {
            output.push_str("<ol>");
            toc_items(&entry.children, output);
            output.push_str("</ol>");
        }

        output.push_str("</li>");
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
