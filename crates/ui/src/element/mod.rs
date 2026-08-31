mod canvas;
mod composition;
mod div;
mod identity;
mod image;
mod render;
pub(crate) mod state;
mod stateful;

pub use canvas::*;
pub use composition::*;
pub use div::*;
pub use identity::*;
pub use image::*;
pub use render::*;
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
