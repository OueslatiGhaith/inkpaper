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
<p><img src="../Images/picture.jpg"/></p>
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

    let PageItem::Image(fragment) = &pagination.pages()[1].items()[0] else {
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

    let PageItem::Text(one) = &page.items()[0] else {
        panic!("expected text");
    };

    assert_eq!(one.text(), "one");
    assert_eq!(one.bounds(), Rect::new(1, 0, 3, 1));
    assert!(one.link().is_none());

    let PageItem::Text(space) = &page.items()[1] else {
        panic!("expected space");
    };

    assert_eq!(space.text(), " ");
    assert_eq!(space.bounds(), Rect::new(4, 0, 1, 1));

    let PageItem::Text(two) = &page.items()[2] else {
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

#[test]
fn owned_pagination_keeps_text_links_images_and_ranges_after_chapter_drop() {
    let owned = {
        let bytes = build_test_epub(
            r##"<p><a href="#target">hello</a></p><p><img src="../Images/picture.jpg" alt="Picture"/></p>"##,
        );

        let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();

        let chapter = future::block_on(epub.load_spine_chapter(0))
            .unwrap()
            .unwrap();

        let styles = future::block_on(epub.load_chapter_styles(&chapter)).unwrap();

        let mut measurer = MonoMeasurer {
            image_dimensions: Some(ImageDimensions::new(6, 3)),
        };

        let borrowed = paginate_chapter(
            &chapter,
            &styles,
            SpineIndex::new(7),
            Viewport::new(10, 6).unwrap(),
            ReaderSettings::new(16, 0).unwrap(),
            &mut measurer,
        )
        .unwrap();

        let owned = borrowed.clone().into_owned();

        assert_eq!(owned, borrowed);

        owned
    };

    assert_eq!(owned.pages()[0].start().spine(), SpineIndex::new(7));

    let mut items = owned.pages().iter().flat_map(|page| page.items());

    let text = items
        .clone()
        .find_map(|item| match item {
            PageItem::Text(text) => Some(text),
            _ => None,
        })
        .unwrap();

    assert_eq!(text.text(), "hello");
    assert_eq!(text.link().unwrap().fragment(), Some("target"));

    let image = items
        .find_map(|item| match item {
            PageItem::Image(image) => Some(image),
            _ => None,
        })
        .unwrap();

    assert_eq!(image.image().path().as_str(), "OPS/Images/picture.jpg");
    assert_eq!(image.image().alt(), Some("Picture"));
}

fn paginate_positions(body: &str, width: u32, height: u32) -> inkpaper_reader::Pagination<'static> {
    let bytes = build_test_epub(body);
    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();
    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();
    let styles = future::block_on(epub.load_chapter_styles(&chapter)).unwrap();
    let mut measurer = MonoMeasurer {
        image_dimensions: Some(ImageDimensions::new(4, 2)),
    };

    paginate_chapter(
        &chapter,
        &styles,
        SpineIndex::new(3),
        Viewport::new(width, height).unwrap(),
        ReaderSettings::new(16, 0).unwrap(),
        &mut measurer,
    )
    .unwrap()
    .into_owned()
}

#[test]
fn repeated_image_pages_have_distinct_positions_that_survive_repagination() {
    let body = r#"<p><img src="../Images/picture.jpg"/><img src="../Images/picture.jpg"/><img src="../Images/picture.jpg"/></p>"#;
    let separate = paginate_positions(body, 4, 2);
    let combined = paginate_positions(body, 4, 4);

    assert_eq!(separate.len(), 3);
    assert_eq!(combined.len(), 2);

    for (index, page) in separate.pages().iter().enumerate() {
        assert_eq!(page.start().offset(), ContentOffset::ZERO);
        assert_eq!(page.end().offset(), ContentOffset::ZERO);
        assert_eq!(page.position().non_text(), index as u64);
        assert_eq!(separate.page_at_position(page.position()), Some(index));
        assert_eq!(combined.page_at_position(page.position()), Some(index / 2));
    }
}

#[test]
fn text_positions_restore_inside_reflowed_pages_and_count_unicode_scalars() {
    let body = "<p>é🙂漢字é🙂漢字</p>";
    let narrow = paginate_positions(body, 2, 1);
    let wide = paginate_positions(body, 4, 1);

    assert_eq!(narrow.len(), 4);
    assert_eq!(wide.len(), 2);

    for (index, page) in narrow.pages().iter().enumerate() {
        assert_eq!(
            page.position().location().offset(),
            ContentOffset::new(index as u64 * 2)
        );
        assert_eq!(page.position().non_text(), 0);
        assert_eq!(narrow.page_at_position(page.position()), Some(index));
        assert_eq!(wide.page_at_position(page.position()), Some(index / 2));
    }
}

#[test]
fn text_after_an_image_at_the_same_text_offset_has_its_own_position() {
    let pagination = paginate_positions(r#"<p><img src="../Images/picture.jpg"/>abcd</p>"#, 4, 2);
    assert_eq!(pagination.len(), 2);

    let image = &pagination.pages()[0];
    let text = &pagination.pages()[1];

    assert_eq!(image.start(), text.start());
    assert_eq!(image.position().non_text(), 0);
    assert_eq!(text.position().non_text(), 1);
    assert_eq!(pagination.page_at_position(image.position()), Some(0));
    assert_eq!(pagination.page_at_position(text.position()), Some(1));
}

#[test]
fn explicit_break_pages_have_distinct_positions() {
    let pagination = paginate_positions("<p><br/><br/><br/>abcd</p>", 4, 1);
    assert_eq!(pagination.len(), 4);

    for (index, page) in pagination.pages().iter().enumerate() {
        assert_eq!(page.position().non_text(), index as u64);
        assert_eq!(pagination.page_at_position(page.position()), Some(index));
    }
}

#[test]
fn hidden_and_unmeasurable_images_do_not_change_following_flow_coordinates() {
    let body = r#"<p><img style="display:none" src="../Images/picture.jpg"/><br style="display:none"/>abcd<img src="../Images/picture.jpg"/>efgh</p>"#;
    let bytes = build_test_epub(body);
    let mut epub = future::block_on(Epub::open(SliceSource::new(&bytes))).unwrap();
    let chapter = future::block_on(epub.load_spine_chapter(0))
        .unwrap()
        .unwrap();
    let styles = future::block_on(epub.load_chapter_styles(&chapter)).unwrap();
    let mut positions = Vec::new();

    for dimensions in [None, Some(ImageDimensions::new(4, 2))] {
        let mut measurer = MonoMeasurer {
            image_dimensions: dimensions,
        };
        let pagination = paginate_chapter(
            &chapter,
            &styles,
            SpineIndex::new(3),
            Viewport::new(4, 1).unwrap(),
            ReaderSettings::new(16, 0).unwrap(),
            &mut measurer,
        )
        .unwrap();

        let last = pagination.pages().last().unwrap();
        assert_eq!(last.end_position().non_text(), 3);
        positions.push(last.position());
    }

    assert_eq!(positions[0], positions[1]);
    assert_eq!(positions[0].non_text(), 3);
    assert_eq!(positions[0].location().offset(), ContentOffset::new(4));
}

#[test]
fn position_lookup_rejects_other_chapters_and_the_exclusive_end() {
    use inkpaper_reader::ReadingPosition;

    let pagination = paginate_positions("<p>abcd</p>", 2, 1);
    let first = pagination.pages()[0].position();

    for spine in [SpineIndex::new(2), SpineIndex::new(4)] {
        let other = ReadingPosition::new(BookLocation::new(spine, first.location().offset()), 0);
        assert_eq!(pagination.page_at_position(other), None);
    }

    assert_eq!(
        pagination.page_at_position(pagination.pages()[1].end_position()),
        None
    );
    assert_eq!(
        pagination.page_at_position(ReadingPosition::new(
            BookLocation::new(SpineIndex::new(3), ContentOffset::new(99)),
            0,
        )),
        None
    );

    let empty = paginate_positions("<p></p>", 2, 1);
    assert_eq!(empty.len(), 1);
    assert_eq!(empty.page_at_position(empty.pages()[0].position()), Some(0));
}
