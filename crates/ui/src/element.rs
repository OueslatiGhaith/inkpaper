pub trait Element: Sized {}

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
}

impl Element for Text<'_> {}

impl<'a> IntoElement for &'a str {
    type Element = Text<'a>;

    fn into_element(self) -> Self::Element {
        Text { text: self }
    }
}

pub struct NoChildren;

pub struct Push<C, E> {
    pub previous: C,
    pub element: E,
}

pub trait Children {}

impl Children for NoChildren {}

impl<C, E> Children for Push<C, E>
where
    C: Children,
    E: IntoElement,
{
}

pub trait ParentElement: Sized {
    type WithChild<E>: ParentElement
    where
        E: IntoElement;

    fn child<E>(self, child: E) -> Self::WithChild<E>
    where
        E: IntoElement;
}
