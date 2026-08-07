use crate::{Children, Element, NoChildren, ParentElement, Push, Style, Styled};

pub struct Div<C = NoChildren> {
    pub(crate) style: Style,
    pub(crate) children: C,
}

pub fn div() -> Div<NoChildren> {
    Div {
        style: Style::default(),
        children: NoChildren,
    }
}

impl<C> Element for Div<C> where C: Children {}

impl<C> Styled for Div<C> {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl<C> ParentElement for Div<C> {
    type WithChild<E>
        = Div<Push<C, E>>
    where
        E: crate::IntoElement;

    fn child<E>(self, child: E) -> Self::WithChild<E>
    where
        E: crate::IntoElement,
    {
        Div {
            style: self.style,
            children: Push {
                previous: self.children,
                element: child,
            },
        }
    }
}
