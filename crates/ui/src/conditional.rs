use crate::{
    Element, IntoElement, MountCx, MountError, NodeId, ParentElement, StatefulInteractiveElement,
    StatefulInteractivity, Style, Styled,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> Element for Either<L, R>
where
    L: IntoElement,
    R: IntoElement,
{
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        match self {
            Either::Left(left) => left.into_element().mount(cx),
            Either::Right(right) => right.into_element().mount(cx),
        }
    }
}

impl<L, R> Styled for Either<L, R>
where
    L: Styled,
    R: Styled,
{
    fn style_mut(&mut self) -> &mut Style {
        match self {
            Either::Left(left) => left.style_mut(),
            Either::Right(right) => right.style_mut(),
        }
    }
}

impl<L, R> ParentElement for Either<L, R>
where
    L: ParentElement,
    R: ParentElement,
{
    type WithChild<E>
        = Either<L::WithChild<E>, R::WithChild<E>>
    where
        E: IntoElement;

    type WithChildren<I>
        = Either<L::WithChildren<I>, R::WithChildren<I>>
    where
        I: IntoIterator,
        I::Item: IntoElement;

    fn child<E>(self, child: E) -> Self::WithChild<E>
    where
        E: IntoElement,
    {
        match self {
            Either::Left(left) => Either::Left(left.child(child)),
            Either::Right(right) => Either::Right(right.child(child)),
        }
    }

    fn children<I>(self, children: I) -> Self::WithChildren<I>
    where
        I: IntoIterator,
        I::Item: IntoElement,
    {
        match self {
            Either::Left(left) => Either::Left(left.children(children)),
            Either::Right(right) => Either::Right(right.children(children)),
        }
    }
}

impl<L, R> StatefulInteractiveElement for Either<L, R>
where
    L: StatefulInteractiveElement,
    R: StatefulInteractiveElement,
{
    fn stateful_interactivity_mut(&mut self) -> &mut StatefulInteractivity {
        match self {
            Either::Left(left) => left.stateful_interactivity_mut(),
            Either::Right(right) => right.stateful_interactivity_mut(),
        }
    }
}

pub trait ConditionalElementExt: IntoElement + Sized {
    fn when<R>(self, condition: bool, transform: impl FnOnce(Self) -> R) -> Either<Self, R>
    where
        R: IntoElement,
    {
        match condition {
            false => Either::Left(self),
            true => Either::Right(transform(self)),
        }
    }

    fn when_some<T, R>(
        self,
        value: Option<T>,
        transform: impl FnOnce(Self, T) -> R,
    ) -> Either<Self, R>
    where
        R: IntoElement,
    {
        match value {
            Some(value) => Either::Right(transform(self, value)),
            None => Either::Left(self),
        }
    }
}

impl<T> ConditionalElementExt for T where T: IntoElement {}
