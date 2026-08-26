use core::convert::Infallible;

use embedded_graphics::{
    Pixel,
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Point, Size},
    pixelcolor::{Rgb888, RgbColor},
    primitives::Rectangle,
};

pub const PHYSICAL_WIDTH: usize = 800;
pub const PHYSICAL_HEIGHT: usize = 480;

pub const LOGICAL_WIDTH: usize = 480;
pub const LOGICAL_HEIGHT: usize = 800;

pub const PHYSICAL_STRIDE: usize = PHYSICAL_WIDTH / 8;

pub const FRAMEBUFFER_LEN: usize = PHYSICAL_STRIDE * PHYSICAL_HEIGHT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    PortraitInverted,
}

impl Orientation {
    pub const fn map_point(self, x: u16, y: u16) -> (u16, u16) {
        match self {
            Orientation::Portrait => (y, PHYSICAL_HEIGHT as u16 - 1 - x),
            Orientation::PortraitInverted => (PHYSICAL_WIDTH as u16 - 1 - y, x),
        }
    }

    pub const fn map_region(self, region: Region) -> Region {
        match self {
            Orientation::Portrait => Region::new(
                region.y,
                PHYSICAL_HEIGHT as u16 - region.x - region.width,
                region.height,
                region.width,
            ),
            Orientation::PortraitInverted => Region::new(
                PHYSICAL_WIDTH as u16 - region.y - region.height,
                region.x,
                region.height,
                region.width,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
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

    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn clip_to(self, width: u16, height: u16) -> Option<Self> {
        if self.is_empty() || self.x >= width || self.y >= height {
            return None;
        }

        let right = (self.x as u32 + self.width as u32).min(width as u32);
        let bottom = (self.y as u32 + self.height as u32).min(height as u32);

        let clipped_width = right - self.x as u32;
        let clipped_height = bottom - self.y as u32;
        if clipped_width == 0 || clipped_height == 0 {
            return None;
        }

        Some(Self::new(
            self.x,
            self.y,
            clipped_width as u16,
            clipped_height as u16,
        ))
    }

    fn align_x_to_byte(self) -> Self {
        let x_start = self.x & !7;
        let x_end = ((self.x as u32 + self.width as u32 + 7) & !7).min(PHYSICAL_WIDTH as u32);

        Self::new(
            x_start,
            self.y,
            (x_end - x_start as u32) as u16,
            self.height,
        )
    }
}

pub struct Framebuffer<'a> {
    buffer: &'a mut [u8; FRAMEBUFFER_LEN],
    orientation: Orientation,
}

impl<'a> Framebuffer<'a> {
    pub fn new(buffer: &'a mut [u8; FRAMEBUFFER_LEN], orientation: Orientation) -> Self {
        Self {
            buffer,
            orientation,
        }
    }

    pub const fn orientation(&self) -> Orientation {
        self.orientation
    }

    pub fn set_orientation(&mut self, orientation: Orientation) {
        self.orientation = orientation;
    }

    pub fn physical_buffer(&self) -> &[u8; FRAMEBUFFER_LEN] {
        self.buffer
    }

    pub fn physical_buffer_mut(&mut self) -> &mut [u8; FRAMEBUFFER_LEN] {
        self.buffer
    }

    pub fn clear_white(&mut self) {
        self.buffer.fill(0xff);
    }

    /// coverts a logical damage rectangle into the physical controller-memory rectangle.
    ///
    /// the returned physical X range is expanded to complete bytes because the e-paper
    /// framebuffer and controller window operate on 8 horizontal pixels per byte.
    ///
    /// expanding damage is safe: the framebuffer already contains the final state of the
    /// neighboring pixels
    pub fn physical_damage_region(&self, logical: Region) -> Option<Region> {
        let logical = logical.clip_to(LOGICAL_WIDTH as u16, LOGICAL_HEIGHT as u16)?;
        let physical = self.orientation.map_region(logical);

        Some(physical.align_x_to_byte())
    }

    fn set_pixel(&mut self, x: u16, y: u16, color: Rgb888) {
        let (physical_x, physical_y) = self.orientation.map_point(x, y);
        let index = physical_y as usize * PHYSICAL_STRIDE + physical_x as usize / 8;
        let mask = 0x80u8 >> (physical_x as usize % 8);

        if is_black(color) {
            self.buffer[index] &= !mask;
        } else {
            self.buffer[index] |= mask;
        }
    }

    fn fill_physical_region(&mut self, region: Region, black: bool) {
        if region.is_empty() {
            return;
        }

        defmt::assert!(
            region.x as usize + region.width as usize <= PHYSICAL_WIDTH,
            "physical region exceeds framebuffer width"
        );
        defmt::assert!(
            region.y as usize + region.height as usize <= PHYSICAL_HEIGHT,
            "physical region exceeds framebuffer height"
        );

        let x_start = region.x as usize;
        let x_end = x_start + region.width as usize;

        let first_byte = x_start / 8;
        let last_byte = (x_end - 1) / 8;

        let first_offset = x_start % 8;
        let last_offset = (x_end - 1) % 8;

        let first_mask = 0xffu8 >> first_offset;
        let last_mask = 0xffu8 << (7 - last_offset);

        let full_byte = if black { 0x00 } else { 0xff };

        for physical_y in region.y as usize..region.y as usize + region.height as usize {
            let row_start = physical_y * PHYSICAL_STRIDE;
            if first_byte == last_byte {
                let mask = first_mask & last_mask;
                apply_mask(&mut self.buffer[row_start + first_byte], mask, black);
                continue;
            }

            apply_mask(&mut self.buffer[row_start + first_byte], first_mask, black);

            let middle_start = row_start + first_byte + 1;
            let middle_end = row_start + last_byte;
            if middle_start < middle_end {
                self.buffer[middle_start..middle_end].fill(full_byte);
            }

            apply_mask(&mut self.buffer[row_start + last_byte], last_mask, black);
        }
    }
}

impl OriginDimensions for Framebuffer<'_> {
    fn size(&self) -> Size {
        Size::new(LOGICAL_WIDTH as u32, LOGICAL_HEIGHT as u32)
    }
}

impl DrawTarget for Framebuffer<'_> {
    type Color = Rgb888;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            if !contains_logical_pixel(point) {
                continue;
            }

