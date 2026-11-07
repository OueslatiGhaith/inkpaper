use core::ops::{Deref, DerefMut};

use crate::{
    MountError,
    pool::{PoolRef, PoolVec, Table},
};

pub(crate) struct FrameBuffer<T> {
    values: PoolVec<T>,
}

impl<T> FrameBuffer<T> {
    pub(super) fn new_in(pool: PoolRef, table: Table) -> Self {
        Self {
            values: PoolVec::new_in(pool, table),
        }
    }

    #[cfg(test)]
    pub(super) fn capacity(&self) -> usize {
        self.values.capacity()
    }

    pub(super) fn clear(&mut self) {
        self.values.clear();
    }

    pub(super) fn truncate(&mut self, len: usize) {
        self.values.truncate(len);
    }

    pub(super) fn reserve(&mut self, additional: usize) -> Result<(), MountError> {
        self.values
            .try_reserve(additional)
            .map_err(MountError::OutOfMemory)
    }

    pub(super) fn push(&mut self, value: T) -> Result<(), MountError> {
        self.values.try_push(value).map_err(MountError::OutOfMemory)
    }

    pub(super) fn extend_from_slice(&mut self, values: &[T]) -> Result<(), MountError>
    where
        T: Copy,
    {
        self.values
            .try_extend_from_slice(values)
            .map_err(MountError::OutOfMemory)
    }

    pub(super) fn shrink_to_fit(&mut self) {
        self.values.shrink_to_fit();
    }
}

impl<T> Deref for FrameBuffer<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl<T> DerefMut for FrameBuffer<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}
