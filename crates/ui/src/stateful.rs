use crate::{
    ClickEvent, Element, ElementId, IntoElementId, Listener, ListenerId, MountCx, MountError,
    NodeId, ParentElement, Style, Styled,
};

pub struct Stateful<E> {
    element: E,
    id: ElementId,
    stateful_interactivity: StatefulInteractivity,
}

impl<E> Stateful<E> {
    pub(crate) fn new(element: E, id: ElementId) -> Self {
        Self {
            element,
            id,
            stateful_interactivity: StatefulInteractivity::default(),
        }
    }

    pub fn element_id(&self) -> ElementId {
        self.id
    }
}

impl<E> Element for Stateful<E>
where
    E: Element,
{
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        let node = self.element.mount(cx)?;
        cx.make_stateful(node, self.id, self.stateful_interactivity.into());

        Ok(node)
    }
}

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
        Stateful {
            id: self.id,
            element: self.element.child(child),
            stateful_interactivity: self.stateful_interactivity,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StatefulInteractivity {
    pub(crate) click: Option<ListenerId>,
}

pub trait StatefulInteractiveElement: Element + Sized {
    fn stateful_interactivity_mut(&mut self) -> &mut StatefulInteractivity;
}

impl<E> StatefulInteractiveElement for Stateful<E>
where
    E: InteractiveElement,
{
    fn stateful_interactivity_mut(&mut self) -> &mut StatefulInteractivity {
        &mut self.stateful_interactivity
    }
}

pub trait StatefulInteractiveElementExt: StatefulInteractiveElement {
    fn on_click(mut self, listener: Listener<ClickEvent>) -> Self {
        self.stateful_interactivity_mut().click = Some(listener.id);
        self
    }
}

impl<T> StatefulInteractiveElementExt for T where T: StatefulInteractiveElement {}
