//! runtime storage selection.
//!
//! the traits in this module are public so they can bound [`RuntimeStorage`], but they
//! are not exported. Applications choose among the storage types this crate provides
//! and cannot implement the traits for their own types.

use core::{
    cell::Cell,
    ops::{Deref, DerefMut},
};

use crate::{
    FixedCallbackArena, FixedEntityArena, FixedGlobalArena, Global, GlobalSetError, Listener,
    ListenerInvokeError, callback::CallbackStore, entity::EntityStore, global::GlobalStore,
};
#[cfg(feature = "alloc")]
use crate::{HeapCallbackArena, HeapEntityArena, HeapGlobalArena};

/// chooses the storage behind each runtime table.
///
/// implement it on a marker type and pick each table's storage from the fixed types
/// (`Fixed*`), or with `alloc` the heap types (`Heap*`). Kinds can be mixed:
///
/// ```
/// # use inkpaper_ui::*;
/// struct DeviceStorage;
///
/// impl RuntimeStorage for DeviceStorage {
///     type Entities = FixedEntityArena<4_096, 8>;
///     type Callbacks = FixedCallbackArena<2_048, 16>;
///     type Globals = FixedGlobalArena<256, 4>;
///     type Frame = FixedFrame<96, 2_048>;
///     type ElementStates = FixedElementStates<32>;
/// }
///
/// type DeviceRuntime = Runtime<DeviceStorage>;
/// ```
///
/// [`FixedStorage`] and [`HeapStorage`] cover the common all-fixed and all-heap cases.
pub trait RuntimeStorage {
    type Entities: EntityStorage;
    type Callbacks: CallbackStorage;
    type Globals: GlobalStorage;
    type Frame: FrameStorage;
    type ElementStates: ElementStateStorage;
}

/// fixed-capacity storage for every runtime table, with the capacities in the order the
/// tables appear in [`RuntimeStorage`]. Never allocates.
///
/// nodes with focused or pressed styles get [`FixedFrame`]'s default capacity. Implement
/// [`RuntimeStorage`] directly to choose another.
pub struct FixedStorage<
    const ENTITY_BYTES: usize,
    const ENTITY_SLOTS: usize,
    const CALLBACK_BYTES: usize,
    const CALLBACK_SLOTS: usize,
    const FRAME_NODES: usize,
    const FRAME_TEXT_BYTES: usize,
    const ELEMENT_STATES: usize,
    const GLOBAL_BYTES: usize = 0,
    const GLOBAL_SLOTS: usize = 0,
>;

impl<
    const ENTITY_BYTES: usize,
    const ENTITY_SLOTS: usize,
    const CALLBACK_BYTES: usize,
    const CALLBACK_SLOTS: usize,
    const FRAME_NODES: usize,
    const FRAME_TEXT_BYTES: usize,
    const ELEMENT_STATES: usize,
    const GLOBAL_BYTES: usize,
    const GLOBAL_SLOTS: usize,
> RuntimeStorage
    for FixedStorage<
        ENTITY_BYTES,
        ENTITY_SLOTS,
        CALLBACK_BYTES,
        CALLBACK_SLOTS,
        FRAME_NODES,
        FRAME_TEXT_BYTES,
        ELEMENT_STATES,
        GLOBAL_BYTES,
        GLOBAL_SLOTS,
    >
{
    type Entities = FixedEntityArena<ENTITY_BYTES, ENTITY_SLOTS>;
    type Callbacks = FixedCallbackArena<CALLBACK_BYTES, CALLBACK_SLOTS>;
    type Globals = FixedGlobalArena<GLOBAL_BYTES, GLOBAL_SLOTS>;
    type Frame = FixedFrame<FRAME_NODES, FRAME_TEXT_BYTES>;
    type ElementStates = FixedElementStates<ELEMENT_STATES>;
}

/// heap storage for every runtime table. Tables grow as needed and are limited only by
/// the allocator.
#[cfg(feature = "alloc")]
pub struct HeapStorage;

#[cfg(feature = "alloc")]
impl RuntimeStorage for HeapStorage {
    type Entities = HeapEntityArena;
    type Callbacks = HeapCallbackArena;
    type Globals = HeapGlobalArena;
    type Frame = HeapFrame;
    type ElementStates = HeapElementStates;
}

/// storage for entity state
pub trait EntityStorage: EntityStore + Default {}

/// storage for the callbacks registered while rendering a frame
pub trait CallbackStorage: CallbackStore + Default {
    fn reset(&mut self);

    fn invoke_listener<E>(
        &self,
        listener: Listener<E>,
        event: &E,
        entities: &dyn EntityStore,
        globals: &dyn GlobalStore,
        notified: &Cell<bool>,
    ) -> Result<(), ListenerInvokeError>
    where
        E: 'static;
}

/// storage for application globals
pub trait GlobalStorage: GlobalStore + Default {
    fn set<G>(&mut self, value: G) -> Result<(), GlobalSetError>
    where
        G: Global;

    fn contains<G>(&self) -> bool
    where
        G: Global;

    fn len(&self) -> usize;

    fn used_bytes(&self) -> usize;

    fn capacity(&self) -> usize;

    fn byte_capacity(&self) -> usize;
}

