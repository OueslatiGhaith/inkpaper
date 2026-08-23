use embedded_graphics::{
    Drawable,
    draw_target::DrawTarget,
    geometry::{Point, Size},
    mono_font::{MonoTextStyle, ascii::FONT_10X20},
    pixelcolor::Rgb888,
    primitives::{Primitive, PrimitiveStyle, Rectangle},
    text::{Baseline, Text},
};

pub fn draw<D>(target: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb888>,
{
    let white = Rgb888::new(255, 255, 255);
    let black = Rgb888::new(0, 0, 0);

    target.clear(white)?;

    let black_fill = PrimitiveStyle::with_fill(black);
    let border = PrimitiveStyle::with_stroke(black, 4);

    Rectangle::new(Point::new(0, 0), Size::new(480, 800))
        .into_styled(border)
        .draw(target)?;

    // top-left: one square.
    Rectangle::new(Point::new(24, 48), Size::new(64, 64))
        .into_styled(black_fill)
        .draw(target)?;

    // top-right: two vertical bars.
    Rectangle::new(Point::new(388, 48), Size::new(16, 64))
        .into_styled(black_fill)
        .draw(target)?;

    Rectangle::new(Point::new(424, 48), Size::new(16, 64))
        .into_styled(black_fill)
        .draw(target)?;

    // bottom-left: three horizontal bars.
    for index in 0..3 {
        Rectangle::new(Point::new(24, 672 + index * 28), Size::new(64, 12))
            .into_styled(black_fill)
            .draw(target)?;
    }

    // bottom-right: four squares.
    for row in 0..2 {
        for column in 0..2 {
            Rectangle::new(
                Point::new(380 + column * 40, 672 + row * 40),
                Size::new(24, 24),
            )
            .into_styled(black_fill)
            .draw(target)?;
        }
    }

    // center cross.
    Rectangle::new(Point::new(180, 396), Size::new(120, 8))
        .into_styled(black_fill)
        .draw(target)?;

    Rectangle::new(Point::new(236, 340), Size::new(8, 120))
        .into_styled(black_fill)
        .draw(target)?;

    let text_style = MonoTextStyle::new(&FONT_10X20, black);

    Text::with_baseline("TOP", Point::new(225, 16), text_style, Baseline::Top).draw(target)?;

    Text::with_baseline("BOTTOM", Point::new(210, 764), text_style, Baseline::Top).draw(target)?;

    Text::with_baseline("L", Point::new(20, 390), text_style, Baseline::Top).draw(target)?;

    Text::with_baseline("R", Point::new(450, 390), text_style, Baseline::Top).draw(target)?;

    Ok(())
}
