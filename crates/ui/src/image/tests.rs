use super::*;
use crate::{
    Color, Size,
    image::registry::{ImageRegistry, ImageRegistryError},
    px,
};

struct SolidImage {
    size: Size,
    color: Color,
}

impl ImageResource for SolidImage {
    fn size(&self) -> Size {
        self.size
    }

    fn pixel(&self, x: u32, y: u32) -> Option<Color> {
        let width = u32::try_from(self.size.width.non_negative().get()).ok()?;
        let height = u32::try_from(self.size.height.non_negative().get()).ok()?;

        if x >= width || y >= height {
            return None;
        }

        Some(self.color)
    }
}

#[test]
fn image_registry_assigns_sources_in_registration_order() {
    let first = SolidImage {
        size: Size::new(px(10), px(20)),
        color: Color::RED,
    };
    let second = SolidImage {
        size: Size::new(px(30), px(40)),
        color: Color::GREEN,
    };

    let mut registry = ImageRegistry::<2>::default();

    let first_source = registry.register(&first).unwrap();
    let second_source = registry.register(&second).unwrap();

    assert_eq!(first_source.id(), ImageId::new(0));
    assert_eq!(first_source.size(), Size::new(px(10), px(20)));

    assert_eq!(second_source.id(), ImageId::new(1));
    assert_eq!(second_source.size(), Size::new(px(30), px(40)));

    assert_eq!(registry.len(), 2);
    assert_eq!(registry.capacity(), 2);
    assert!(!registry.is_empty());
}

#[test]
fn image_registry_resolves_registered_resources() {
    let image = SolidImage {
        size: Size::new(px(3), px(2)),
        color: Color::BLUE,
    };

    let mut registry = ImageRegistry::<1>::default();
    let source = registry.register(&image).unwrap();

    let resolved = registry.get(source.id()).unwrap();

    assert_eq!(resolved.size(), Size::new(px(3), px(2)));
    assert_eq!(resolved.pixel(0, 0), Some(Color::BLUE));
    assert_eq!(resolved.pixel(2, 1), Some(Color::BLUE));
    assert_eq!(resolved.pixel(3, 1), None);
}

#[test]
fn image_registry_reports_full_capacity() {
    let first = SolidImage {
        size: Size::new(px(1), px(1)),
        color: Color::RED,
    };
    let second = SolidImage {
        size: Size::new(px(1), px(1)),
        color: Color::GREEN,
    };

    let mut registry = ImageRegistry::<1>::default();

    assert!(registry.register(&first).is_ok());
    assert_eq!(registry.register(&second), Err(ImageRegistryError::Full));
    assert_eq!(registry.len(), 1);
}

#[test]
fn empty_image_registry_has_zero_registered_images() {
    let registry = ImageRegistry::<4>::default();

    assert_eq!(registry.len(), 0);
    assert_eq!(registry.capacity(), 4);
    assert!(registry.is_empty());
    assert!(registry.get(ImageId::new(0)).is_none());
}
