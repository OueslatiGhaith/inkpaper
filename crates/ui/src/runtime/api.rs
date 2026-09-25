#[cfg(feature = "alloc")]
use alloc::boxed::Box;

use crate::{
    Context, DamageRegion, Entity, EntityAccessError, FontFace, FontFamilyId, FontId,
    FontRegistryError, FrameBuildError, ImageRegistryError, ImageResource, ImageSource, Offset,
    PaintReport, Point, RenderInvalidation, ResourcePainter, Runtime, RuntimeResources, Size,
    TextMeasurer, callback::ListenerInvokeError,
};

/// application-facing runtime operations.
///
/// this intentionally excludes frame construction, layout, painting, render resources,
/// and backend-specific behavior.
pub trait RuntimeApi {
    fn update<T, R, F>(&self, entity: Entity<T>, update: F) -> Result<R, EntityAccessError>
    where
        T: 'static,
        F: FnOnce(&mut T, &mut Context<'_, T>) -> R;

    fn focus_previous(&mut self) -> bool;

    fn focus_next(&mut self) -> bool;

    fn clear_focus(&mut self);

    fn begin_focused_activation(&mut self) -> bool;

    fn complete_focused_activation(&mut self) -> Result<bool, ListenerInvokeError>;

    fn begin_activation_at(&mut self, position: Point) -> bool;

    fn complete_activation_at(&mut self, position: Point) -> Result<bool, ListenerInvokeError>;

    fn cancel_activation(&mut self);

    fn scroll_at(&mut self, position: Point, delta: Offset) -> bool;
}

pub trait ResourceRuntimeApi<'resource> {
    fn register_font_family(&mut self) -> Result<FontFamilyId, FontRegistryError>;

    fn register_font_face(
        &mut self,
        family: FontFamilyId,
        font: &'resource dyn FontFace,
    ) -> Result<FontId, FontRegistryError>;

    fn register_image(
        &mut self,
        image: &'resource dyn ImageResource,
    ) -> Result<ImageSource, ImageRegistryError>;

    #[cfg(feature = "alloc")]
    fn register_owned_image(
        &mut self,
        image: Box<dyn ImageResource>,
    ) -> Result<ImageSource, ImageRegistryError>;

    #[cfg(feature = "alloc")]
    fn clear_owned_images(&mut self);
}

/// platform-facing rendering operations.
///
/// this interface is backend-independent. It knows about the runtime's abstract resource
/// type and `ResourcePainter`, but not about embedded-graphics, framebuffers, SDL, e-ink, or any other backend.
pub trait RenderRuntimeApi {
    type Resources: TextMeasurer;

    fn rebuild(&mut self) -> Result<(), FrameBuildError>;

    fn layout(&mut self, viewport: Size) -> Option<Size>;

    fn take_render_invalidation(&self) -> RenderInvalidation;

    fn paint_with_damage<P>(
        &mut self,
        damage: DamageRegion,
        painter: &mut P,
    ) -> Result<Option<PaintReport>, P::Error>
    where
        P: ResourcePainter<Self::Resources>;
}

impl<
    const EB: usize,
    const ES: usize,
    const CB: usize,
    const CS: usize,
    const FN: usize,
    const FT: usize,
    const ST: usize,
    const GB: usize,
    const GS: usize,
    RESOURCES,
> RuntimeApi for Runtime<EB, ES, CB, CS, FN, FT, ST, GB, GS, RESOURCES>
{
    fn update<T, R, F>(&self, entity: Entity<T>, update: F) -> Result<R, EntityAccessError>
    where
        T: 'static,
        F: FnOnce(&mut T, &mut Context<'_, T>) -> R,
    {
        self.update(entity, update)
    }

    fn focus_previous(&mut self) -> bool {
        self.focus_previous()
    }

    fn focus_next(&mut self) -> bool {
        self.focus_next()
    }

    fn clear_focus(&mut self) {
        self.clear_focus();
    }

    fn begin_focused_activation(&mut self) -> bool {
        self.begin_focused_activation()
    }

    fn complete_focused_activation(&mut self) -> Result<bool, ListenerInvokeError> {
        self.complete_focused_activation()
    }

    fn begin_activation_at(&mut self, position: Point) -> bool {
        self.begin_activation_at(position)
    }

    fn complete_activation_at(&mut self, position: Point) -> Result<bool, ListenerInvokeError> {
        self.complete_activation_at(position)
    }

    fn cancel_activation(&mut self) {
        self.cancel_activation();
    }

    fn scroll_at(&mut self, position: Point, delta: Offset) -> bool {
        self.scroll_at(position, delta)
    }
}

impl<
    'resource,
    const EB: usize,
    const ES: usize,
    const CB: usize,
    const CS: usize,
    const FN: usize,
    const FT: usize,
    const ST: usize,
    const GB: usize,
    const GS: usize,
    const FONTS: usize,
    const GLYPH_SLOTS: usize,
    const GLYPH_BYTES: usize,
    const IMAGES: usize,
> ResourceRuntimeApi<'resource>
    for Runtime<
        EB,
        ES,
        CB,
        CS,
        FN,
        FT,
        ST,
        GB,
        GS,
        RuntimeResources<'resource, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>,
    >
{
    fn register_font_family(&mut self) -> Result<FontFamilyId, FontRegistryError> {
        self.register_font_family()
    }

    fn register_font_face(
        &mut self,
        family: FontFamilyId,
        font: &'resource dyn FontFace,
    ) -> Result<FontId, FontRegistryError> {
        self.register_font_face(family, font)
    }

    fn register_image(
        &mut self,
        image: &'resource dyn ImageResource,
    ) -> Result<ImageSource, ImageRegistryError> {
        self.register_image(image)
    }

    #[cfg(feature = "alloc")]
    fn register_owned_image(
        &mut self,
        image: Box<dyn ImageResource>,
    ) -> Result<ImageSource, ImageRegistryError> {
        self.register_owned_image(image)
    }

    #[cfg(feature = "alloc")]
    fn clear_owned_images(&mut self) {
        self.clear_owned_images();
    }
}

impl<
    const EB: usize,
    const ES: usize,
    const CB: usize,
    const CS: usize,
    const FN: usize,
    const FT: usize,
    const ST: usize,
    const GB: usize,
    const GS: usize,
    RESOURCES,
> RenderRuntimeApi for Runtime<EB, ES, CB, CS, FN, FT, ST, GB, GS, RESOURCES>
where
    RESOURCES: TextMeasurer,
{
    type Resources = RESOURCES;

    fn rebuild(&mut self) -> Result<(), FrameBuildError> {
        self.rebuild()
    }

    fn layout(&mut self, viewport: Size) -> Option<Size> {
        self.layout(viewport)
    }

    fn take_render_invalidation(&self) -> RenderInvalidation {
        self.take_render_invalidation()
    }

    fn paint_with_damage<P>(
        &mut self,
        damage: DamageRegion,
        painter: &mut P,
    ) -> Result<Option<PaintReport>, P::Error>
    where
        P: ResourcePainter<Self::Resources>,
    {
        self.paint_with_damage(damage, painter)
    }
}
