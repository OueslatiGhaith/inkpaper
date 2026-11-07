#[cfg(feature = "alloc")]
use alloc::boxed::Box;

use crate::{
    Context, DamageRegion, ElementId, Entity, EntityAccessError, FontFace, FontFamilyId, FontId,
    FontRegistryError, FrameBuildError, FramePhase, ImageRegistryError, ImageResource, ImageSource,
    Offset, PaintError, PaintReport, PaintResources, Painter, Point, Runtime, RuntimeResources,
    Size, TextMeasurer, callback::ListenerInvokeError,
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

    /// whether something changed since the last frame was prepared, so a frame is
    /// waiting to render. Work that takes long can wait until it is on screen
    fn has_pending_frame(&self) -> bool;

    /// Sends an event to the element under `position` that listens for it,
    /// built from that element's id. Returns whether any listener ran.
    fn dispatch_at_with<E>(
        &mut self,
        position: Point,
        event: impl FnOnce(ElementId) -> E,
    ) -> Result<bool, ListenerInvokeError>
    where
        E: 'static;
}

pub trait ResourceRuntimeApi<'resource> {
    fn register_font_family(&mut self) -> Result<FontFamilyId, FontRegistryError>;

    fn register_font_face(
        &mut self,
        family: FontFamilyId,
        font: &'resource dyn FontFace,
    ) -> Result<FontId, FontRegistryError>;

    #[cfg(feature = "alloc")]
    fn register_owned_font_face(
        &mut self,
        family: FontFamilyId,
        font: Box<dyn FontFace>,
    ) -> Result<FontId, FontRegistryError>;

    #[cfg(feature = "alloc")]
    fn clear_owned_font_faces(&mut self);

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
/// type and [`Painter`], but not about embedded-graphics, framebuffers, SDL, e-ink, or any other backend.
pub trait RenderRuntimeApi {
    type Resources: TextMeasurer + PaintResources;

    /// see [`Runtime::prepare_frame`]
    fn prepare_frame(&mut self, viewport: Size) -> Result<DamageRegion, FrameBuildError> {
        self.prepare_frame_with(viewport, |_| ())
    }

    /// see [`Runtime::prepare_frame_with`]
    fn prepare_frame_with<G>(
        &mut self,
        viewport: Size,
        phase: impl FnMut(FramePhase) -> G,
    ) -> Result<DamageRegion, FrameBuildError>;

    fn paint_with_damage<P>(
        &mut self,
        damage: DamageRegion,
        painter: &mut P,
    ) -> Result<Option<PaintReport>, PaintError<P::Error>>
    where
        P: Painter;
}

impl<RESOURCES> RuntimeApi for Runtime<RESOURCES> {
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

    fn has_pending_frame(&self) -> bool {
        !self.render_invalidation().is_none()
    }

    fn dispatch_at_with<E>(
        &mut self,
        position: Point,
        event: impl FnOnce(ElementId) -> E,
    ) -> Result<bool, ListenerInvokeError>
    where
        E: 'static,
    {
        Runtime::dispatch_at_with(self, position, event)
    }
}

impl<
    'resource,
    const FONTS: usize,
    const GLYPH_SLOTS: usize,
    const GLYPH_BYTES: usize,
    const IMAGES: usize,
> ResourceRuntimeApi<'resource>
    for Runtime<RuntimeResources<'resource, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>>
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

    #[cfg(feature = "alloc")]
    fn register_owned_font_face(
        &mut self,
        family: FontFamilyId,
        font: Box<dyn FontFace>,
    ) -> Result<FontId, FontRegistryError> {
        self.register_owned_font_face(family, font)
    }

    #[cfg(feature = "alloc")]
    fn clear_owned_font_faces(&mut self) {
        self.clear_owned_font_faces();
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

impl<RESOURCES> RenderRuntimeApi for Runtime<RESOURCES>
where
    RESOURCES: TextMeasurer + PaintResources,
{
    type Resources = RESOURCES;

    fn prepare_frame_with<G>(
        &mut self,
        viewport: Size,
        phase: impl FnMut(FramePhase) -> G,
    ) -> Result<DamageRegion, FrameBuildError> {
        self.prepare_frame_with(viewport, phase)
    }

    fn paint_with_damage<P>(
        &mut self,
        damage: DamageRegion,
        painter: &mut P,
    ) -> Result<Option<PaintReport>, PaintError<P::Error>>
    where
        P: Painter,
    {
        self.paint_with_damage(damage, painter)
    }
}
