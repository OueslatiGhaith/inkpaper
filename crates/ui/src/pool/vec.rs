use core::{
    alloc::Layout,
    marker::PhantomData,
    mem,
    ops::{Deref, DerefMut},
    ptr::{self, NonNull},
};

use super::{OutOfMemory, PoolRef, Table};

/// the fewest values a table makes room for when it first grows
const MIN_CAPACITY: usize = 4;

/// a growable list of values in a [`Pool`](super::Pool).
///
/// it grows by doubling, or to the exact size when doubling doesn't fit. Failed
/// operations leave the contents unchanged.
pub(crate) struct PoolVec<T> {
    pool: PoolRef,
    table: Table,
    ptr: NonNull<T>,
    len: usize,
    capacity: usize,
    marker: PhantomData<T>,
}

impl<T> PoolVec<T> {
    pub(crate) fn new_in(pool: PoolRef, table: Table) -> Self {
        Self {
            pool,
            table,
            ptr: NonNull::dangling(),
            len: 0,
            // zero-sized values never need memory
            capacity: if mem::size_of::<T>() == 0 {
                usize::MAX
            } else {
                0
            },
            marker: PhantomData,
        }
    }

    /// an error for growing to `capacity` values
    fn out_of_memory(&self, capacity: usize) -> OutOfMemory {
        OutOfMemory {
            table: self.table,
            requested: capacity.saturating_mul(mem::size_of::<T>()),
        }
    }

    fn grow_to(&mut self, capacity: usize) -> Result<(), OutOfMemory> {
        let new = Layout::array::<T>(capacity).map_err(|_| self.out_of_memory(capacity))?;

        let ptr = if self.capacity == 0 {
            self.pool.allocate(self.table, new)
        } else {
            // SAFETY: the memory came from this pool for the current capacity, with the
            // same alignment
            unsafe {
                self.pool
                    .reallocate(self.table, self.ptr.cast(), self.layout(), new)
            }
        };

        self.ptr = ptr.ok_or_else(|| self.out_of_memory(capacity))?.cast();
        self.capacity = capacity;

        Ok(())
    }

    fn layout(&self) -> Layout {
        Layout::array::<T>(self.capacity).expect("the current capacity was allocated")
    }

    /// returns the memory to the pool. Only called without values
    fn release(&mut self) {
        debug_assert_eq!(self.len, 0);

        if mem::size_of::<T>() == 0 || self.capacity == 0 {
            return;
        }

        // SAFETY: the memory came from this pool for the current capacity, and nothing
        // points into it once the capacity is gone
        unsafe {
            self.pool
                .deallocate(self.table, self.ptr.cast(), self.layout());
        }
        self.ptr = NonNull::dangling();
        self.capacity = 0;
    }

    #[cfg(test)]
    pub(crate) fn capacity(&self) -> usize {
        self.capacity
    }

    pub(crate) fn clear(&mut self) {
        self.truncate(0);
    }

    pub(crate) fn truncate(&mut self, len: usize) {
        if len >= self.len {
            return;
        }

        let dropped = ptr::slice_from_raw_parts_mut(
            // SAFETY: `len` is within the initialized values
            unsafe { self.ptr.as_ptr().add(len) },
            self.len - len,
        );
        // shorten first, so a panicking drop can't drop a value twice
        self.len = len;

        // SAFETY: the values past `len` were initialized and are no longer reachable
        unsafe { ptr::drop_in_place(dropped) };
    }

    /// ensures `additional` more values can be pushed without failing
    pub(crate) fn try_reserve(&mut self, additional: usize) -> Result<(), OutOfMemory> {
        let required = self
            .len
            .checked_add(additional)
            .ok_or_else(|| self.out_of_memory(usize::MAX))?;
        if required <= self.capacity {
            return Ok(());
        }

        let doubled = self.capacity.saturating_mul(2).max(MIN_CAPACITY);
        if doubled > required && self.grow_to(doubled).is_ok() {
            return Ok(());
        }

        self.grow_to(required)
    }

    pub(crate) fn try_push(&mut self, value: T) -> Result<(), OutOfMemory> {
        self.try_reserve(1)?;

        // SAFETY: the slot after the last value is reserved and uninitialized
        unsafe { self.ptr.as_ptr().add(self.len).write(value) };
        self.len += 1;

        Ok(())
    }

    pub(crate) fn try_extend_from_slice(&mut self, values: &[T]) -> Result<(), OutOfMemory>
    where
        T: Copy,
    {
        self.try_reserve(values.len())?;

        // SAFETY: the slots after the last value are reserved, and `values` can't be
        // inside them
        unsafe {
            ptr::copy_nonoverlapping(
                values.as_ptr(),
                self.ptr.as_ptr().add(self.len),
                values.len(),
            );
        }
        self.len += values.len();

        Ok(())
    }

    pub(crate) fn try_resize(&mut self, len: usize, value: T) -> Result<(), OutOfMemory>
    where
        T: Clone,
    {
        if len <= self.len {
            self.truncate(len);
            return Ok(());
        }

        self.try_reserve(len - self.len)?;
        while self.len < len {
            // SAFETY: the slot after the last value is reserved and uninitialized
            unsafe { self.ptr.as_ptr().add(self.len).write(value.clone()) };
            self.len += 1;
        }

        Ok(())
    }

    /// returns spare capacity to the pool
    pub(crate) fn shrink_to_fit(&mut self) {
        if mem::size_of::<T>() == 0 || self.capacity == self.len {
            return;
        }

        if self.len == 0 {
            self.release();
            return;
        }

        let Ok(new) = Layout::array::<T>(self.len) else {
            return;
        };
        // SAFETY: the memory came from this pool for the current capacity, with the same
        // alignment
        let shrunk = unsafe {
            self.pool
                .reallocate(self.table, self.ptr.cast(), self.layout(), new)
        };

        // keep the larger allocation when it can't move
        if let Some(ptr) = shrunk {
            self.ptr = ptr.cast();
            self.capacity = self.len;
        }
    }
}

impl<T> Deref for PoolVec<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        // SAFETY: the first `len` values are initialized, and the pointer is aligned and
        // non-null even without memory
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<T> DerefMut for PoolVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        // SAFETY: as in `deref`, borrowed exclusively
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<T> Drop for PoolVec<T> {
    fn drop(&mut self) {
        self.clear();
        self.release();
    }
}