/// storage for frame nodes and their text
pub trait FrameStorage {
    /// sized by node count: nodes, event bindings and node caches
    type Nodes<T>: VecStorage<T>;
    type Text: VecStorage<u8>;
    /// sized by the number of nodes with focused or pressed styles
    type InteractionStyles<T>: VecStorage<T>;
}

/// fixed storage for at most `NODES` frame nodes, `TEXT_BYTES` bytes of text and
/// `INTERACTION_STYLES` nodes with focused or pressed styles
pub struct FixedFrame<
    const NODES: usize,
    const TEXT_BYTES: usize,
    const INTERACTION_STYLES: usize = 32,
>;

impl<const NODES: usize, const TEXT_BYTES: usize, const INTERACTION_STYLES: usize> FrameStorage
    for FixedFrame<NODES, TEXT_BYTES, INTERACTION_STYLES>
{
    type Nodes<T> = heapless::Vec<T, NODES>;
    type Text = heapless::Vec<u8, TEXT_BYTES>;
    type InteractionStyles<T> = heapless::Vec<T, INTERACTION_STYLES>;
}

/// heap storage for frame nodes and text. The first allocations reserve the initial
/// counts.
#[cfg(feature = "alloc")]
pub struct HeapFrame<const INITIAL_NODES: usize = 0, const INITIAL_TEXT_BYTES: usize = 0>;

#[cfg(feature = "alloc")]
impl<const INITIAL_NODES: usize, const INITIAL_TEXT_BYTES: usize> FrameStorage
    for HeapFrame<INITIAL_NODES, INITIAL_TEXT_BYTES>
{
    type Nodes<T> = HeapVec<T, INITIAL_NODES>;
    type Text = HeapVec<u8, INITIAL_TEXT_BYTES>;
    type InteractionStyles<T> = HeapVec<T, 0>;
}

/// storage for element identities and the state kept for them, such as scroll offsets
pub trait ElementStateStorage {
    type Slots<T>: VecStorage<T>;
}

/// fixed storage for at most `SLOTS` element identities
pub struct FixedElementStates<const SLOTS: usize>;

impl<const SLOTS: usize> ElementStateStorage for FixedElementStates<SLOTS> {
    type Slots<T> = heapless::Vec<T, SLOTS>;
}

/// heap storage for element identities. The first allocation reserves `INITIAL_SLOTS`.
#[cfg(feature = "alloc")]
pub struct HeapElementStates<const INITIAL_SLOTS: usize = 0>;

#[cfg(feature = "alloc")]
impl<const INITIAL_SLOTS: usize> ElementStateStorage for HeapElementStates<INITIAL_SLOTS> {
    type Slots<T> = HeapVec<T, INITIAL_SLOTS>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageError {
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
pub trait VecStorage<T>: Default + Deref<Target = [T]> + DerefMut {
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
pub struct HeapVec<T, const INITIAL: usize> {
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

    fn shrink_to_fit(&mut self) {
        self.values.shrink_to_fit();
    }
}

// unit tests run against the storage the `alloc` feature implies, so both builds keep
// covering their own storage kind
#[cfg(all(test, not(feature = "alloc")))]
pub(crate) type TestFrame<const NODES: usize, const TEXT_BYTES: usize> =
    FixedFrame<NODES, TEXT_BYTES>;
#[cfg(all(test, feature = "alloc"))]
pub(crate) type TestFrame<const NODES: usize, const TEXT_BYTES: usize> =
    HeapFrame<NODES, TEXT_BYTES>;

#[cfg(all(test, not(feature = "alloc")))]
pub(crate) type TestStorage<
    const ENTITY_BYTES: usize,
    const ENTITY_SLOTS: usize,
    const CALLBACK_BYTES: usize,
    const CALLBACK_SLOTS: usize,
    const FRAME_NODES: usize,
    const FRAME_TEXT_BYTES: usize,
    const ELEMENT_STATES: usize,
    const GLOBAL_BYTES: usize = 0,
    const GLOBAL_SLOTS: usize = 0,
> = FixedStorage<
    ENTITY_BYTES,
    ENTITY_SLOTS,
    CALLBACK_BYTES,
    CALLBACK_SLOTS,
    FRAME_NODES,
    FRAME_TEXT_BYTES,
    ELEMENT_STATES,
    GLOBAL_BYTES,
    GLOBAL_SLOTS,
>;
#[cfg(all(test, feature = "alloc"))]
pub(crate) type TestStorage<
    const ENTITY_BYTES: usize,
    const ENTITY_SLOTS: usize,
    const CALLBACK_BYTES: usize,
    const CALLBACK_SLOTS: usize,
    const FRAME_NODES: usize,
    const FRAME_TEXT_BYTES: usize,
    const ELEMENT_STATES: usize,
    const GLOBAL_BYTES: usize = 0,
    const GLOBAL_SLOTS: usize = 0,
> = HeapStorage;

#[cfg(all(test, not(feature = "alloc")))]
pub(crate) type TestElementStates<const SLOTS: usize> = FixedElementStates<SLOTS>;
#[cfg(all(test, feature = "alloc"))]
pub(crate) type TestElementStates<const SLOTS: usize> = HeapElementStates<SLOTS>;

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
