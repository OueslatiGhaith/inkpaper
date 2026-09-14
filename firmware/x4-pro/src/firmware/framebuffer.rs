use core::convert::Infallible;

use alloc::{vec, vec::Vec};
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

pub struct FramebufferStorage {
    lsb: Vec<u8>,
    msb: Vec<u8>,
}

impl FramebufferStorage {
    pub fn white() -> Self {
        Self {
            lsb: vec![0xff; FRAMEBUFFER_LEN],
            msb: vec![0xff; FRAMEBUFFER_LEN],
        }
    }

    pub fn clear_white(&mut self) {
        self.lsb.fill(0xff);
        self.msb.fill(0xff);
    }

    pub fn lsb(&self) -> &[u8] {
        &self.lsb
    }

    pub fn msb(&self) -> &[u8] {
        &self.msb
    }

    pub fn planes(&self) -> (&[u8], &[u8]) {
        (&self.lsb, &self.msb)
    }

    pub fn binary_plane(&self) -> &[u8] {
        debug_assert!(!self.has_grayscale());

        &self.lsb
    }

    pub fn has_grayscale(&self) -> bool {
        self.lsb != self.msb
    }

    pub fn has_grayscale_in(&self, region: Region) -> bool {
        let Some(region) = region.clip_to(PHYSICAL_WIDTH as u16, PHYSICAL_HEIGHT as u16) else {
            return false;
        };

        let x_start = region.x as usize;
        let x_end = x_start + region.width as usize;

        let first_byte = x_start / 8;
        let last_byte = (x_end - 1) / 8;

        let first_offset = x_start % 8;
        let last_offset = (x_end - 1) % 8;

        let first_mask = 0xffu8 >> first_offset;
        let last_mask = 0xffu8 << (7 - last_offset);

        for y in region.y as usize..region.y as usize + region.height as usize {
            let row_start = y * PHYSICAL_STRIDE;

            if first_byte == last_byte {
                let mask = first_mask & last_mask;
                let index = row_start + first_byte;

                if (self.lsb[index] ^ self.msb[index]) & mask != 0 {
                    return true;
                }

                continue;
            }

            let first_index = row_start + first_byte;

            if (self.lsb[first_index] ^ self.msb[first_index]) & first_mask != 0 {
                return true;
            }

            for byte in first_byte + 1..last_byte {
                let index = row_start + byte;

                if self.lsb[index] != self.msb[index] {
                    return true;
                }
            }

            let last_index = row_start + last_byte;

            if (self.lsb[last_index] ^ self.msb[last_index]) & last_mask != 0 {
                return true;
            }
        }

        false
    }
}

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
    storage: &'a mut FramebufferStorage,
    orientation: Orientation,
}

impl<'a> Framebuffer<'a> {
    pub fn new(storage: &'a mut FramebufferStorage, orientation: Orientation) -> Self {
        Self {
            storage,
            orientation,
        }
    }

    pub const fn orientation(&self) -> Orientation {
        self.orientation
    }

    pub fn set_orientation(&mut self, orientation: Orientation) {
        self.orientation = orientation;
    }

    pub fn clear_white(&mut self) {
        self.storage.lsb.fill(0xff);
        self.storage.msb.fill(0xff);
    }

    pub fn physical_damage_region(&self, logical: Region) -> Option<Region> {
        let logical = logical.clip_to(LOGICAL_WIDTH as u16, LOGICAL_HEIGHT as u16)?;
        let physical = self.orientation.map_region(logical);

        Some(physical.align_x_to_byte())
    }

    pub fn get_pixel(&self, point: Point) -> Option<Rgb888> {
        if !contains_logical_pixel(point) {
            return None;
        }

        let x = point.x as u16;
        let y = point.y as u16;

        let (physical_x, physical_y) = self.orientation.map_point(x, y);

        let index = physical_y as usize * PHYSICAL_STRIDE + physical_x as usize / 8;
        let mask = 0x80u8 >> (physical_x as usize % 8);

        let lsb = self.storage.lsb[index] & mask != 0;
        let msb = self.storage.msb[index] & mask != 0;

        let level = u8::from(lsb) | (u8::from(msb) << 1);

        Some(level_color(level))
    }

    fn set_pixel(&mut self, x: u16, y: u16, color: Rgb888) {
        let (physical_x, physical_y) = self.orientation.map_point(x, y);
        let index = physical_y as usize * PHYSICAL_STRIDE + physical_x as usize / 8;

        let mask = 0x80u8 >> (physical_x as usize % 8);
        let level = quantize_color(color);

        set_mask(&mut self.storage.lsb[index], mask, level & 0b01 != 0);
        set_mask(&mut self.storage.msb[index], mask, level & 0b10 != 0);
    }

    fn fill_physical_region(&mut self, region: Region, level: u8) {
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

        fill_plane_region(&mut self.storage.lsb, region, level & 0b01 != 0);
        fill_plane_region(&mut self.storage.msb, region, level & 0b10 != 0);
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

        self.fill_physical_region(physical, quantize_color(color));

        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let level = quantize_color(color);

        let lsb = if level & 0b01 != 0 { 0xff } else { 0x00 };
        let msb = if level & 0b10 != 0 { 0xff } else { 0x00 };

        self.storage.lsb.fill(lsb);
        self.storage.msb.fill(msb);

        Ok(())
    }
}

fn fill_plane_region(plane: &mut [u8], region: Region, value: bool) {
    let x_start = region.x as usize;
    let x_end = x_start + region.width as usize;

    let first_byte = x_start / 8;
    let last_byte = (x_end - 1) / 8;

    let first_offset = x_start % 8;
    let last_offset = (x_end - 1) % 8;

    let first_mask = 0xffu8 >> first_offset;
    let last_mask = 0xffu8 << (7 - last_offset);

    let full_byte = if value { 0xff } else { 0x00 };

    for physical_y in region.y as usize..region.y as usize + region.height as usize {
        let row_start = physical_y * PHYSICAL_STRIDE;

        if first_byte == last_byte {
            let mask = first_mask & last_mask;

            set_mask(&mut plane[row_start + first_byte], mask, value);

            continue;
        }

        set_mask(&mut plane[row_start + first_byte], first_mask, value);

        let middle_start = row_start + first_byte + 1;
        let middle_end = row_start + last_byte;

        if middle_start < middle_end {
            plane[middle_start..middle_end].fill(full_byte);
        }

        set_mask(&mut plane[row_start + last_byte], last_mask, value);
    }
}

fn quantize_color(color: Rgb888) -> u8 {
    // rec. 601 luminance in a 256-scaled integer domain.
    let luminance =
        77u32 * u32::from(color.r()) + 150u32 * u32::from(color.g()) + 29u32 * u32::from(color.b());

    // convert to 0..255, then round to the nearest of four equally spaced levels:
    // 0   -> black
    // 85  -> dark gray
    // 170 -> light gray
    // 255 -> white
    let luminance = (luminance + 128) / 256;

    ((luminance * 3 + 127) / 255) as u8
}

fn level_color(level: u8) -> Rgb888 {
    match level {
        0 => Rgb888::new(0, 0, 0),
        1 => Rgb888::new(85, 85, 85),
        2 => Rgb888::new(170, 170, 170),
        _ => Rgb888::new(255, 255, 255),
    }
}

fn set_mask(byte: &mut u8, mask: u8, value: bool) {
    if value {
        *byte |= mask;
    } else {
        *byte &= !mask;
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
