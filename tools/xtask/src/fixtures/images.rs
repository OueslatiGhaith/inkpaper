use anyhow::Result;
use indoc::{formatdoc, indoc};

use super::{
    book::{Book, Epub},
    pixels::{checkerboard_png, encode_png, gradient_png},
};

pub fn mixed_content() -> Result<Epub> {
    Ok(Book::new("mixed-content", "Mixed Content")
        .chapter(
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
        )
        .image("mixed.png", checkerboard_png(320, 180, 20)?)
        .build())
}

pub fn image_layout() -> Result<Epub> {
    Ok(Book::new("image-layout", "Image Layout")
        .chapter(
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
        )
        .image("small.png", checkerboard_png(96, 96, 12)?)
        .image("wide.png", gradient_png(900, 240)?)
        .image("tall.png", gradient_png(240, 1000)?)
        .image("large.png", checkerboard_png(900, 1000, 40)?)
        .build())
}

pub fn transparent_image() -> Result<Epub> {
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

    Ok(Book::new("transparent-image", "Transparent PNG")
        .chapter(
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
        )
        .image(
            "alpha.png",
            encode_png(width, height, png::ColorType::Rgba, &pixels)?,
        )
        .build())
}

pub fn many_images() -> Result<Epub> {
    let mut book = Book::new("many-images", "Many Images");

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

        body.push_str(&formatdoc! {r#"
            <p>Image {} of 16:</p>
            <p><img src="{name}" alt="resource {}"/></p>
        "#,
            index + 1, index + 1
        });

        book = book.image(name, encode_png(96, 64, png::ColorType::Rgb, &pixels)?);
    }

    body.push_str(indoc! {r#"
        <p>
            If this paragraph remains reachable, the complete sequence
            paginated successfully.
        </p>
    "#});

    Ok(book.chapter("chapter.xhtml", "Many Images", body).build())
}

pub fn image_only() -> Result<Epub> {
    Ok(Book::new("image-only", "Image Only")
        .chapter(
            "cover.xhtml",
            "Image Only",
            r#"<img src="cover.png" alt="fixture cover"/>"#,
        )
        .image("cover.png", checkerboard_png(480, 720, 40)?)
        .build())
}

/// the image is cut off after its header, so it fails to decode
pub fn broken_image() -> Result<Epub> {
    let mut image = checkerboard_png(480, 800, 40)?;

    image.truncate(24);

    Ok(Book::new("broken-image", "Broken Image")
        .chapter(
            "chapter.xhtml",
            "Broken Image",
            indoc! {r#"
                <p>
                    Press Next to load the deliberately broken image. The current
                    page should stay visible with a failure notice. Tap the right
                    side to retry. Press Escape to clear the notice and go Home.
                </p>
                <img src="broken.png" alt="broken"/>
            "#},
        )
        .image("broken.png", image)
        .build())
}
