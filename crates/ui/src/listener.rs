use core::marker::PhantomData;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ListenerId {
    slot: u16,
    generation: u32,
}

impl ListenerId {
    pub(crate) const fn new(slot: u16, generation: u32) -> Self {
        Self { slot, generation }
    }

    pub(crate) const fn slot(self) -> u16 {
        self.slot
    }

    pub(crate) const fn generation(self) -> u32 {
        self.generation
    }
}

pub struct Listener<E> {
    pub(crate) id: ListenerId,
    _event: PhantomData<fn(&E)>,
}

impl<E> Listener<E> {
    pub(crate) const fn from_id(id: ListenerId) -> Self {
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
