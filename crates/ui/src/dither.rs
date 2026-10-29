#[rustfmt::skip]
pub(crate) const BAYER_4X4: [u8; 16] = [
    0, 8, 2, 10,
    12, 4, 14, 6,
    3, 11, 1, 9,
    15, 7, 13, 5,
];

/// The 4x4 ordered-dither threshold at `(x, y)`, centered in its quantization
/// bucket (8, 24, 40, ... 248) and repeating every 4 pixels on both axes.
pub const fn bayer_4x4_threshold(x: i32, y: i32) -> u8 {
    let rank = BAYER_4X4[(y.rem_euclid(4) * 4 + x.rem_euclid(4)) as usize];

    rank * 16 + 8
}
