use core::marker::PhantomData;

use crate::callback::CallbackId;

pub struct Listener<E> {
    pub(crate) id: CallbackId,
    _event: PhantomData<fn(&E)>,
}

impl<E> Listener<E> {
    pub(crate) const fn from_id(id: CallbackId) -> Self {
        Self {
            id,
            _event: PhantomData,
        }
    }
}

impl<E> Copy for Listener<E> {}

impl<E> Clone for Listener<E> {
    fn clone(&self) -> Self {
        *self
    }
}
