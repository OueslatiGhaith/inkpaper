//! the byte pool the runtime keeps its tables in.
//!
//! a fixed [`Pool`] (`Pool::new`) takes a static buffer of bytes from the application and
//! hands out memory from it through a TLSF allocator, so allocation and release take
//! constant time and work without a global allocator. Under the `alloc` feature,
//! [`Pool::global`] instead delegates directly to the global allocator so freed memory
//! returns immediately without reserving or hoarding fixed chunks.
//!
//! SAFETY INVARIANTS:
//!
//! 1. a [`PoolRef`] is only used while the [`Pool`] it came from exists. The runtime
//!    drops every table before its pool.
//! 2. for a fixed pool, the allocator is only borrowed inside the methods of [`PoolRef`],
//!    which never run other code meanwhile, so its borrows never overlap.

use core::{
    alloc::Layout,
    cell::{Cell, UnsafeCell},
    mem::{self, MaybeUninit},
    ptr::NonNull,
};

use rlsf::{GRANULARITY, Tlsf};

mod vec;

#[cfg(test)]
mod tests;

pub(crate) use vec::PoolVec;

/// with 8 second-level lists per first-level one
const SECOND_LEVELS: usize = 8;
/// the largest single allocation is `(GRANULARITY << FIRST_LEVELS) - GRANULARITY`
/// bytes: 16 MiB with 32-bit pointers
const FIRST_LEVELS: usize = 20;

type Allocator = Tlsf<'static, u32, u8, FIRST_LEVELS, SECOND_LEVELS>;

/// the header the allocator puts before each allocation
const ALLOCATION_HEADER: usize = GRANULARITY / 2;

/// the runtime tables that take memory from a [`Pool`]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Table {
    /// frame nodes and the tables kept per node
    Nodes,
    /// the text of the frame's text nodes
    Text,
    /// what each entity read during its last render
    Dependencies,
    /// element identities and the state kept for them
    ElementStates,
    /// event subscriptions and the events waiting for delivery
    Events,
    Entities,
    Callbacks,
    Globals,
}

impl Table {
    const COUNT: usize = 8;

    /// every table, in declaration order
    pub const ALL: [Self; Self::COUNT] = [
        Self::Nodes,
        Self::Text,
        Self::Dependencies,
        Self::ElementStates,
        Self::Events,
        Self::Entities,
        Self::Callbacks,
        Self::Globals,
    ];

    const fn index(self) -> usize {
        self as usize
    }
}

/// a runtime table needed more memory than its [`Pool`] had left.
///
/// [`MemoryStats`] tells how much of the pool each table uses, to size it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct OutOfMemory {
    /// the table that asked
    pub table: Table,
    /// the bytes it asked for
    pub requested: usize,
}

/// how much of a [`Pool`] the runtime uses.
///
/// for fixed pools, byte counts include the allocator's header and rounding for each
/// allocation, so `used` reaching `capacity` is when allocations fail. For
/// [`Pool::global`], `capacity` is `None` and byte counts track requested layout sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryStats {
    /// bytes a fixed pool can hand out after the allocator's own state, or `None` for
    /// [`Pool::global`]
    pub capacity: Option<usize>,
    /// bytes in use now
    pub used: usize,
    /// the most bytes in use at once
    pub peak: usize,
    /// allocations made so far, growing a table included
    pub allocations: usize,
    by_table: [usize; Table::COUNT],
}

impl MemoryStats {
    const fn new(capacity: Option<usize>) -> Self {
        Self {
            capacity,
            used: 0,
            peak: 0,
            allocations: 0,
            by_table: [0; Table::COUNT],
        }
    }

    /// bytes `table` uses now
    pub const fn table(&self, table: Table) -> usize {
        self.by_table[table.index()]
    }

    fn add(&mut self, table: Table, bytes: usize) {
        self.used += bytes;
        self.peak = self.peak.max(self.used);
        self.by_table[table.index()] += bytes;
    }

