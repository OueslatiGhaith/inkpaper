//! growable storage for runtime tables.

use core::ops::{Deref, DerefMut};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageError {
    /// the storage has no room
    Full,
    /// heap-backed storage could not allocate
    #[expect(
        dead_code,
        reason = "nothing heap-backed is left, and the errors merge next"
    )]
    AllocationFailed,
}

impl StorageError {
    pub(crate) fn or<E>(self, full: E, allocation_failed: E) -> E {
        match self {
            Self::Full => full,
            Self::AllocationFailed => allocation_failed,
        }
    }
}

/// growable storage for runtime tables.
///
/// implemented by the runtime pool's vectors and by fixed-capacity [`heapless::Vec`].
/// Failed operations leave the contents unchanged.
pub trait VecStorage<T>: Deref<Target = [T]> + DerefMut {
    #[cfg(test)]
    fn capacity(&self) -> usize;

    fn clear(&mut self);

    fn truncate(&mut self, len: usize);

    /// ensures `additional` more values can be pushed without failing
    fn try_reserve(&mut self, additional: usize) -> Result<(), StorageError>;

    fn try_push(&mut self, value: T) -> Result<(), StorageError>;

    fn try_extend_from_slice(&mut self, values: &[T]) -> Result<(), StorageError>
    where
        T: Copy;

    fn try_resize(&mut self, len: usize, value: T) -> Result<(), StorageError>
    where
        T: Clone;

    /// releases spare capacity, if the storage can
    fn shrink_to_fit(&mut self) {}
}

impl<T, const N: usize> VecStorage<T> for heapless::Vec<T, N> {
    #[cfg(test)]
    fn capacity(&self) -> usize {
        N
    }

    fn clear(&mut self) {
        heapless::Vec::<T, N>::clear(self);
    }

    fn truncate(&mut self, len: usize) {
        heapless::Vec::<T, N>::truncate(self, len);
    }

    fn try_reserve(&mut self, additional: usize) -> Result<(), StorageError> {
        match self.len().checked_add(additional) {
            Some(required) if required <= N => Ok(()),
            _ => Err(StorageError::Full),
        }
    }

    fn try_push(&mut self, value: T) -> Result<(), StorageError> {
        self.push(value).map_err(|_| StorageError::Full)
    }

    fn try_extend_from_slice(&mut self, values: &[T]) -> Result<(), StorageError>
    where
        T: Copy,
    {
        self.extend_from_slice(values)
            .map_err(|_| StorageError::Full)
    }

    fn try_resize(&mut self, len: usize, value: T) -> Result<(), StorageError>
    where
        T: Clone,
    {
        self.resize(len, value).map_err(|_| StorageError::Full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_storage_reports_full_without_changing_contents() {
        let mut values = heapless::Vec::<u8, 2>::new();

        values.try_extend_from_slice(&[1, 2]).unwrap();

        assert_eq!(values.try_reserve(1), Err(StorageError::Full));
        assert_eq!(values.try_push(3), Err(StorageError::Full));
        assert_eq!(values.try_extend_from_slice(&[3]), Err(StorageError::Full));
        assert_eq!(values.try_resize(3, 0), Err(StorageError::Full));
        assert_eq!(&*values, &[1, 2]);
    }
}
