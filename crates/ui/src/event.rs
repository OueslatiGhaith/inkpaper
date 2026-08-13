use core::any::TypeId;

use crate::{
    Element, IntoElement, Listener, MountCx, MountError, NodeId, ParentElement,
    StatefulInteractiveElement, StatefulInteractivity, Style, Styled, callback::CallbackId,
    element_state::ElementStateId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivateEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventTarget {
    element: ElementStateId,
}

impl EventTarget {
    pub(crate) const fn new(element: ElementStateId) -> Self {
        Self { element }
    }

    pub(crate) const fn element(self) -> ElementStateId {
        self.element
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct EventBindingId(u16);

impl EventBindingId {
    pub(crate) const fn new(index: u16) -> Self {
        Self(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct EventBinding {
    pub(crate) event_type: TypeId,
    pub(crate) callback: CallbackId,
    pub(crate) next: Option<EventBindingId>,
}

pub struct OnEvent<E, Event> {
    element: E,
    listener: Listener<Event>,
}

impl<E, Event> OnEvent<E, Event> {
    pub(crate) const fn new(element: E, listener: Listener<Event>) -> Self {
        Self { element, listener }
    }
}

impl<E, Event> Element for OnEvent<E, Event>
where
    E: Element,
    Event: 'static,
{
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        let node = self.element.mount(cx)?;

        cx.bind_event(node, TypeId::of::<Event>(), self.listener.id)?;

        Ok(node)
    }
}

impl<E, Event> Styled for OnEvent<E, Event>
where
    E: Styled,
{
    fn style_mut(&mut self) -> &mut Style {
        self.element.style_mut()
    }
}

impl<E, Event> ParentElement for OnEvent<E, Event>
where
    E: ParentElement,
    Event: 'static,
{
    type WithChild<C>
        = OnEvent<E::WithChild<C>, Event>
    where
        C: IntoElement;

    fn child<C>(self, child: C) -> Self::WithChild<C>
    where
        C: IntoElement,
    {
        OnEvent {
            element: self.element.child(child),
            listener: self.listener,
        }
    }
}

impl<E, Event> StatefulInteractiveElement for OnEvent<E, Event>
where
    E: StatefulInteractiveElement,
    Event: 'static,
{
    fn stateful_interactivity_mut(&mut self) -> &mut StatefulInteractivity {
        self.element.stateful_interactivity_mut()
    }
}

pub(crate) struct EventCallbacks<'a> {
    bindings: &'a [EventBinding],
    next: Option<EventBindingId>,
    event_type: TypeId,
}

impl<'a> EventCallbacks<'a> {
    pub(crate) const fn new(
        bindings: &'a [EventBinding],
        first: Option<EventBindingId>,
        event_type: TypeId,
    ) -> Self {
        Self {
            bindings,
            next: first,
            event_type,
        }
    }
}

impl Iterator for EventCallbacks<'_> {
    type Item = CallbackId;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(id) = self.next {
            let binding = &self.bindings[id.index()];
            self.next = binding.next;
            if binding.event_type == self.event_type {
                return Some(binding.callback);
            }
        }

        None
    }
}
