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

#[test]
fn image_resource_derives_luminance_from_rgb_pixels() {
    let image = SolidImage {
        size: Size::new(px(1), px(1)),
        color: Color::RED,
    };

    assert_eq!(image.luminance(0, 0), Some(Luminance::new(77)));
    assert_eq!(image.luminance(1, 0), None);
}

#[cfg(feature = "alloc")]
#[test]
fn image_registry_owns_registered_resources() {
    let mut registry = ImageRegistry::<1>::default();

    let source = registry
        .register_owned(alloc::boxed::Box::new(SolidImage {
            size: Size::new(px(3), px(2)),
            color: Color::GREEN,
        }))
        .unwrap();

    let resolved = registry.get(source.id()).unwrap();

    assert_eq!(resolved.size(), Size::new(px(3), px(2)));
    assert_eq!(resolved.pixel(0, 0), Some(Color::GREEN));
    assert_eq!(resolved.pixel(2, 1), Some(Color::GREEN));
}

#[cfg(feature = "alloc")]
#[test]
fn clearing_owned_images_preserves_borrowed_images_and_invalidates_old_sources() {
    let borrowed = SolidImage {
        size: Size::new(px(2), px(2)),
        color: Color::RED,
    };

    let mut registry = ImageRegistry::<2>::default();

    let borrowed_source = registry.register(&borrowed).unwrap();

    let old_owned_source = registry
        .register_owned(alloc::boxed::Box::new(SolidImage {
            size: Size::new(px(3), px(3)),
            color: Color::GREEN,
        }))
        .unwrap();

    registry.clear_owned();

    assert_eq!(registry.len(), 1);
    assert!(registry.get(borrowed_source.id()).is_some());
    assert!(registry.get(old_owned_source.id()).is_none());

    let new_owned_source = registry
        .register_owned(alloc::boxed::Box::new(SolidImage {
            size: Size::new(px(4), px(4)),
            color: Color::BLUE,
        }))
        .unwrap();

    assert_ne!(new_owned_source.id(), old_owned_source.id());
    assert_eq!(
        registry.get(new_owned_source.id()).unwrap().pixel(0, 0),
        Some(Color::BLUE),
    );
}
