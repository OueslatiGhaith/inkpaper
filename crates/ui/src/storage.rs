use core::ops::{Deref, DerefMut};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StorageError {
    /// fixed storage reached its capacity
    Full,
    /// heap-backed storage could not allocate
    #[cfg_attr(not(feature = "alloc"), allow(dead_code))]
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
/// implemented by fixed-capacity [`heapless::Vec`] and, with `alloc`, by [`HeapVec`].
/// Failed operations leave the contents unchanged.
pub(crate) trait VecStorage<T>: Default + Deref<Target = [T]> + DerefMut {
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
}

impl<T, const N: usize> VecStorage<T> for heapless::Vec<T, N> {
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

/// heap-backed [`VecStorage`] without a capacity limit.
///
/// the first allocation reserves at least `INITIAL` values, so small tables don't grow
/// one value at a time.
#[cfg(feature = "alloc")]
pub(crate) struct HeapVec<T, const INITIAL: usize> {
    values: alloc::vec::Vec<T>,
}

#[cfg(feature = "alloc")]
impl<T, const INITIAL: usize> Default for HeapVec<T, INITIAL> {
    fn default() -> Self {
        Self {
            values: alloc::vec::Vec::new(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<T, const INITIAL: usize> Deref for HeapVec<T, INITIAL> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

#[cfg(feature = "alloc")]
impl<T, const INITIAL: usize> DerefMut for HeapVec<T, INITIAL> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}

#[cfg(feature = "alloc")]
impl<T, const INITIAL: usize> VecStorage<T> for HeapVec<T, INITIAL> {
    fn capacity(&self) -> usize {
        self.values.capacity()
    }

    fn clear(&mut self) {
        self.values.clear();
    }

    fn truncate(&mut self, len: usize) {
        self.values.truncate(len);
    }

    fn try_reserve(&mut self, additional: usize) -> Result<(), StorageError> {
        let required = self
            .values
            .len()
            .checked_add(additional)
            .ok_or(StorageError::Full)?;

        if required <= self.values.capacity() {
            return Ok(());
        }

        let target = if self.values.capacity() == 0 {
            required.max(INITIAL)
        } else {
            required
        };

        self.values
            .try_reserve(target - self.values.len())
            .map_err(|_| StorageError::AllocationFailed)
    }

    fn try_push(&mut self, value: T) -> Result<(), StorageError> {
        self.try_reserve(1)?;
        self.values.push(value);

        Ok(())
    }

    fn try_extend_from_slice(&mut self, values: &[T]) -> Result<(), StorageError>
    where
        T: Copy,
    {
        self.try_reserve(values.len())?;
        self.values.extend_from_slice(values);

        Ok(())
    }

    fn try_resize(&mut self, len: usize, value: T) -> Result<(), StorageError>
    where
        T: Clone,
    {
        self.try_reserve(len.saturating_sub(self.values.len()))?;
        self.values.resize(len, value);

        Ok(())
    }
}

#[cfg(feature = "alloc")]
impl<T, const INITIAL: usize> HeapVec<T, INITIAL> {
    pub(crate) fn shrink_to_fit(&mut self) {
        self.values.shrink_to_fit();
    }
}

/// the storage selected by the `alloc` feature
#[cfg(not(feature = "alloc"))]
pub(crate) type DefaultVec<T, const N: usize> = heapless::Vec<T, N>;
#[cfg(feature = "alloc")]
pub(crate) type DefaultVec<T, const N: usize> = HeapVec<T, N>;

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

    #[cfg(feature = "alloc")]
    #[test]
    fn heap_storage_reserves_initial_capacity_once() {
        let mut values = HeapVec::<u8, 8>::default();

        values.try_push(1).unwrap();
        assert!(VecStorage::capacity(&values) >= 8);

        values.try_resize(20, 0).unwrap();
        assert_eq!(values.len(), 20);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn heap_storage_reports_allocation_failure_without_changing_contents() {
        let mut values = HeapVec::<u8, 0>::default();

        values.try_push(1).unwrap();

        assert_eq!(
            values.try_reserve(usize::MAX - 1),
            Err(StorageError::AllocationFailed)
        );
        assert_eq!(
            values.try_resize(usize::MAX, 0),
            Err(StorageError::AllocationFailed)
        );
        assert_eq!(values.try_reserve(usize::MAX), Err(StorageError::Full));
        assert_eq!(&*values, &[1]);
    }
}