    fn remove(&mut self, table: Table, bytes: usize) {
        self.used -= bytes;
        self.by_table[table.index()] -= bytes;
    }
}

#[repr(C)]
struct PoolHeader {
    stats: Cell<MemoryStats>,
    #[cfg(feature = "alloc")]
    is_global: bool,
}

#[repr(C)]
struct FixedPoolHeader {
    common: PoolHeader,
    allocator: UnsafeCell<Allocator>,
}

/// the bytes the runtime keeps its tables in.
///
/// a pool made with [`Pool::new`] has a fixed size backed by an internal TLSF allocator:
/// tables that need more than it holds fail with [`OutOfMemory`]. Under the `alloc`
/// feature, [`Pool::global`] delegates directly to the global allocator.
///
/// ```
/// # use core::mem::MaybeUninit;
/// # use inkpaper_ui::Pool;
/// static mut BYTES: [MaybeUninit<u8>; 16 * 1024] = [MaybeUninit::uninit(); 16 * 1024];
///
/// // SAFETY: nothing else uses `BYTES`
/// let pool = Pool::new(unsafe { &mut *core::ptr::addr_of_mut!(BYTES) });
/// assert!(pool.stats().capacity.unwrap() > 14 * 1024);
/// ```
pub struct Pool {
    header: NonNull<PoolHeader>,
}

impl Pool {
    /// a pool over `memory`. A few hundred bytes of it hold the allocator's state.
    ///
    /// # Panics
    ///
    /// when `memory` is too small to hold the allocator's state and an allocation
    pub fn new(memory: &'static mut [MaybeUninit<u8>]) -> Self {
        let memory = NonNull::from(memory);
        let start = memory.cast::<u8>();
        let len = memory.len();

        let offset = start.align_offset(mem::align_of::<FixedPoolHeader>());
        let blocks_start = offset
            .checked_add(mem::size_of::<FixedPoolHeader>())
            .filter(|&end| end < len)
            .unwrap_or_else(|| panic!("a pool of {len} bytes can't hold its allocator"));

        // SAFETY: the header and the block after it are inside `memory`, which is
        // borrowed for `'static` and exclusively owned by this pool. The header is aligned
        let (fixed, blocks) = unsafe {
            let fixed = start.add(offset).cast::<FixedPoolHeader>();
            fixed.write(FixedPoolHeader {
                common: PoolHeader {
                    stats: Cell::new(MemoryStats::new(None)),
                    #[cfg(feature = "alloc")]
                    is_global: false,
                },
                allocator: UnsafeCell::new(Allocator::new()),
            });

            let blocks = NonNull::slice_from_raw_parts(start.add(blocks_start), len - blocks_start);

            (fixed, blocks)
        };

        // SAFETY: the block lives as long as the allocator, which is stored in front of
        // it, and nothing else borrows the allocator yet
        let capacity = unsafe {
            let allocator = &raw const (*fixed.as_ptr()).allocator;
            (*(*allocator).get()).insert_free_block_ptr(blocks)
        }
        .unwrap_or_else(|| panic!("a pool of {len} bytes can't hold an allocation"));

        // SAFETY: just written; pointer-to-pointer cast preserves provenance over the
        // full `FixedPoolHeader`
        let header = fixed.cast::<PoolHeader>();
        unsafe {
            (*header.as_ptr())
                .stats
                .set(MemoryStats::new(Some(capacity.get())));
        }

        Self { header }
    }

    /// a pool that allocates directly from the global allocator, returning freed memory
    /// immediately while still tracking [`MemoryStats`] per table
    #[cfg(feature = "alloc")]
    pub fn global() -> Self {
        let header = alloc::boxed::Box::new(PoolHeader {
            stats: Cell::new(MemoryStats::new(None)),
            is_global: true,
        });
        let header = NonNull::from(alloc::boxed::Box::leak(header));

        Self { header }
    }

    pub fn stats(&self) -> MemoryStats {
        self.handle().stats()
    }

