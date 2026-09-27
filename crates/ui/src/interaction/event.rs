use core::any::TypeId;

use crate::{
    Element, ElementId, IntoElement, Listener, MountCx, MountError, NodeId, ParentElement,
    StatefulInteractiveElement, StatefulInteractivity, Style, Styled, callback::CallbackId,
    element::state::ElementStateId,
};

/// An element was activated. It carries the element's id, so one listener can
/// serve many elements, such as every row of a list or every key of a keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivateEvent {
    id: ElementId,
}

impl ActivateEvent {
    pub const fn new(id: ElementId) -> Self {
        Self { id }
    }

    /// The id of the activated element.
    pub const fn id(self) -> ElementId {
        self.id
    }

    /// The number in the activated element's id, such as 3 for `("row", 3)`.
    pub const fn index(self) -> Option<usize> {
        match self.id {
            ElementId::Value(value) | ElementId::NamedValue(_, value) => Some(value as usize),
            ElementId::Name(_) => None,
        }
    }
}

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

    type WithChildren<I>
        = OnEvent<E::WithChildren<I>, Event>
    where
        I: IntoIterator,
        I::Item: IntoElement;

    fn child<C>(self, child: C) -> Self::WithChild<C>
    where
        C: IntoElement,
    {
        OnEvent {
            element: self.element.child(child),
            listener: self.listener,
        }
    }

    fn children<I>(self, children: I) -> Self::WithChildren<I>
    where
        I: IntoIterator,
        I::Item: IntoElement,
    {
        OnEvent {
            element: self.element.children(children),
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
