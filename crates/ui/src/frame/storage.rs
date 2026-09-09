use core::ops::{Deref, DerefMut};

use crate::MountError;

pub(crate) struct FrameBuffer<T, const INITIAL: usize> {
    #[cfg(feature = "alloc")]
    values: alloc::vec::Vec<T>,
    #[cfg(not(feature = "alloc"))]
    values: heapless::Vec<T, INITIAL>,
}

impl<T, const INITIAL: usize> FrameBuffer<T, INITIAL> {
    pub(super) const fn new() -> Self {
        Self {
            #[cfg(feature = "alloc")]
            values: alloc::vec::Vec::new(),
            #[cfg(not(feature = "alloc"))]
            values: heapless::Vec::new(),
        }
    }

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
        let required = self.len().checked_add(additional).ok_or(full)?;

        #[cfg(not(feature = "alloc"))]
        if required > INITIAL {
            return Err(full);
        }

        #[cfg(feature = "alloc")]
        if required > self.capacity() {
            let target = if self.capacity() == 0 {
                required.max(INITIAL)
            } else {
                required
            };

            self.values
                .try_reserve(target - self.len())
                .map_err(|_| MountError::AllocationFailed)?;
        }

        Ok(())
    }

    pub(super) fn push(&mut self, value: T, full: MountError) -> Result<(), MountError> {
        self.reserve(1, full)?;

        #[cfg(feature = "alloc")]
        self.values.push(value);

        #[cfg(not(feature = "alloc"))]
        self.values.push(value).map_err(|_| full)?;

        Ok(())
    }

    pub(super) fn extend_from_slice(
        &mut self,
        values: &[T],
        full: MountError,
    ) -> Result<(), MountError>
    where
        T: Copy,
    {
        self.reserve(values.len(), full)?;

        #[cfg(feature = "alloc")]
        self.values.extend_from_slice(values);

        #[cfg(not(feature = "alloc"))]
        self.values.extend_from_slice(values).map_err(|_| full)?;

        Ok(())
    }

    #[cfg(feature = "alloc")]
    pub(super) fn shrink_to_fit(&mut self) {
        self.values.shrink_to_fit();
    }
}

impl<T, const INITIAL: usize> Deref for FrameBuffer<T, INITIAL> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.values.as_slice()
    }
}

impl<T, const INITIAL: usize> DerefMut for FrameBuffer<T, INITIAL> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.values.as_mut_slice()
    }
}
