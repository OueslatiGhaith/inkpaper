use core::marker::PhantomData;

#[cfg(feature = "alloc")]
mod allocated;
mod arena;

pub use arena::{CallbackAllocError, CanvasInvokeError, ListenerInvokeError};

pub(crate) use arena::{CallbackStore, register_canvas_callback, register_listener};

#[cfg(feature = "alloc")]
pub(crate) use allocated::CallbackArena;
#[cfg(not(feature = "alloc"))]
pub(crate) use arena::CallbackArena;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CallbackId {
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
