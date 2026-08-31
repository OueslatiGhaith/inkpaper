use crate::{
    Children, Element, IntoElement, MountCx, MountError, NoChildren, NodeId, ParentElement, Push,
    PushMany, Style, Styled,
};

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

impl<C> Element for Div<C>
where
    C: Children,
{
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        let node = cx.push_div(self.style)?;
        self.children.mount_children(node, cx)?;

        Ok(node)
    }
}

impl<C> Styled for Div<C> {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl<C> ParentElement for Div<C> {
    type WithChild<E>
        = Div<Push<C, E>>
    where
        E: IntoElement;

    type WithChildren<I>
        = Div<PushMany<C, I>>
    where
        I: IntoIterator,
        I::Item: IntoElement;

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

    fn children<I>(self, children: I) -> Self::WithChildren<I>
    where
        I: IntoIterator,
        I::Item: IntoElement,
    {
        Div {
            style: self.style,
            children: PushMany {
                previous: self.children,
                elements: children,
            },
        }
    }
}
