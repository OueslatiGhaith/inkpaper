use crate::{
    Color, MountCx, MountError, NodeId, Pixels,
    text_style::{FontId, TextStylePatch, TextStyled},
};

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
    pub(crate) style: TextStylePatch,
}

impl<'a> Text<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            style: TextStylePatch::default(),
        }
    }
}

pub fn text(value: &str) -> Text<'_> {
    Text::new(value)
}

impl TextStyled for Text<'_> {
    fn font(mut self, font: FontId) -> Self {
        self.style.font = Some(font);
        self
    }

    fn text_color(mut self, color: Color) -> Self {
        self.style.color = Some(color);
        self
    }

    fn line_height(mut self, line_height: Pixels) -> Self {
        self.style.line_height = Some(line_height.non_negative());
        self
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

pub trait ParentElement: Sized {
    type WithChild<E>: ParentElement
    where
        E: IntoElement;

    fn child<E>(self, child: E) -> Self::WithChild<E>
    where
        E: IntoElement;
}