            self.set_pixel(point.x as u16, point.y as u16, color);
        }

        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let Some(logical) = logical_rectangle_to_region(area) else {
            return Ok(());
        };

        let physical = self.orientation.map_region(logical);
        self.fill_physical_region(physical, is_black(color));

        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.buffer.fill(if is_black(color) { 0x00 } else { 0xff });

        Ok(())
    }
}

fn is_black(color: Rgb888) -> bool {
    // integer approximation of Rec. 601 luminance:
    //   0.299 R + 0.587 G + 0.114 B
    // the weights sum to 256, so no division is required when
    // comparing against the midpoint.
    let luminance = 77 * color.r() as u16 + 150 * color.g() as u16 + 29 * color.b() as u16;

    luminance < 128 * 256
}

fn apply_mask(byte: &mut u8, mask: u8, black: bool) {
    if black {
        *byte &= !mask;
    } else {
        *byte |= mask;
    }
}

fn contains_logical_pixel(point: Point) -> bool {
    point.x >= 0
        && point.y >= 0
        && point.x < LOGICAL_WIDTH as i32
        && point.y < LOGICAL_HEIGHT as i32
}

fn logical_rectangle_to_region(area: &Rectangle) -> Option<Region> {
    if area.size.width == 0 || area.size.height == 0 {
        return None;
    }

    let left = area.top_left.x as i64;
    let top = area.top_left.y as i64;
    let right = left + area.size.width as i64;
    let bottom = top + area.size.height as i64;

    let clipped_left = left.clamp(0, LOGICAL_WIDTH as i64);
    let clipped_top = top.clamp(0, LOGICAL_HEIGHT as i64);
    let clipped_right = right.clamp(0, LOGICAL_WIDTH as i64);
    let clipped_bottom = bottom.clamp(0, LOGICAL_HEIGHT as i64);
    if clipped_right <= clipped_left || clipped_bottom <= clipped_top {
        return None;
    }

    Some(Region::new(
        clipped_left as u16,
        clipped_top as u16,
        (clipped_right - clipped_left) as u16,
        (clipped_bottom - clipped_top) as u16,
    ))
}
