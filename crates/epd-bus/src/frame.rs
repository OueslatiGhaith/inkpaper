/// A rectangle of the panel, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Region {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

impl Region {
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn x(self) -> u16 {
        self.x
    }

    pub const fn y(self) -> u16 {
        self.y
    }

    pub const fn width(self) -> u16 {
        self.width
    }

    pub const fn height(self) -> u16 {
        self.height
    }

    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// Whether a 2-bit frame holds gray inside `region`: whether its two planes,
/// `stride` bytes a row, differ there. `region` must be byte-aligned
/// horizontally.
pub fn region_has_grayscale(lsb: &[u8], msb: &[u8], stride: usize, region: Region) -> bool {
    let x_byte = region.x as usize / 8;
    let row_bytes = region.width as usize / 8;

    for row in 0..region.height as usize {
        let start = (region.y as usize + row) * stride + x_byte;
        let end = start + row_bytes;

        if lsb[start..end] != msb[start..end] {
            return true;
        }
    }

    false
}

/// The black/white base of eight 2-bit pixels: white only where both planes
/// are set, so every gray starts from black.
pub const fn grayscale_base_byte(lsb: u8, msb: u8) -> u8 {
    lsb & msb
}

#[cfg(test)]
mod tests {
    use super::grayscale_base_byte;

    #[test]
    fn absolute_grayscale_maps_to_expected_bw_base() {
        // black: 00 -> black base
        assert_eq!(grayscale_base_byte(0x00, 0x00), 0x00);

        // dark: 10 -> black base
        assert_eq!(grayscale_base_byte(0xff, 0x00), 0x00);

        // light: 01 -> black base
        assert_eq!(grayscale_base_byte(0x00, 0xff), 0x00);

        // white: 11 -> white base
        assert_eq!(grayscale_base_byte(0xff, 0xff), 0xff);
    }
}
