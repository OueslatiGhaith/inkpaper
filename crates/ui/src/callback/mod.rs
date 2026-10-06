use core::marker::PhantomData;

mod arena;

#[cfg(test)]
pub(crate) use arena::CanvasInvokeError;
#[cfg(feature = "alloc")]
pub use arena::HeapCallbackArena;
pub use arena::{CallbackAllocError, FixedCallbackArena, ListenerInvokeError};
pub(crate) use arena::{CallbackMark, CallbackStore, register_canvas_callback, register_listener};

// unit tests run against the storage the `alloc` feature implies
#[cfg(all(test, not(feature = "alloc")))]
pub(crate) type TestCallbackArena<const BYTES: usize, const SLOTS: usize> =
    FixedCallbackArena<BYTES, SLOTS>;
#[cfg(all(test, feature = "alloc"))]
pub(crate) type TestCallbackArena<const BYTES: usize, const SLOTS: usize> =
    HeapCallbackArena<SLOTS>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallbackId {
    slot: u16,
    generation: u32,
}

impl CallbackId {
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
