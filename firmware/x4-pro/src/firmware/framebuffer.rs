use core::convert::Infallible;

use embedded_graphics::{
    Pixel,
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Point, Size},
    pixelcolor::BinaryColor,
};

pub const PHYSICAL_WIDTH: usize = 800;
pub const PHYSICAL_HEIGHT: usize = 480;

pub const LOGICAL_WIDTH: usize = 480;
pub const LOGICAL_HEIGHT: usize = 800;

pub const PHYSICAL_STRIDE: usize = PHYSICAL_WIDTH / 8;

pub const FRAMEBUFFER_LEN: usize = PHYSICAL_STRIDE * PHYSICAL_HEIGHT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    Clockwise,
    CounterClockwise,
}

impl Rotation {
    pub const fn map_point(self, x: u16, y: u16) -> (u16, u16) {
        match self {
            Rotation::Clockwise => (PHYSICAL_WIDTH as u16 - 1 - y, x),
            Rotation::CounterClockwise => (y, LOGICAL_WIDTH as u16 - 1 - x),
        }
    }

    pub const fn map_region(self, region: Region) -> Region {
        let (x, y) = self.map_point(region.x, region.y);
        Region::new(x, y, region.height, region.width)
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
}

pub struct Framebuffer<'a> {
    buffer: &'a mut [u8; FRAMEBUFFER_LEN],
    rotation: Rotation,
}

impl<'a> Framebuffer<'a> {
    pub fn new(buffer: &'a mut [u8; FRAMEBUFFER_LEN], rotation: Rotation) -> Self {
        Self { buffer, rotation }
    }

    pub const fn rotation(&self) -> Rotation {
        self.rotation
    }

    pub fn set_rotation(&mut self, rotation: Rotation) {
        self.rotation = rotation;
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

    fn set_pixel(&mut self, x: u16, y: u16, color: BinaryColor) {
        let (physical_x, physical_y) = self.rotation.map_point(x, y);
        let index = physical_y as usize * PHYSICAL_STRIDE + physical_x as usize / 8;
        let mask = 0x80 >> (physical_x as usize % 8);

        match color {
            // for this panel representation:
            // 0 = black
            // 1 = white
            BinaryColor::On => self.buffer[index] &= !mask,
            BinaryColor::Off => self.buffer[index] |= mask,
        }
    }
}

impl OriginDimensions for Framebuffer<'_> {
    fn size(&self) -> Size {
        Size::new(LOGICAL_WIDTH as u32, LOGICAL_HEIGHT as u32)
    }
}

impl DrawTarget for Framebuffer<'_> {
    type Color = BinaryColor;
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

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        match color {
            BinaryColor::On => self.buffer.fill(0x00),
            BinaryColor::Off => self.buffer.fill(0xff),
        }

        Ok(())
    }
}

fn contains_logical_pixel(point: Point) -> bool {
    point.x >= 0
        && point.y >= 0
        && point.x < LOGICAL_WIDTH as i32
        && point.y < LOGICAL_HEIGHT as i32
}
