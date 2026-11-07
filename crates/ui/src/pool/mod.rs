//! the byte pool the runtime keeps its tables in.
//!
//! a [`Pool`] takes a buffer of bytes from the application and hands out memory from it
//! through a TLSF allocator, so allocation and release take constant time. The
//! allocator's own state sits at the start of the buffer, which lets the tables hold a
//! plain pointer to the pool while the runtime that owns it moves.
//!
//! SAFETY INVARIANTS:
//!
//! 1. a [`PoolRef`] is only used while the [`Pool`] it came from exists. The runtime
//!    drops every table before its pool.
//! 2. the allocator is only borrowed inside the methods of [`PoolRef`], which never run
//!    other code meanwhile, so its borrows never overlap.
//! 3. the chunks a growable pool adds are freed only when the pool is dropped, after
//!    every table, and before the buffer that holds the allocator.

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
/// byte counts include the allocator's header and rounding for each allocation, so
/// `used` reaching `capacity` is when allocations fail
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryStats {
    /// bytes the pool can hand out, after the allocator's own state
    pub capacity: usize,
    /// bytes in use now
    pub used: usize,
    /// the most bytes in use at once
    pub peak: usize,
    /// allocations made so far, growing a table included
    pub allocations: usize,
    /// chunks of memory a growable pool added because it ran out. Always 0 for a
    /// fixed pool
    pub chunks_added: usize,
    by_table: [usize; Table::COUNT],
}

