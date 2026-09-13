use crate::{
    Children, Element, IntoElement, MountCx, MountError, NodeId, ParentElement,
    StatefulInteractiveElement, StatefulInteractivity, Style, Styled,
};

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

pub trait ComponentChildren<C>: Sized
where
    C: Children,
{
    type WithChildren: IntoElement;

    fn with_children(self, children: C) -> Self::WithChildren;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> Element for Either<L, R>
where
    L: IntoElement,
    R: IntoElement,
{
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        match self {
            Either::Left(left) => left.into_element().mount(cx),
            Either::Right(right) => right.into_element().mount(cx),
        }
    }
}

impl<L, R> Styled for Either<L, R>
where
    L: Styled,
    R: Styled,
{
    fn style_mut(&mut self) -> &mut Style {
        match self {
            Either::Left(left) => left.style_mut(),
            Either::Right(right) => right.style_mut(),
        }
    }
}

impl<L, R> Children for Either<L, R>
where
    L: Children,
    R: Children,
{
    fn mount_children(self, parent: NodeId, cx: &mut MountCx<'_>) -> Result<(), MountError> {
        match self {
            Either::Left(left) => left.mount_children(parent, cx),
            Either::Right(right) => right.mount_children(parent, cx),
        }
    }
}

impl<L, R> ParentElement for Either<L, R>
where
    L: ParentElement,
    R: ParentElement,
{
    type WithChild<E>
        = Either<L::WithChild<E>, R::WithChild<E>>
    where
        E: IntoElement;

    type WithChildren<I>
        = Either<L::WithChildren<I>, R::WithChildren<I>>
    where
        I: IntoIterator,
        I::Item: IntoElement;

    fn child<E>(self, child: E) -> Self::WithChild<E>
    where
        E: IntoElement,
    {
        match self {
            Either::Left(left) => Either::Left(left.child(child)),
            Either::Right(right) => Either::Right(right.child(child)),
        }
    }

    fn children<I>(self, children: I) -> Self::WithChildren<I>
    where
        I: IntoIterator,
        I::Item: IntoElement,
    {
        match self {
            Either::Left(left) => Either::Left(left.children(children)),
            Either::Right(right) => Either::Right(right.children(children)),
        }
    }
}

impl<L, R> StatefulInteractiveElement for Either<L, R>
where
    L: StatefulInteractiveElement,
    R: StatefulInteractiveElement,
{
    fn stateful_interactivity_mut(&mut self) -> &mut StatefulInteractivity {
        match self {
            Either::Left(left) => left.stateful_interactivity_mut(),
            Either::Right(right) => right.stateful_interactivity_mut(),
        }
    }
}

pub trait ConditionalElementExt: IntoElement + Sized {
    fn when<R>(self, condition: bool, transform: impl FnOnce(Self) -> R) -> Either<Self, R>
    where
        R: IntoElement,
    {
        match condition {
            false => Either::Left(self),
            true => Either::Right(transform(self)),
        }
    }

    fn when_some<T, R>(
        self,
        value: Option<T>,
        transform: impl FnOnce(Self, T) -> R,
    ) -> Either<Self, R>
    where
        R: IntoElement,
    {
        match value {
            Some(value) => Either::Right(transform(self, value)),
            None => Either::Left(self),
        }
    }
}

impl<T> ConditionalElementExt for T where T: IntoElement {}
