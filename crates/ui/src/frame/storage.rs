use core::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use crate::{MountError, storage::VecStorage};

pub(crate) struct FrameBuffer<T, S> {
    values: S,
    marker: PhantomData<fn() -> T>,
}

impl<T, S> FrameBuffer<T, S>
where
    S: VecStorage<T>,
{
    pub(super) fn new() -> Self {
        Self {
            values: S::default(),
            marker: PhantomData,
        }
    }

    #[cfg(all(test, feature = "alloc"))]
    pub(super) fn capacity(&self) -> usize {
        self.values.capacity()
    }

    pub(super) fn clear(&mut self) {
        self.values.clear();
    }

    pub(super) fn truncate(&mut self, len: usize) {
        self.values.truncate(len);
    }

    pub(super) fn reserve(
        &mut self,
        additional: usize,
        full: MountError,
    ) -> Result<(), MountError> {
        self.values
            .try_reserve(additional)
            .map_err(|error| error.or(full, MountError::AllocationFailed))
    }

    pub(super) fn push(&mut self, value: T, full: MountError) -> Result<(), MountError> {
        self.values
            .try_push(value)
            .map_err(|error| error.or(full, MountError::AllocationFailed))
    }

    pub(super) fn extend_from_slice(
        &mut self,
        values: &[T],
        full: MountError,
    ) -> Result<(), MountError>
    where
        T: Copy,
    {
        self.values
            .try_extend_from_slice(values)
            .map_err(|error| error.or(full, MountError::AllocationFailed))
    }

    pub(super) fn shrink_to_fit(&mut self) {
        self.values.shrink_to_fit();
    }
}

impl<T, S> Deref for FrameBuffer<T, S>
where
    S: VecStorage<T>,
{
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl<T, S> DerefMut for FrameBuffer<T, S>
where
    S: VecStorage<T>,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}