impl MemoryStats {
    const fn new(capacity: usize) -> Self {
        Self {
            capacity,
            used: 0,
            peak: 0,
            allocations: 0,
            chunks_added: 0,
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

struct PoolHeader {
    allocator: UnsafeCell<Allocator>,
    stats: Cell<MemoryStats>,
    /// how the pool grows, if it does
    #[cfg(feature = "alloc")]
    growth: Cell<Option<Growth>>,
}

#[cfg(feature = "alloc")]
#[derive(Clone, Copy)]
struct Growth {
    /// the fewest bytes a chunk takes from the heap
    min_chunk: usize,
    /// the chunks added so far, newest first
    chunks: Option<NonNull<Chunk>>,
}

/// the start of a chunk a growable pool added. The rest of the chunk is free memory
#[cfg(feature = "alloc")]
struct Chunk {
    next: Option<NonNull<Chunk>>,
    layout: Layout,
}

/// the bytes the runtime keeps its tables in.
///
/// a pool made with [`Pool::new`] or [`Pool::boxed`] has a fixed size: tables that
/// need more than it holds fail with [`OutOfMemory`]. One made with [`Pool::growable`]
/// takes another chunk from the heap instead.
///
/// ```
/// # use core::mem::MaybeUninit;
/// # use inkpaper_ui::Pool;
/// static mut BYTES: [MaybeUninit<u8>; 16 * 1024] = [MaybeUninit::uninit(); 16 * 1024];
///
/// // SAFETY: nothing else uses `BYTES`
/// let pool = Pool::new(unsafe { &mut *core::ptr::addr_of_mut!(BYTES) });
/// assert!(pool.stats().capacity > 14 * 1024);
/// ```
pub struct Pool {
    header: NonNull<PoolHeader>,
    /// the buffer, when the pool allocated it
    #[cfg(feature = "alloc")]
    owned: Option<NonNull<[MaybeUninit<u8>]>>,
}

impl Pool {
    /// a pool over `memory`. A few hundred bytes of it hold the allocator's state.
    ///
    /// # Panics
    ///
    /// when `memory` is too small to hold the allocator's state and an allocation
    pub fn new(memory: &'static mut [MaybeUninit<u8>]) -> Self {
        // SAFETY: the memory is borrowed for the rest of the program, and only the pool
        // uses it
        unsafe { Self::from_raw(NonNull::from(memory)) }
    }

    /// a pool over a new heap buffer of `bytes` bytes, released when the pool is dropped
    ///
    /// # Panics
    ///
    /// when `bytes` is too few to hold the allocator's state and an allocation
    #[cfg(feature = "alloc")]
    pub fn boxed(bytes: usize) -> Self {
        let memory = alloc::boxed::Box::<[u8]>::new_uninit_slice(bytes);
        let memory = NonNull::from(alloc::boxed::Box::leak(memory));

        // SAFETY: the pool owns the buffer and releases it after its last use
        let mut pool = unsafe { Self::from_raw(memory) };
        pool.owned = Some(memory);

        pool
    }

    /// a pool over a new heap buffer of `bytes` bytes that takes another chunk from the
    /// heap whenever it runs out. Each chunk holds at least `bytes` bytes and at least
    /// twice the allocation that needed it. Chunks are released when the pool is
    /// dropped, not when the memory in them is freed
    ///
    /// # Panics
    ///
    /// when `bytes` is too few to hold the allocator's state and an allocation
    #[cfg(feature = "alloc")]
    pub fn growable(bytes: usize) -> Self {
        let pool = Self::boxed(bytes);
        pool.handle().header().growth.set(Some(Growth {
            min_chunk: bytes,
            chunks: None,
        }));

        pool
    }

    /// # Safety
    ///
    /// `memory` is valid for the life of the pool, and nothing else uses it meanwhile
    unsafe fn from_raw(memory: NonNull<[MaybeUninit<u8>]>) -> Self {
        let start = memory.cast::<u8>();
        let len = memory.len();

        let offset = start.align_offset(mem::align_of::<PoolHeader>());
        let blocks_start = offset
            .checked_add(mem::size_of::<PoolHeader>())
            .filter(|&end| end < len)
            .unwrap_or_else(|| panic!("a pool of {len} bytes can't hold its allocator"));

        // SAFETY: the header and the block after it are inside `memory`, which the pool
        // owns. The header is aligned
        let (header, blocks) = unsafe {
            let header = start.add(offset).cast::<PoolHeader>();
            header.write(PoolHeader {
                allocator: UnsafeCell::new(Allocator::new()),
                stats: Cell::new(MemoryStats::new(0)),
                #[cfg(feature = "alloc")]
                growth: Cell::new(None),
            });

            let blocks = NonNull::slice_from_raw_parts(start.add(blocks_start), len - blocks_start);

            (header, blocks)
        };

        // SAFETY: the block lives as long as the allocator, which is stored in front of
        // it, and nothing else borrows the allocator yet
        let capacity = unsafe { (*header.as_ref().allocator.get()).insert_free_block_ptr(blocks) }
            .unwrap_or_else(|| panic!("a pool of {len} bytes can't hold an allocation"));

        // SAFETY: just written
        unsafe { header.as_ref() }
            .stats
            .set(MemoryStats::new(capacity.get()));

        Self {
            header,
            #[cfg(feature = "alloc")]
            owned: None,
        }
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
        // the chunks' list lives in the buffer below, so the chunks go first
        #[cfg(feature = "alloc")]
        if let Some(growth) = self.handle().header().growth.get() {
            let mut next = growth.chunks;
            while let Some(chunk) = next {
                // SAFETY: `grow` wrote this header at the start of the chunk, which no
                // table uses any more
                let Chunk {
                    next: after,
                    layout,
                } = unsafe { chunk.read() };
                // SAFETY: `grow` allocated the chunk with this layout
                unsafe { alloc::alloc::dealloc(chunk.as_ptr().cast(), layout) };
                next = after;
            }
        }

        #[cfg(feature = "alloc")]
        if let Some(memory) = self.owned {
            // SAFETY: `boxed` leaked this box for the pool, and no table outlives the
            // pool. The header needs no drop
            drop(unsafe { alloc::boxed::Box::from_raw(memory.as_ptr()) });
        }
    }
}

/// a handle to a [`Pool`], copied into each table that allocates from it
#[derive(Clone, Copy)]
pub(crate) struct PoolRef {
    header: NonNull<PoolHeader>,
}

impl PoolRef {
    fn header(&self) -> &PoolHeader {
        // SAFETY: the pool outlives its handles
        unsafe { self.header.as_ref() }
    }

    /// # Safety
    ///
    /// the returned borrow ends before the allocator is borrowed again
    #[expect(
        clippy::mut_from_ref,
        reason = "the pool's allocator is only borrowed here"
    )]
    unsafe fn allocator(&self) -> &mut Allocator {
        // SAFETY: upheld by the caller
        unsafe { &mut *self.header().allocator.get() }
    }

    fn update_stats(&self, update: impl FnOnce(&mut MemoryStats)) {
        let header = self.header();
        let mut stats = header.stats.get();
        update(&mut stats);
        header.stats.set(stats);
    }

    pub(crate) fn stats(&self) -> MemoryStats {
        self.header().stats.get()
    }

    /// adds a chunk from the heap with room for `layout`, if the pool grows. Returns
    /// whether it did
    #[cfg(feature = "alloc")]
    fn grow(&self, layout: Layout) -> bool {
        let header = self.header();
        let Some(growth) = header.growth.get() else {
            return false;
        };

        // the allocation, its header and alignment, the chunk's header, and the
        // allocator's rounding and sentinel
        let Some(needed) = layout
            .size()
            .checked_add(layout.align())
            .and_then(|bytes| bytes.checked_add(mem::size_of::<Chunk>() + 4 * GRANULARITY))
        else {
            return false;
        };
        let bytes = needed.saturating_mul(2).max(growth.min_chunk);
        let Ok(chunk_layout) =
            Layout::from_size_align(bytes, mem::align_of::<Chunk>().max(GRANULARITY))
        else {
            return false;
        };

        // SAFETY: the layout has a nonzero size
        let Some(chunk) = NonNull::new(unsafe { alloc::alloc::alloc(chunk_layout) }) else {
            return false;
        };
        let chunk = chunk.cast::<Chunk>();

        // SAFETY: the chunk is new, aligned for its header and larger than it. The free
        // block after the header lives until the pool drops it, after the allocator's
        // last use, and nothing borrows the allocator meanwhile
        let added = unsafe {
            chunk.write(Chunk {
                next: growth.chunks,
                layout: chunk_layout,
            });
            let start = chunk.cast::<u8>().add(mem::size_of::<Chunk>());
            let block = NonNull::slice_from_raw_parts(start, bytes - mem::size_of::<Chunk>());

            self.allocator().insert_free_block_ptr(block)
        };
        let Some(added) = added else {
            // SAFETY: allocated above with this layout, and the allocator didn't take it
            unsafe { alloc::alloc::dealloc(chunk.as_ptr().cast(), chunk_layout) };
            return false;
        };

        header.growth.set(Some(Growth {
            chunks: Some(chunk),
            ..growth
        }));
        self.update_stats(|stats| {
            stats.capacity += added.get();
            stats.chunks_added += 1;
        });

        true
    }

    #[cfg(not(feature = "alloc"))]
    fn grow(&self, _: Layout) -> bool {
        false
    }

    /// memory for `layout`, or `None` when the pool has no room. Zero-sized layouts get a
    /// dangling, aligned pointer and use no memory
    pub(crate) fn allocate(&self, table: Table, layout: Layout) -> Option<NonNull<u8>> {
        if layout.size() == 0 {
            return Some(dangling(layout));
        }

        // SAFETY: the borrows end with the calls
        let ptr = match unsafe { self.allocator() }.allocate(layout) {
            Some(ptr) => ptr,
            None if self.grow(layout) => unsafe { self.allocator() }.allocate(layout)?,
            None => return None,
        };
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

        // SAFETY: `ptr` came from this allocator with this alignment, and the borrows
        // end with the calls. A failed reallocation leaves `ptr` as it was
        let moved = match unsafe { self.allocator().reallocate(ptr, new) } {
            Some(moved) => moved,
            None if self.grow(new) => unsafe { self.allocator().reallocate(ptr, new) }?,
            None => return None,
        };
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
        let handle = pool.handle();
        let bytes = |size| Layout::from_size_align(size, 1).unwrap();

        let reserved = handle.allocate(Table::Text, bytes(room)).unwrap();
        let mut size = pool.stats().capacity;
        while size > 0 {
            if handle.allocate(Table::Text, bytes(size)).is_none() {
                size /= 2;
            }
        }
        // SAFETY: allocated above with the same layout and table
        unsafe { handle.deallocate(Table::Text, reserved, bytes(room)) };

        pool
    }

    /// a handle to a pool with about `room` bytes left
    pub(crate) fn pool_ref_with_room(room: usize) -> PoolRef {
        Box::leak(Box::new(pool_with_room(room))).handle()
    }
}
