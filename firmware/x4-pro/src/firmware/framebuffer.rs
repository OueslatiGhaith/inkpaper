use core::convert::Infallible;

use alloc::{vec, vec::Vec};
use embedded_graphics::{
    Pixel,
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Point, Size},
    pixelcolor::GrayColor,
    primitives::Rectangle,
};
use inkpaper_ui::backend::{EInkOrderedCoverageBitmap, Gray2};

pub const PHYSICAL_WIDTH: usize = 800;
pub const PHYSICAL_HEIGHT: usize = 480;

pub const LOGICAL_WIDTH: usize = 480;
pub const LOGICAL_HEIGHT: usize = 800;

pub const PHYSICAL_STRIDE: usize = PHYSICAL_WIDTH / 8;

pub const FRAMEBUFFER_LEN: usize = PHYSICAL_STRIDE * PHYSICAL_HEIGHT;

/// must remain byte-for-byte equivalent to inkpaper-ui's ordered_dither_accepts().
/// 
/// these are the 4x4 Bayer ranks transformed with:
/// ```
///     threshold = rank * 16 + 8
/// ```
/// 
/// the framebuffer equivalence tests protect this contract.
#[rustfmt::skip]
const BAYER_4X4_THRESHOLDS: [u8; 16] = [
    8, 136, 40, 168,
    200, 72, 232, 104,
    56, 184, 24, 152,
    248, 120, 216, 88,
];

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
    #[cfg(feature = "ui-metrics")]
    draw_iter_pixels: u64,
}

impl<'a> Framebuffer<'a> {
    pub fn new(storage: &'a mut FramebufferStorage, orientation: Orientation) -> Self {
        Self {
            storage,
            orientation,
            #[cfg(feature = "ui-metrics")]
            draw_iter_pixels: 0,
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

    pub fn get_pixel(&self, point: Point) -> Option<Gray2> {
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

        Some(Gray2::new(level))
    }

    fn set_pixel(&mut self, x: u16, y: u16, color: Gray2) {
        let (physical_x, physical_y) = self.orientation.map_point(x, y);
        let index = physical_y as usize * PHYSICAL_STRIDE + physical_x as usize / 8;

        let mask = 0x80u8 >> (physical_x as usize % 8);

        let level = color.luma();

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

    #[cfg(feature = "ui-metrics")]
    pub const fn draw_iter_pixels(&self) -> u64 {
        self.draw_iter_pixels
    }

    pub fn draw_ordered_coverage_bitmap(
        &mut self,
        bitmap: EInkOrderedCoverageBitmap<'_>,
    ) -> Option<u64> {
        if self.orientation != Orientation::Portrait {
            return None;
        }

        match bitmap.foreground().luma() {
            0 => self.blit_ordered_coverage_portrait_binary::<false>(bitmap),
            3 => self.blit_ordered_coverage_portrait_binary::<true>(bitmap),
            _ => None,
        }
    }

    fn blit_ordered_coverage_portrait_binary<const WHITE: bool>(
        &mut self,
        bitmap: EInkOrderedCoverageBitmap<'_>,
    ) -> Option<u64> {
        let width = usize::from(bitmap.width());
        let height = usize::from(bitmap.height());

        if width == 0 || height == 0 {
            return Some(0);
        }

        let coverage = bitmap.coverage();

        debug_assert_eq!(coverage.len(), width.saturating_mul(height));

        let origin = bitmap.origin();
        let origin_x = origin.x.get();
        let origin_y = origin.y.get();

        let glyph_right = origin_x.checked_add(i32::from(bitmap.width()))?;
        let glyph_bottom = origin_y.checked_add(i32::from(bitmap.height()))?;

        let clip = bitmap.clip();

        let left = origin_x.max(clip.x().get()).max(0);
        let top = origin_y.max(clip.y().get()).max(0);

        let right = glyph_right
            .min(clip.right().get())
            .min(LOGICAL_WIDTH as i32);

        let bottom = glyph_bottom
            .min(clip.bottom().get())
            .min(LOGICAL_HEIGHT as i32);

        if left >= right || top >= bottom {
            return Some(0);
        }

        let start_col = usize::try_from(left - origin_x).ok()?;
        let end_col = usize::try_from(right - origin_x).ok()?;
        let start_row = usize::try_from(top - origin_y).ok()?;
        let end_row = usize::try_from(bottom - origin_y).ok()?;

        let physical_y_start = PHYSICAL_HEIGHT - right as usize;
        let initial_x_phase = ((right - 1) as usize) & 0b11;

        let mut accepted = 0u64;

        for row in start_row..end_row {
            let logical_y = top as usize + (row - start_row);

            // portrait mapping:
            //
            // logical (x, y)
            //     -> physical (y, 479 - x)
            //
            // y is constant across this glyph row, so physical_x, the framebuffer
            // byte column, and the bit mask are all constant.
            let physical_x = logical_y;
            let byte_column = physical_x / 8;
            let mask = 0x80u8 >> (physical_x & 7);

            // traverse logical x from right to left. Physical y therefore increases
            // monotonically and framebuffer addressing becomes += PHYSICAL_STRIDE.
            let mut framebuffer_index = physical_y_start * PHYSICAL_STRIDE + byte_column;

            let row_start = row * width;

            let row_coverage = &coverage[row_start + start_col..row_start + end_col];

            let threshold_row = (logical_y & 0b11) * 4;
            let mut x_phase = initial_x_phase;

            for sample in row_coverage.iter().rev().copied() {
                let threshold = BAYER_4X4_THRESHOLDS[threshold_row + x_phase];

                if sample > threshold {
                    if WHITE {
                        self.storage.lsb[framebuffer_index] |= mask;
                        self.storage.msb[framebuffer_index] |= mask;
                    } else {
                        self.storage.lsb[framebuffer_index] &= !mask;
                        self.storage.msb[framebuffer_index] &= !mask;
                    }

                    accepted = accepted.saturating_add(1);
                }

                x_phase = (x_phase + 3) & 0b11;
                framebuffer_index += PHYSICAL_STRIDE;
            }
        }

        Some(accepted)
    }
}

impl OriginDimensions for Framebuffer<'_> {
    fn size(&self) -> Size {
        Size::new(LOGICAL_WIDTH as u32, LOGICAL_HEIGHT as u32)
    }
}

impl DrawTarget for Framebuffer<'_> {
    type Color = Gray2;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            if !contains_logical_pixel(point) {
                continue;
            }

            #[cfg(feature = "ui-metrics")]
            {
                self.draw_iter_pixels = self.draw_iter_pixels.saturating_add(1);
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

        self.fill_physical_region(physical, color.luma());

        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let level = color.luma();

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
