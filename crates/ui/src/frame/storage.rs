use core::ops::{Deref, DerefMut};

use crate::{
    MountError,
    storage::{DefaultVec, VecStorage},
};

pub(crate) struct FrameBuffer<T, const INITIAL: usize> {
    values: DefaultVec<T, INITIAL>,
}

impl<T, const INITIAL: usize> FrameBuffer<T, INITIAL> {
    pub(super) fn new() -> Self {
        Self {
            values: DefaultVec::default(),
        }
    }

    pub(super) fn capacity(&self) -> usize {
        VecStorage::capacity(&self.values)
    }

    pub(super) fn clear(&mut self) {
        VecStorage::clear(&mut self.values);
    }

    pub(super) fn truncate(&mut self, len: usize) {
        VecStorage::truncate(&mut self.values, len);
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

    #[cfg(feature = "alloc")]
    pub(super) fn shrink_to_fit(&mut self) {
        self.values.shrink_to_fit();
    }
}

impl<T, const INITIAL: usize> Deref for FrameBuffer<T, INITIAL> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl<T, const INITIAL: usize> DerefMut for FrameBuffer<T, INITIAL> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}
