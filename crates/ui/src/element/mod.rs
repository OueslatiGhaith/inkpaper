#[cfg(feature = "alloc")]
mod any;
mod canvas;
mod composition;
mod div;
mod identity;
mod image;
mod paint;
mod render;
pub(crate) mod state;
mod stateful;
mod svg;

#[cfg(feature = "alloc")]
pub use any::*;
pub use canvas::*;
pub use composition::*;
pub use div::*;
pub use identity::*;
pub use image::*;
pub use paint::*;
pub use render::*;
pub use state::IdentityError;
pub use stateful::*;
pub use svg::*;

use crate::{MountCx, MountError, NodeId, TextStyle, TextStyled};

pub trait Element: Sized {
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError>;
}

pub trait IntoElement {
    type Element: Element;

    fn into_element(self) -> Self::Element;

    #[cfg(feature = "alloc")]
    fn into_any_element<'a>(self) -> AnyElement<'a>
    where
        Self: Sized,
        Self::Element: 'a,
    {
        AnyElement::new(self)
    }
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

pub enum TextContent<'a> {
    Borrowed(&'a str),
    #[cfg(feature = "alloc")]
    Owned(alloc::string::String),
}

impl TextContent<'_> {
    pub fn as_str(&self) -> &str {
        match self {
            TextContent::Borrowed(value) => value,
            #[cfg(feature = "alloc")]
            TextContent::Owned(value) => value.as_str(),
        }
    }
}

impl<'a> From<&'a str> for TextContent<'a> {
    fn from(value: &'a str) -> Self {
        Self::Borrowed(value)
    }
}

#[cfg(feature = "alloc")]
impl<'a> From<alloc::string::String> for TextContent<'a> {
    fn from(value: alloc::string::String) -> Self {
        Self::Owned(value)
    }
}

pub struct Text<'a> {
    pub content: TextContent<'a>,
    pub(crate) style: TextStyle,
}

impl<'a> Text<'a> {
    pub fn new(content: impl Into<TextContent<'a>>) -> Self {
        Self {
            content: content.into(),
            style: TextStyle::default(),
        }
    }

    pub fn content(&self) -> &str {
        self.content.as_str()
    }
}

pub fn text<'a>(value: impl Into<TextContent<'a>>) -> Text<'a> {
    Text::new(value)
}

impl TextStyled for Text<'_> {
    fn text_style_mut(&mut self) -> &mut TextStyle {
        &mut self.style
    }
}

impl Element for Text<'_> {
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        cx.push_text(self.content.as_str(), self.style)
    }
}

impl<'a> IntoElement for &'a str {
    type Element = Text<'a>;

    fn into_element(self) -> Self::Element {
        Text::new(self)
    }
}

#[cfg(feature = "alloc")]
impl IntoElement for alloc::string::String {
    type Element = Text<'static>;

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

/// an element that mounts directly via a closure, used by `rsx!` to lower trees and
/// control flow into plain Rust statements without nested builder or `Either` types
#[doc(hidden)]
pub struct MountFn<F>(F);

impl<F> MountFn<F>
where
    F: FnOnce(&mut MountCx<'_>) -> Result<NodeId, MountError>,
{
    pub fn new(f: F) -> Self {
        Self(f)
    }
}

impl<F> Element for MountFn<F>
where
    F: FnOnce(&mut MountCx<'_>) -> Result<NodeId, MountError>,
{
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        (self.0)(cx)
    }
}

/// a child sequence that mounts directly via a closure, used by `rsx!` for component
/// children without nested `Push`/`Either` types
#[doc(hidden)]
pub struct ChildrenFn<F>(F);

impl<F> ChildrenFn<F>
where
    F: FnOnce(NodeId, &mut MountCx<'_>) -> Result<(), MountError>,
{
    pub fn new(f: F) -> Self {
        Self(f)
    }
}

impl<F> Children for ChildrenFn<F>
where
    F: FnOnce(NodeId, &mut MountCx<'_>) -> Result<(), MountError>,
{
    fn mount_children(self, parent: NodeId, cx: &mut MountCx<'_>) -> Result<(), MountError> {
        (self.0)(parent, cx)
    }
}

#[doc(hidden)]
pub fn mount_child<E>(parent: NodeId, child: E, cx: &mut MountCx<'_>) -> Result<(), MountError>
where
    E: IntoElement,
{
    let node = child.into_element().mount(cx)?;
    cx.append_child(parent, node);

    Ok(())
}

#[doc(hidden)]
pub fn append_child(parent: NodeId, child: NodeId, cx: &mut MountCx<'_>) {
    cx.append_child(parent, child);
}

#[doc(hidden)]
pub fn identify_node(
    node: NodeId,
    id: impl IntoElementId,
    cx: &mut MountCx<'_>,
) -> Result<(), MountError> {
    cx.identify(node, id.into_element_id())
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

/// extension for attaching an already-composed child sequence to a parent element
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
