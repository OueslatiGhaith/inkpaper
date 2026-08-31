use crate::{ImageId, ImageSource, image::ImageResource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ImageRegistryError {
    Full,
}

#[derive(Clone, Copy)]
pub struct ImageRegistry<'image, const IMAGES: usize> {
    images: [Option<&'image dyn ImageResource>; IMAGES],
    len: usize,
}

impl<const IMAGES: usize> Default for ImageRegistry<'_, IMAGES> {
    fn default() -> Self {
        Self {
            images: [None; IMAGES],
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
        if self.len >= IMAGES {
            return Err(ImageRegistryError::Full);
        }

        let index = u16::try_from(self.len).map_err(|_| ImageRegistryError::Full)?;
        let id = ImageId::new(index);
        let source = ImageSource::new(id, image.size());

        self.images[self.len] = Some(image);
        self.len += 1;

        Ok(source)
    }

    pub fn get(&self, id: ImageId) -> Option<&'image dyn ImageResource> {
        self.images.get(id.index()).copied().flatten()
    }
}
