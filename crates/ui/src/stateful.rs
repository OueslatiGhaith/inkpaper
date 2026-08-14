use crate::{
    ActivateEvent, Element, ElementId, InteractionStyle, IntoElement, IntoElementId, Listener,
    MountCx, MountError, NodeId, OnEvent, ParentElement, Style, StylePatch, Styled,
    scroll::ScrollAxes,
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

        cx.identify(node, self.id);
        cx.apply_interactivity(node, self.stateful_interactivity);

        Ok(node)
    }
}

pub trait IdentifiableElementExt: Element {
    fn id(self, id: impl IntoElementId) -> Stateful<Self> {
        Stateful::new(self, id.into_element_id())
    }
}

impl<E> IdentifiableElementExt for E where E: Element {}

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
        C: IntoElement;

    type WithChildren<I>
        = Stateful<E::WithChildren<I>>
    where
        I: IntoIterator,
        I::Item: IntoElement;

    fn child<C>(self, child: C) -> Self::WithChild<C>
    where
        C: IntoElement,
    {
        Stateful {
            id: self.id,
            element: self.element.child(child),
            stateful_interactivity: self.stateful_interactivity,
        }
    }

    fn children<I>(self, children: I) -> Self::WithChildren<I>
    where
        I: IntoIterator,
        I::Item: IntoElement,
    {
        Stateful {
            id: self.id,
            element: self.element.children(children),
            stateful_interactivity: self.stateful_interactivity,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StatefulInteractivity {
    pub(crate) focusable: bool,
    pub(crate) focused_style: StylePatch,
    pub(crate) pressed_style: StylePatch,
    pub(crate) scroll_axes: ScrollAxes,
}

pub trait StatefulInteractiveElement: Element + Sized {
    fn stateful_interactivity_mut(&mut self) -> &mut StatefulInteractivity;
}

impl<E> StatefulInteractiveElement for Stateful<E>
where
    E: Element,
{
    fn stateful_interactivity_mut(&mut self) -> &mut StatefulInteractivity {
        &mut self.stateful_interactivity
    }
}

pub trait StatefulInteractiveElementExt: StatefulInteractiveElement {
    fn on<Event>(self, listener: Listener<Event>) -> OnEvent<Self, Event>
    where
        Event: 'static,
    {
        OnEvent::new(self, listener)
    }

    fn on_activate(mut self, listener: Listener<ActivateEvent>) -> OnEvent<Self, ActivateEvent> {
        self.stateful_interactivity_mut().focusable = true;
        OnEvent::new(self, listener)
    }

    fn focusable(mut self) -> Self {
        self.stateful_interactivity_mut().focusable = true;
        self
    }

    fn when_focused<F>(mut self, transform: F) -> Self
    where
        Self: Styled,
        F: FnOnce(InteractionStyle) -> InteractionStyle,
    {
        let base = *self.style_mut();
        let variant = transform(InteractionStyle::new(base)).into_style();
        let patch = StylePatch::between(base, variant);
        let interaction = self.stateful_interactivity_mut();
        interaction.focused_style = interaction.focused_style.merge(patch);

        self
    }

    fn when_pressed<F>(mut self, transform: F) -> Self
    where
        Self: Styled,
        F: FnOnce(InteractionStyle) -> InteractionStyle,
    {
        let base = *self.style_mut();
        let variant = transform(InteractionStyle::new(base)).into_style();
        let patch = StylePatch::between(base, variant);
        let interaction = self.stateful_interactivity_mut();
        interaction.pressed_style = interaction.pressed_style.merge(patch);

        self
    }

    fn overflow_x_scroll(mut self) -> Self
    where
        Self: Styled,
    {
        self.style_mut().clip_children = true;
        self.stateful_interactivity_mut().scroll_axes = ScrollAxes::Horizontal;
        self
    }

    fn overflow_y_scroll(mut self) -> Self
    where
        Self: Styled,
    {
        self.style_mut().clip_children = true;
        self.stateful_interactivity_mut().scroll_axes = ScrollAxes::Vertical;
        self
    }

    fn overflow_scroll(mut self) -> Self
    where
        Self: Styled,
    {
        self.style_mut().clip_children = true;
        self.stateful_interactivity_mut().scroll_axes = ScrollAxes::Both;
        self
    }
}

impl<T> StatefulInteractiveElementExt for T where T: StatefulInteractiveElement {}
