use std::convert::Infallible;

use futures_lite::future;
use inkpaper_epub::{
    BookLocation, ChapterImage, ContentOffset, Epub, ImageDimensions, Inline, SliceSource,
    SpineIndex,
};
use inkpaper_reader::{
    ImageMeasurer, PageItem, PageRange, ReaderSettings, Rect, TextMeasurer, TextStyle, Viewport,
    paginate_chapter,
};

#[derive(Default)]
struct MonoMeasurer {
    image_dimensions: Option<ImageDimensions>,
}

impl TextMeasurer for MonoMeasurer {
    type Error = Infallible;

    fn measure_text(&mut self, text: &str, _style: TextStyle) -> Result<u32, Self::Error> {
        Ok(text.chars().fold(0u32, |width, _| width.saturating_add(1)))
    }

    fn line_height(&mut self, _style: TextStyle) -> Result<u32, Self::Error> {
        Ok(1)
    }
}

impl ImageMeasurer for MonoMeasurer {
    fn image_dimensions(&mut self, _image: &ChapterImage) -> Option<ImageDimensions> {
        self.image_dimensions
    }
}

const TEST_JPEG: &[u8] = &[
    0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x20, 0x00, 0x40, 0x03, 0x01, 0x11, 0x00, 0x02,
    0x11, 0x00, 0x03, 0x11, 0x00,
];

#[test]
fn pagination_produces_stable_content_ranges() {
    let bytes = build_test_epub("<p>one two three four</p>");

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();

    let styles = future::block_on(epub.load_chapter_styles(&chapter)).unwrap();

    assert_eq!(chapter.content_len(), ContentOffset::new(18));

    let mut measurer = MonoMeasurer::default();

    let pagination = paginate_chapter(
        &chapter,
        &styles,
        SpineIndex::ZERO,
        Viewport::new(7, 2).unwrap(),
        ReaderSettings::new(16, 0).unwrap(),
        &mut measurer,
    )
    .unwrap();

    assert_eq!(pagination.len(), 2);
    assert_eq!(
        pagination.pages()[0].range(),
        PageRange::new(
            BookLocation::new(SpineIndex::ZERO, ContentOffset::ZERO),
            BookLocation::new(SpineIndex::ZERO, ContentOffset::new(14)),
        ),
    );
    assert_eq!(
        pagination.pages()[1].range(),
        PageRange::new(
            BookLocation::new(SpineIndex::ZERO, ContentOffset::new(14)),
            BookLocation::new(SpineIndex::ZERO, ContentOffset::new(18)),
        ),
    );
}

#[test]
fn repagination_changes_page_count_not_canonical_content_extent() {
    let bytes = build_test_epub("<p>one two three four</p>");

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();

    let styles = future::block_on(epub.load_chapter_styles(&chapter)).unwrap();

    let mut measurer = MonoMeasurer::default();

    let narrow = paginate_chapter(
        &chapter,
        &styles,
        SpineIndex::ZERO,
        Viewport::new(7, 2).unwrap(),
        ReaderSettings::new(16, 0).unwrap(),
        &mut measurer,
    )
    .unwrap();

    let wide = paginate_chapter(
        &chapter,
        &styles,
        SpineIndex::ZERO,
        Viewport::new(10, 2).unwrap(),
        ReaderSettings::new(16, 0).unwrap(),
        &mut measurer,
    )
    .unwrap();

    assert_eq!(narrow.len(), 2);
    assert_eq!(wide.len(), 1);
    assert_eq!(
        narrow.pages().last().unwrap().end().offset(),
        ContentOffset::new(18),
    );
    assert_eq!(
        wide.pages().last().unwrap().end().offset(),
        ContentOffset::new(18),
    );
}

#[test]
fn hidden_css_text_keeps_its_content_offsets_without_using_layout_space() {
    let bytes = build_test_epub(
        r#"
<p>
    one
    <span style="display: none"> hidden hidden </span>
    two
</p>
"#,
    );

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();

    let styles = future::block_on(epub.load_chapter_styles(&chapter)).unwrap();

    let content_end = chapter.content_len();

    assert!(content_end > ContentOffset::new(7));

    let mut measurer = MonoMeasurer::default();

    let pagination = paginate_chapter(
        &chapter,
        &styles,
        SpineIndex::ZERO,
        Viewport::new(20, 1).unwrap(),
        ReaderSettings::new(16, 0).unwrap(),
        &mut measurer,
    )
    .unwrap();

    assert_eq!(pagination.len(), 1);
    assert_eq!(pagination.pages()[0].end().offset(), content_end);
}

