use alloc::boxed::Box;

use crate::{Element, IntoElement, MountCx, MountError, NodeId};

/// A type-erased element.
///
/// Lets code pick between elements of different concrete types at runtime, for
/// example a screen chosen through a trait object. Each `AnyElement` costs one
/// heap allocation; prefer [`Either`](crate::Either) when the choice is static.
pub struct AnyElement<'a> {
    element: Box<dyn ErasedElement + 'a>,
}

impl<'a> AnyElement<'a> {
    pub fn new<E>(element: E) -> Self
    where
        E: IntoElement,
        E::Element: 'a,
    {
        Self {
            element: Box::new(element.into_element()),
        }
    }
}

impl Element for AnyElement<'_> {
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        self.element.mount_boxed(cx)
    }
}

trait ErasedElement {
    fn mount_boxed(self: Box<Self>, cx: &mut MountCx<'_>) -> Result<NodeId, MountError>;
}

impl<E> ErasedElement for E
where
    E: Element,
{
    fn mount_boxed(self: Box<Self>, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        (*self).mount(cx)
    }
}