    /// a handle the tables allocate through. See the module's safety invariants
    pub(crate) fn handle(&self) -> PoolRef {
        PoolRef {
            header: self.header,
        }
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        #[cfg(feature = "alloc")]
        if self.handle().is_global() {
            // SAFETY: `global` leaked this `Box<PoolHeader>` for the pool, and no
            // table outlives the pool
            drop(unsafe { alloc::boxed::Box::from_raw(self.header.as_ptr()) });
        }
    }
}

/// a handle to a [`Pool`], copied into each table that allocates from it
#[derive(Clone, Copy)]
pub(crate) struct PoolRef {
    header: NonNull<PoolHeader>,
}

impl PoolRef {
    fn stats_cell(&self) -> &Cell<MemoryStats> {
        // SAFETY: the pool outlives its handles
        unsafe { &(*self.header.as_ptr()).stats }
    }

    #[cfg(feature = "alloc")]
    fn is_global(&self) -> bool {
        // SAFETY: the pool outlives its handles, and `is_global` is never mutated
        unsafe { (*self.header.as_ptr()).is_global }
    }

    /// # Safety
    ///
    /// the pool must be a fixed pool (not [`Pool::global`]), and the returned borrow ends
    /// before the allocator is borrowed again
    #[expect(
        clippy::mut_from_ref,
        reason = "the pool's allocator is only borrowed here"
    )]
    unsafe fn allocator(&self) -> &mut Allocator {
        // SAFETY: upheld by the caller; `header` was cast from `NonNull<FixedPoolHeader>`
        unsafe {
            let fixed = self.header.cast::<FixedPoolHeader>();
            let allocator = &raw const (*fixed.as_ptr()).allocator;
            &mut *(*allocator).get()
        }
    }

    fn update_stats(&self, update: impl FnOnce(&mut MemoryStats)) {
        let cell = self.stats_cell();
        let mut stats = cell.get();
        update(&mut stats);
        cell.set(stats);
    }

    pub(crate) fn stats(&self) -> MemoryStats {
        self.stats_cell().get()
    }

    /// memory for `layout`, or `None` when the pool has no room. Zero-sized layouts get a
    /// dangling, aligned pointer and use no memory
    pub(crate) fn allocate(&self, table: Table, layout: Layout) -> Option<NonNull<u8>> {
        if layout.size() == 0 {
            return Some(dangling(layout));
        }

        #[cfg(feature = "alloc")]
        if self.is_global() {
            // SAFETY: `layout.size() > 0`
            let ptr = NonNull::new(unsafe { alloc::alloc::alloc(layout) })?;
            self.update_stats(|stats| {
                stats.allocations += 1;
                stats.add(table, layout.size());
            });
            return Some(ptr);
        }

        // SAFETY: not a global pool, and the borrow ends with the call
        let ptr = unsafe { self.allocator() }.allocate(layout)?;
        self.update_stats(|stats| {
            stats.allocations += 1;
            stats.add(table, footprint(layout));
        });

        Some(ptr)
    }

    /// moves the memory at `ptr` to fit `new`, keeping its contents up to the smaller
    /// size. Returns `None` and leaves the memory as it was when the pool has no room.
    ///
    /// # Safety
    ///
    /// `ptr` came from this pool for `old` and `table`, and `new` has the same alignment
    pub(crate) unsafe fn reallocate(
        &self,
        table: Table,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Option<NonNull<u8>> {
        debug_assert_eq!(old.align(), new.align());

        if old.size() == 0 {
            return self.allocate(table, new);
        }
        if new.size() == 0 {
            // SAFETY: upheld by the caller
            unsafe { self.deallocate(table, ptr, old) };
            return Some(dangling(new));
        }

        #[cfg(feature = "alloc")]
        if self.is_global() {
            // SAFETY: `ptr` came from the global allocator with `old`, `new.size() > 0`,
            // and `new` is a valid layout with the same alignment as `old`
            let moved = NonNull::new(unsafe {
                alloc::alloc::realloc(ptr.as_ptr(), old, new.size())
            })?;
            self.update_stats(|stats| {
                stats.allocations += 1;
                stats.remove(table, old.size());
                stats.add(table, new.size());
            });
            return Some(moved);
        }

        // SAFETY: `ptr` came from this allocator with this alignment, and the borrow
        // ends with the call. A failed reallocation leaves `ptr` as it was
        let moved = unsafe { self.allocator().reallocate(ptr, new) }?;
        self.update_stats(|stats| {
            stats.allocations += 1;
            stats.remove(table, footprint(old));
            stats.add(table, footprint(new));
        });

        Some(moved)
    }

    /// # Safety
    ///
    /// `ptr` came from this pool for `layout` and `table`, and isn't used afterwards
    pub(crate) unsafe fn deallocate(&self, table: Table, ptr: NonNull<u8>, layout: Layout) {
        if layout.size() == 0 {
            return;
        }

        #[cfg(feature = "alloc")]
        if self.is_global() {
            // SAFETY: `ptr` came from the global allocator with `layout`
            unsafe { alloc::alloc::dealloc(ptr.as_ptr(), layout) };
            self.update_stats(|stats| stats.remove(table, layout.size()));
            return;
        }

        // SAFETY: `ptr` came from this allocator with this alignment, and the borrow
        // ends with the call
        unsafe { self.allocator().deallocate(ptr, layout.align()) };
        self.update_stats(|stats| stats.remove(table, footprint(layout)));
    }
}