#[test]
fn image_can_occupy_a_page_without_advancing_content_location() {
    let bytes = build_test_epub(
        r#"
<p>one</p>
<p>
    <img
        src="../Images/picture.jpg"
        alt="Picture"
    />
</p>
<p>two</p>
"#,
    );

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();

    let styles = future::block_on(epub.load_chapter_styles(&chapter)).unwrap();

    let image = chapter
        .blocks()
        .iter()
        .flat_map(|block| block.inlines())
        .find_map(|inline| {
            let Inline::Image(image) = inline else {
                return None;
            };

            Some(image)
        })
        .unwrap();

    let dimensions = future::block_on(epub.image_dimensions(image.path()))
        .unwrap()
        .unwrap();

    assert_eq!(chapter.content_len(), ContentOffset::new(6));

    let mut measurer = MonoMeasurer {
        image_dimensions: Some(dimensions),
    };

    let pagination = paginate_chapter(
        &chapter,
        &styles,
        SpineIndex::ZERO,
        Viewport::new(10, 3).unwrap(),
        ReaderSettings::new(16, 0).unwrap(),
        &mut measurer,
    )
    .unwrap();

    assert_eq!(pagination.len(), 3);

    assert_eq!(
        pagination.pages()[0].range(),
        PageRange::new(
            BookLocation::new(SpineIndex::ZERO, ContentOffset::ZERO),
            BookLocation::new(SpineIndex::ZERO, ContentOffset::new(3)),
        ),
    );
    // the image occupies the entire second page but contributes no visible-text ContentOffset.
    assert_eq!(
        pagination.pages()[1].range(),
        PageRange::new(
            BookLocation::new(SpineIndex::ZERO, ContentOffset::new(3)),
            BookLocation::new(SpineIndex::ZERO, ContentOffset::new(3)),
        ),
    );
    assert_eq!(
        pagination.pages()[2].range(),
        PageRange::new(
            BookLocation::new(SpineIndex::ZERO, ContentOffset::new(3)),
            BookLocation::new(SpineIndex::ZERO, ContentOffset::new(6)),
        ),
    );
    assert_eq!(pagination.pages()[1].items().len(), 1);

    let PageItem::Image(fragment) = pagination.pages()[1].items()[0] else {
        panic!("expected image fragment");
    };

    assert_eq!(fragment.image().path().as_str(), "OPS/Images/picture.jpg");
    assert_eq!(fragment.bounds(), Rect::new(0, 0, 6, 3));
}

#[test]
fn hidden_image_does_not_consume_page_space() {
    let bytes = build_test_epub(
        r#"
<p>one</p>
<p>
    <img
        src="../Images/picture.jpg"
        style="display: none"
    />
</p>
<p>two</p>
"#,
    );

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();

    let styles = future::block_on(epub.load_chapter_styles(&chapter)).unwrap();

    let mut measurer = MonoMeasurer {
        image_dimensions: Some(ImageDimensions::new(64, 32)),
    };

    let pagination = paginate_chapter(
        &chapter,
        &styles,
        SpineIndex::ZERO,
        Viewport::new(10, 2).unwrap(),
        ReaderSettings::new(16, 0).unwrap(),
        &mut measurer,
    )
    .unwrap();

    assert_eq!(pagination.len(), 1);
    assert_eq!(
        pagination.pages()[0].range(),
        PageRange::new(
            BookLocation::new(SpineIndex::ZERO, ContentOffset::ZERO),
            BookLocation::new(SpineIndex::ZERO, ContentOffset::new(6)),
        ),
    );
    assert!(
        pagination.pages()[0]
            .items()
            .iter()
            .all(|item| { !matches!(item, PageItem::Image(_)) }),
    );
}

