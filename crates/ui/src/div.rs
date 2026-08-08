use crate::{
    Children, Element, InteractiveElement, MountCx, MountError, NoChildren, NodeId, ParentElement,
    Push, Style, Styled,
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

impl<C> InteractiveElement for Div<C> where C: Children {}
