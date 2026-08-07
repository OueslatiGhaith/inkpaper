use crate::{Element, ElementId, IntoElementId, ParentElement, Style, Styled};

pub struct Stateful<E> {
    pub(crate) element: E,
    pub(crate) id: ElementId,
}

impl<E> Stateful<E> {
    pub(crate) fn new(element: E, id: ElementId) -> Self {
        Self { element, id }
    }

    pub fn element_id(&self) -> ElementId {
        self.id
    }
}

impl<E> Element for Stateful<E> where E: Element {}

pub trait InteractiveElement: Element + Sized {
    fn id(self, id: impl IntoElementId) -> Stateful<Self> {
        Stateful::new(self, id.into_element_id())
    }
}

impl<E> Styled for Stateful<E>
where
    E: Styled,
{
    fn style_mut(&mut self) -> &mut Style {
        self.element.style_mut()
    }
}

impl<E> ParentElement for Stateful<E>
where
    E: ParentElement,
{
    type WithChild<C>
        = Stateful<E::WithChild<C>>
    where
        C: crate::prelude::IntoElement;

    fn child<C>(self, child: C) -> Self::WithChild<C>
    where
        C: crate::prelude::IntoElement,
    {
        Stateful::new(self.element.child(child), self.id)
    }
}
