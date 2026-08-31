use crate::{Color, Size};

mod processing;
mod registry;

#[cfg(test)]
mod tests;

pub(crate) use processing::process_image_pixel;
pub use registry::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ImageId(u16);

impl ImageId {
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
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
}