fn dangling(layout: Layout) -> NonNull<u8> {
    // an address that is never dereferenced, aligned for `layout`
    NonNull::<u8>::dangling()
        .with_addr(core::num::NonZeroUsize::new(layout.align()).expect("alignments are non-zero"))
}

/// the bytes an allocation of `layout` takes from the pool, its header and rounding
/// included. Alignments above the header's size can take a little more
fn footprint(layout: Layout) -> usize {
    (layout.size() + ALLOCATION_HEADER.max(layout.align())).next_multiple_of(GRANULARITY)
}

/// pools for unit tests, over leaked memory
#[cfg(test)]
pub(crate) mod testing {
    use core::alloc::Layout;
    use std::boxed::Box;

    use super::{Pool, PoolRef, Table};

    /// a pool of `bytes` bytes
    pub(crate) fn pool_of(bytes: usize) -> Pool {
        Pool::new(Box::leak(Box::<[u8]>::new_uninit_slice(bytes)))
    }

    /// a pool large enough for any test's runtime
    pub(crate) fn pool() -> Pool {
        pool_of(256 * 1024)
    }

    /// a handle to a pool that is never dropped
    pub(crate) fn pool_ref_of(bytes: usize) -> PoolRef {
        Box::leak(Box::new(pool_of(bytes))).handle()
    }

    /// a handle to a pool large enough for any test's frame
    pub(crate) fn pool_ref() -> PoolRef {
        pool_ref_of(256 * 1024)
    }

    /// a pool with about `room` bytes left, the rest taken by allocations that stay
    pub(crate) fn pool_with_room(room: usize) -> Pool {
        let pool = pool_of(4 * 1024 + room);
        fill_leaving(pool.handle(), room);

        pool
    }

    /// takes all but about `room` bytes of `pool` with allocations that stay
    pub(crate) fn fill_leaving(pool: PoolRef, room: usize) {
        let bytes = |size| Layout::from_size_align(size, 1).unwrap();

        let reserved = pool.allocate(Table::Text, bytes(room)).unwrap();
        let mut size = pool.stats().capacity.expect("fixed pool has capacity");
        while size > 0 {
            if pool.allocate(Table::Text, bytes(size)).is_none() {
                size /= 2;
            }
        }
        // SAFETY: allocated above with the same layout and table
        unsafe { pool.deallocate(Table::Text, reserved, bytes(room)) };
    }

    /// a handle to a pool with about `room` bytes left
    pub(crate) fn pool_ref_with_room(room: usize) -> PoolRef {
        Box::leak(Box::new(pool_with_room(room))).handle()
    }
}
