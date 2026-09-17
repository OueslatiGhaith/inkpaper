#[cfg(feature = "alloc")]
use alloc::boxed::Box;

use crate::{ImageId, ImageSource, image::ImageResource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ImageRegistryError {
    Full,
}

enum ImageRegistryEntry<'image> {
    Borrowed(&'image dyn ImageResource),
    #[cfg(feature = "alloc")]
    Owned(Box<dyn ImageResource>),
}

impl ImageRegistryEntry<'_> {
    fn resource(&self) -> &dyn ImageResource {
        match self {
            Self::Borrowed(image) => *image,
            #[cfg(feature = "alloc")]
            Self::Owned(image) => image.as_ref(),
        }
    }

    #[cfg(feature = "alloc")]
    const fn is_owned(&self) -> bool {
        matches!(self, Self::Owned(_))
    }
}

pub struct ImageRegistry<'image, const IMAGES: usize> {
    images: [Option<ImageRegistryEntry<'image>>; IMAGES],
    generations: [u16; IMAGES],
    len: usize,
}

impl<const IMAGES: usize> Default for ImageRegistry<'_, IMAGES> {
    fn default() -> Self {
        Self {
            images: core::array::from_fn(|_| None),
            generations: [0; IMAGES],
            len: 0,
        }
    }
}

impl<'image, const IMAGES: usize> ImageRegistry<'image, IMAGES> {
    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn capacity(&self) -> usize {
        IMAGES
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn register(
        &mut self,
        image: &'image dyn ImageResource,
    ) -> Result<ImageSource, ImageRegistryError> {
        self.insert(ImageRegistryEntry::Borrowed(image))
    }

    #[cfg(feature = "alloc")]
    pub fn register_owned(
        &mut self,
        image: Box<dyn ImageResource>,
    ) -> Result<ImageSource, ImageRegistryError> {
        self.insert(ImageRegistryEntry::Owned(image))
    }

    #[cfg(feature = "alloc")]
    pub fn clear_owned(&mut self) {
        for (index, image) in self.images.iter_mut().enumerate() {
            let owned = image.as_ref().is_some_and(ImageRegistryEntry::is_owned);

            if !owned {
                continue;
            }

            *image = None;
            self.generations[index] = self.generations[index].wrapping_add(1);
            self.len -= 1;
        }
    }

    pub fn get(&self, id: ImageId) -> Option<&dyn ImageResource> {
        let index = id.index();

        if self.generations.get(index).copied()? != id.generation() {
            return None;
        }

        self.images
            .get(index)?
            .as_ref()
            .map(ImageRegistryEntry::resource)
    }

    fn insert(
        &mut self,
        image: ImageRegistryEntry<'image>,
    ) -> Result<ImageSource, ImageRegistryError> {
        let Some(index) = self.images.iter().position(Option::is_none) else {
            return Err(ImageRegistryError::Full);
        };

        let index_u16 = u16::try_from(index).map_err(|_| ImageRegistryError::Full)?;
        let generation = self.generations[index];

        let id = ImageId::from_parts(index_u16, generation);
        let source = ImageSource::new(id, image.resource().size());

        self.images[index] = Some(image);
        self.len += 1;

        Ok(source)
    }
}
