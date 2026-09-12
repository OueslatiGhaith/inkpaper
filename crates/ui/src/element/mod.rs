mod canvas;
mod composition;
mod div;
mod identity;
mod image;
mod paint;
mod render;
pub(crate) mod state;
mod stateful;

use core::marker::PhantomData;

pub use canvas::*;
pub use composition::*;
pub use div::*;
pub use identity::*;
pub use image::*;
pub use paint::*;
pub use render::*;
pub use state::IdentityError;
pub use stateful::*;

use crate::{MountCx, MountError, NodeId, TextStyle, TextStyled};

pub trait Element: Sized {
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError>;
}

pub trait IntoElement {
    type Element: Element;

    fn into_element(self) -> Self::Element;
}

impl<T> IntoElement for T
where
    T: Element,
{
    type Element = T;

    fn into_element(self) -> Self::Element {
        self
    }
}

pub struct Text<'a> {
    pub text: &'a str,
    pub(crate) style: TextStyle,
}

impl<'a> Text<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            style: TextStyle::default(),
        }
    }
}

pub fn text(value: &str) -> Text<'_> {
    Text::new(value)
}

impl TextStyled for Text<'_> {
    fn text_style_mut(&mut self) -> &mut TextStyle {
        &mut self.style
    }
}

impl Element for Text<'_> {
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        cx.push_text(self.text, self.style)
    }
}

impl<'a> IntoElement for &'a str {
    type Element = Text<'a>;

    fn into_element(self) -> Self::Element {
        Text::new(self)
    }
}

pub struct NoChildren;

pub struct Push<C, E> {
    pub previous: C,
    pub element: E,
}

pub struct PushMany<C, I> {
    pub previous: C,
    pub elements: I,
}

#[doc(hidden)]
pub struct PushEach<C, I, F, H, G, E> {
    previous: C,
    items: I,
    render: F,
    fallback: H,
    marker: PhantomData<fn() -> (G, E)>,
}

pub trait Children {
    fn mount_children(self, parent: NodeId, cx: &mut MountCx<'_>) -> Result<(), MountError>;
}

impl Children for NoChildren {
    fn mount_children(self, _: NodeId, _: &mut MountCx<'_>) -> Result<(), MountError> {
        Ok(())
    }
}

impl<C, E> Children for Push<C, E>
where
    C: Children,
    E: IntoElement,
{
    fn mount_children(self, parent: NodeId, cx: &mut MountCx<'_>) -> Result<(), MountError> {
        self.previous.mount_children(parent, cx)?;

        let child = self.element.into_element();
        let child = child.mount(cx)?;

        cx.append_child(parent, child);

        Ok(())
    }
}

impl<C, I> Children for PushMany<C, I>
where
    C: Children,
    I: IntoIterator,
    I::Item: IntoElement,
{
    fn mount_children(self, parent: NodeId, cx: &mut MountCx<'_>) -> Result<(), MountError> {
        self.previous.mount_children(parent, cx)?;

        for element in self.elements {
            let child = element.into_element();
            let child = child.mount(cx)?;
            cx.append_child(parent, child);
        }

        Ok(())
    }
}

impl<C, I, F, H, G, E> Children for PushEach<C, I, F, H, G, E>
where
    C: Children,
    I: IntoIterator,
    F: FnMut(I::Item) -> G,
    H: FnOnce() -> E,
    G: Children,
    E: Children,
{
    fn mount_children(self, parent: NodeId, cx: &mut MountCx<'_>) -> Result<(), MountError> {
        let PushEach {
            previous,
            items,
            mut render,
            fallback,
            ..
        } = self;

        previous.mount_children(parent, cx)?;

        let mut empty = true;

        for item in items {
            empty = false;
            render(item).mount_children(parent, cx)?;
        }

        if empty {
            fallback().mount_children(parent, cx)?;
        }

        Ok(())
    }
}

#[doc(hidden)]
pub trait ChildrenExt: Children + Sized {
    fn child<E>(self, child: E) -> Push<Self, E>
    where
        E: IntoElement,
    {
        Push {
            previous: self,
            element: child,
        }
    }

    fn children<I>(self, children: I) -> PushMany<Self, I>
    where
        I: IntoIterator,
        I::Item: IntoElement,
    {
        PushMany {
            previous: self,
            elements: children,
        }
    }

    fn each<I, F, H, G, E>(self, items: I, render: F, fallback: H) -> PushEach<Self, I, F, H, G, E>
    where
        I: IntoIterator,
        F: FnMut(I::Item) -> G,
        H: FnOnce() -> E,
        G: Children,
        E: Children,
    {
        PushEach {
            previous: self,
            items,
            render,
            fallback,
            marker: PhantomData,
        }
    }
}

impl<C> ChildrenExt for C where C: Children {}

pub trait ParentElement: Sized {
    type WithChild<E>: ParentElement
    where
        E: IntoElement;

    type WithChildren<I>: ParentElement
    where
        I: IntoIterator,
        I::Item: IntoElement;

    fn child<E>(self, child: E) -> Self::WithChild<E>
    where
        E: IntoElement;

    fn children<I>(self, children: I) -> Self::WithChildren<I>
    where
        I: IntoIterator,
        I::Item: IntoElement;
}

#[doc(hidden)]
pub struct AppendChildren<P, C> {
    parent: P,
    children: C,
}

impl<P, C> Element for AppendChildren<P, C>
where
    P: IntoElement,
    C: Children,
{
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        let root = self.parent.into_element().mount(cx)?;

        self.children.mount_children(root, cx)?;

        Ok(root)
    }
}

impl<P, C> ParentElement for AppendChildren<P, C>
where
    P: IntoElement,
    C: Children,
{
    type WithChild<E>
        = AppendChildren<P, Push<C, E>>
    where
        E: IntoElement;

    type WithChildren<I>
        = AppendChildren<P, PushMany<C, I>>
    where
        I: IntoIterator,
        I::Item: IntoElement;

    fn child<E>(self, child: E) -> Self::WithChild<E>
    where
        E: IntoElement,
    {
        AppendChildren {
            parent: self.parent,
            children: Push {
                previous: self.children,
                element: child,
            },
        }
    }

    fn children<I>(self, children: I) -> Self::WithChildren<I>
    where
        I: IntoIterator,
        I::Item: IntoElement,
    {
        AppendChildren {
            parent: self.parent,
            children: PushMany {
                previous: self.children,
                elements: children,
            },
        }
    }
}

#[doc(hidden)]
pub trait ParentElementChildrenExt: ParentElement + IntoElement + Sized {
    fn child_sequence<C>(self, children: C) -> AppendChildren<Self, C>
    where
        C: Children,
    {
        AppendChildren {
            parent: self,
            children,
        }
    }
}

impl<T> ParentElementChildrenExt for T where T: ParentElement + IntoElement {}
