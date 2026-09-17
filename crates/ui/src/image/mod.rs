use crate::{Color, Luminance, Size};

mod processing;
mod registry;
mod sampling;

#[cfg(test)]
mod tests;

pub(crate) use processing::process_image_pixel;
pub use registry::*;
pub(crate) use sampling::sample_image;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ImageId(u32);

impl ImageId {
    pub const fn new(index: u16) -> Self {
        Self(index as u32)
    }

    pub(crate) const fn from_parts(index: u16, generation: u16) -> Self {
        Self(((generation as u32) << 16) | index as u32)
    }

    pub(crate) const fn index(self) -> usize {
        (self.0 & 0xffff) as usize
    }

    pub(crate) const fn generation(self) -> u16 {
        (self.0 >> 16) as u16
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ImageSource {
    id: ImageId,
    size: Size,
}

impl ImageSource {
    pub const fn new(id: ImageId, size: Size) -> Self {
        Self { id, size }
    }

    pub const fn id(self) -> ImageId {
        self.id
    }

    pub const fn size(self) -> Size {
        self.size
    }
}

pub trait ImageResource {
    fn size(&self) -> Size;

    /// returns one source pixel
    ///
    /// coordinates are relative to the image origin and use integer pixel coordinates.
    /// Implementations should return `None` for coordinates outside the image bounds
    fn pixel(&self, x: u32, y: u32) -> Option<Color>;

    /// returns the luminance of one source pixel
    ///
    /// the default implementation derives luminance from `pixel()`. Resources with a native
    /// grayscale representation can override this to void reconstructing RGB values
    fn luminance(&self, x: u32, y: u32) -> Option<Luminance> {
        self.pixel(x, y).map(Color::luminance)
    }
}
