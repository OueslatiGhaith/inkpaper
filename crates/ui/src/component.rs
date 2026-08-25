/// marker type for a [`ComponentSlot`] that has not been filled yet.
///
/// `EmptySlot` intentionally does not implement [`IntoElement`](crate::IntoElement).
/// A component can therefore use it as a type-safe marker and only implement
/// [`RenderOnce`](crate::RenderOnce) once all required slots contain renderable content
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct EmptySlot;

/// type-state storage for content owned by a reusable component.
///
/// an empty slot is represented as:
/// ```
/// # use inkpaper_ui::ComponentSlot;
/// let slot = ComponentSlot::empty();
/// ```
/// filling it changes its type:
/// ```
/// # use inkpaper_ui::{ComponentSlot, div};
/// let slot = ComponentSlot::empty().fill(div());
/// ```
/// `ComponentSlot` has not runtime behavior. It only stores its value and is intended
/// to make typed component builders easier to express
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ComponentSlot<T = EmptySlot> {
    value: T,
}

impl ComponentSlot<EmptySlot> {
    pub const fn empty() -> Self {
        Self { value: EmptySlot }
    }

    pub const fn fill<T>(self, value: T) -> ComponentSlot<T> {
        ComponentSlot { value }
    }
}

impl<T> ComponentSlot<T> {
    pub const fn new(value: T) -> Self {
        Self { value }
    }

    pub fn into_inner(self) -> T {
        self.value
    }

    pub const fn get(&self) -> &T {
        &self.value
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    pub fn map<U>(self, map: impl FnOnce(T) -> U) -> ComponentSlot<U> {
        ComponentSlot::new(map(self.value))
    }
}
