use crate::{Element, MountCx, MountError, NodeId, Size};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Image {
    source: ImageSource,
}

impl Image {
    pub const fn new(source: ImageSource) -> Self {
        Self { source }
    }

    pub const fn source(self) -> ImageSource {
        self.source
    }
}

impl Element for Image {
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        cx.push_image(self.source)
    }
}

pub fn image(source: impl Into<ImageSource>) -> Image {
    Image::new(source.into())
}