#[test]
fn pagination_emits_positioned_text_fragments_and_links() {
    let bytes = build_test_epub(
        r##"
<p style="text-align: center">one <a href="#target">two</a></p>
"##,
    );

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();

    let styles = future::block_on(epub.load_chapter_styles(&chapter)).unwrap();

    let mut measurer = MonoMeasurer::default();

    let pagination = paginate_chapter(
        &chapter,
        &styles,
        SpineIndex::ZERO,
        Viewport::new(10, 2).unwrap(),
        ReaderSettings::new(16, 0).unwrap(),
        &mut measurer,
    )
    .unwrap();

    assert_eq!(pagination.len(), 1);

    let page = &pagination.pages()[0];

    assert_eq!(page.items().len(), 3);

    let PageItem::Text(one) = page.items()[0] else {
        panic!("expected text");
    };

    assert_eq!(one.text(), "one");
    assert_eq!(one.bounds(), Rect::new(1, 0, 3, 1));
    assert!(one.link().is_none());

    let PageItem::Text(space) = page.items()[1] else {
        panic!("expected space");
    };

    assert_eq!(space.text(), " ");
    assert_eq!(space.bounds(), Rect::new(4, 0, 1, 1));

    let PageItem::Text(two) = page.items()[2] else {
        panic!("expected linked text");
    };

    assert_eq!(two.text(), "two");
    assert_eq!(two.bounds(), Rect::new(5, 0, 3, 1));

    let link = two.link().unwrap();

    assert_eq!(link.path().unwrap().as_str(), "OPS/Text/chapter.xhtml");
    assert_eq!(link.fragment(), Some("target"));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestMeasurementError {
    Text,
}

struct FailingMeasurer;

impl TextMeasurer for FailingMeasurer {
    type Error = TestMeasurementError;

    fn measure_text(&mut self, _text: &str, _style: TextStyle) -> Result<u32, Self::Error> {
        Err(TestMeasurementError::Text)
    }

    fn line_height(&mut self, _style: TextStyle) -> Result<u32, Self::Error> {
        Ok(1)
    }
}

impl ImageMeasurer for FailingMeasurer {}

#[test]
fn pagination_propagates_text_measurement_errors() {
    let bytes = build_test_epub("<p>one two</p>");

    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();

    let styles = future::block_on(epub.load_chapter_styles(&chapter)).unwrap();

    let mut measurer = FailingMeasurer;

    let result = paginate_chapter(
        &chapter,
        &styles,
        SpineIndex::ZERO,
        Viewport::new(10, 2).unwrap(),
        ReaderSettings::new(16, 0).unwrap(),
        &mut measurer,
    );

    assert_eq!(result.unwrap_err(), TestMeasurementError::Text);
}

fn build_test_epub(body: &str) -> Vec<u8> {
    let chapter = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
    <body>
        {}
    </body>
</html>
"#,
        body,
    );

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
        <dc:title>Reader test</dc:title>
    </metadata>

    <manifest>
        <item
            id="chapter"
            href="Text/chapter.xhtml"
            media-type="application/xhtml+xml"
        />

        <item
            id="picture"
            href="Images/picture.jpg"
            media-type="image/jpeg"
        />
    </manifest>

    <spine>
        <itemref idref="chapter"/>
    </spine>
</package>
"#;

    build_stored_zip(&[
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OPS/package.opf", PACKAGE.as_bytes()),
        ("OPS/Text/chapter.xhtml", chapter.as_bytes()),
        ("OPS/Images/picture.jpg", TEST_JPEG),
    ])
}

fn build_stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    struct CentralEntry {
        name: String,
        size: u32,
        local_offset: u32,
    }

    let mut output = Vec::new();
    let mut central = Vec::new();

    for (name, data) in entries {
        let local_offset = u32::try_from(output.len()).unwrap();
        let size = u32::try_from(data.len()).unwrap();
        let name_len = u16::try_from(name.len()).unwrap();

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

        output.extend_from_slice(name.as_bytes());
        output.extend_from_slice(data);

        central.push(CentralEntry {
            name: String::from(*name),
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
    let count = u16::try_from(central.len()).unwrap();

    push_u32(&mut output, 0x0605_4b50);
    push_u16(&mut output, 0);
    push_u16(&mut output, 0);
    push_u16(&mut output, count);
    push_u16(&mut output, count);
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
