use inkpaper_ui_style_schema::tailwind::color::{Oklch, Rgb};

pub(super) fn oklch_to_srgb(value: Oklch) -> Rgb {
    let hue = value.hue.to_radians();

    let a = value.chroma * hue.cos();
    let b = value.chroma * hue.sin();

    let l_ = value.lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = value.lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = value.lightness - 0.089_484_177_5 * a - 1.291_485_548_0 * b;

    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    let red_linear = 4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s;
    let green_linear = -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s;
    let blue_linear = -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s;

    Rgb {
        red: linear_to_srgb_channel(red_linear),
        green: linear_to_srgb_channel(green_linear),
        blue: linear_to_srgb_channel(blue_linear),
    }
}

fn linear_to_srgb_channel(value: f64) -> u8 {
    let value = if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };

    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}
