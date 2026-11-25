use core::alloc::Layout;
use std::{rc::Rc, vec, vec::Vec};

use super::OutOfMemory;
#[cfg(feature = "alloc")]
use super::Pool;
use super::{GRANULARITY, PoolVec, Table, testing::pool_of as pool};

fn bytes(size: usize) -> Layout {
    Layout::from_size_align(size, 1).unwrap()
}

#[test]
fn hands_out_almost_all_of_its_memory() {
    let pool = pool(16 * 1024);
    let stats = pool.stats();
    let capacity = stats.capacity.unwrap();

    assert!(capacity > 14 * 1024, "{stats:?}");
    assert!(capacity < 16 * 1024);
    assert_eq!((stats.used, stats.peak, stats.allocations), (0, 0, 0));
}

#[test]
#[should_panic(expected = "can't hold its allocator")]
fn rejects_memory_too_small_for_its_allocator() {
    pool(16);
}

#[test]
fn counts_the_memory_each_table_uses() {
    let pool = pool(4 * 1024);
    let handle = pool.handle();

    let node = handle.allocate(Table::Nodes, bytes(100)).unwrap();
    let text = handle.allocate(Table::Text, bytes(10)).unwrap();
    let stats = pool.stats();

    assert!(stats.table(Table::Nodes) >= 100 + GRANULARITY / 2);
    assert!(stats.table(Table::Text) >= 10);
    assert_eq!(stats.table(Table::Entities), 0);
    assert_eq!(
        stats.used,
        stats.table(Table::Nodes) + stats.table(Table::Text)
    );
    assert_eq!(stats.peak, stats.used);
    assert_eq!(stats.allocations, 2);

    unsafe {
        handle.deallocate(Table::Nodes, node, bytes(100));
        handle.deallocate(Table::Text, text, bytes(10));
    }
    let after = pool.stats();

    assert_eq!(after.used, 0);
    assert_eq!(after.table(Table::Nodes), 0);
    assert_eq!(after.peak, stats.peak);
}

#[test]
fn fails_without_room_and_reuses_released_memory() {
    let pool = pool(4 * 1024);
    let handle = pool.handle();
    let half = pool.stats().capacity.unwrap() / 2;

    let first = handle.allocate(Table::Nodes, bytes(half)).unwrap();
    let before = pool.stats();

    assert!(handle.allocate(Table::Nodes, bytes(half + 64)).is_none());
    assert_eq!(pool.stats(), before);

    unsafe { handle.deallocate(Table::Nodes, first, bytes(half)) };

    assert!(handle.allocate(Table::Nodes, bytes(half + 64)).is_some());
}

#[test]
fn reallocation_keeps_contents_and_leaves_them_when_it_fails() {
    let pool = pool(4 * 1024);
    let handle = pool.handle();

    let ptr = handle.allocate(Table::Text, bytes(4)).unwrap();
    unsafe { ptr.as_ptr().copy_from([1, 2, 3, 4].as_ptr(), 4) };

    let grown = unsafe { handle.reallocate(Table::Text, ptr, bytes(4), bytes(64)) }.unwrap();
    assert_eq!(
        unsafe { core::slice::from_raw_parts(grown.as_ptr(), 4) },
        [1, 2, 3, 4]
    );

    let before = pool.stats();
    let too_big = bytes(4 * 1024);

    assert!(unsafe { handle.reallocate(Table::Text, grown, bytes(64), too_big) }.is_none());
    assert_eq!(pool.stats(), before);
    assert_eq!(
        unsafe { core::slice::from_raw_parts(grown.as_ptr(), 4) },
        [1, 2, 3, 4]
    );
}

#[test]
fn zero_sized_allocations_use_no_memory() {
    let pool = pool(4 * 1024);
    let handle = pool.handle();
    let layout = Layout::from_size_align(0, 8).unwrap();

    let ptr = handle.allocate(Table::Entities, layout).unwrap();

    assert_eq!(ptr.as_ptr().addr() % 8, 0);
    assert_eq!(pool.stats().used, 0);
    unsafe { handle.deallocate(Table::Entities, ptr, layout) };
}

#[cfg(feature = "alloc")]
#[test]
fn global_pool_delegates_to_heap_and_tracks_stats() {
    let pool = Pool::global();
    let handle = pool.handle();
    assert_eq!(pool.stats().capacity, None);

    let first = handle.allocate(Table::Nodes, bytes(128)).unwrap();
    let second = handle.allocate(Table::Text, bytes(64 * 1024)).unwrap();
    let stats = pool.stats();

    assert_eq!(stats.capacity, None);
    assert_eq!(stats.table(Table::Nodes), 128);
    assert_eq!(stats.table(Table::Text), 64 * 1024);
    assert_eq!(stats.used, 128 + 64 * 1024);
    assert_eq!(stats.peak, stats.used);
    assert_eq!(stats.allocations, 2);

    unsafe { first.as_ptr().copy_from([1, 2, 3, 4].as_ptr(), 4) };
    let grown = unsafe { handle.reallocate(Table::Nodes, first, bytes(128), bytes(512)) }.unwrap();
    assert_eq!(
        unsafe { core::slice::from_raw_parts(grown.as_ptr(), 4) },
        [1, 2, 3, 4]
    );
    assert_eq!(pool.stats().table(Table::Nodes), 512);
    assert_eq!(pool.stats().allocations, 3);

    unsafe {
        handle.deallocate(Table::Nodes, grown, bytes(512));
        handle.deallocate(Table::Text, second, bytes(64 * 1024));
    }

    let after = pool.stats();
    assert_eq!(after.used, 0);
    assert_eq!(after.table(Table::Nodes), 0);
    assert_eq!(after.table(Table::Text), 0);
    assert_eq!(after.peak, 512 + 64 * 1024);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_grows_and_frees_in_a_global_pool() {
    let pool = Pool::global();
    {
        let mut values = PoolVec::new_in(pool.handle(), Table::Nodes);

        for value in 0..4_096u32 {
            values.try_push(value).unwrap();
        }

        assert_eq!(pool.stats().capacity, None);
        assert!(pool.stats().table(Table::Nodes) >= 4_096 * core::mem::size_of::<u32>());
        assert!(values.iter().copied().eq(0..4_096));
    }

    assert_eq!(pool.stats().used, 0);
}

#[test]
fn fixed_pool_rejects_allocations_beyond_capacity() {
    let pool = pool(4 * 1024);
    let capacity = pool.stats().capacity.unwrap();

    assert!(
        pool.handle()
            .allocate(Table::Nodes, bytes(capacity))
            .is_none()
    );
    assert_eq!(pool.stats().capacity, Some(capacity));
}

#[test]
fn vec_pushes_extends_resizes_and_truncates() {
    let pool = pool(4 * 1024);
    let mut values = PoolVec::new_in(pool.handle(), Table::Nodes);

    values.try_push(1u32).unwrap();
    values.try_extend_from_slice(&[2, 3]).unwrap();
    values.try_resize(5, 9).unwrap();
    assert_eq!(&*values, &[1, 2, 3, 9, 9]);

    values[0] = 7;
    values.truncate(2);
    assert_eq!(&*values, &[7, 2]);

    values.try_resize(1, 0).unwrap();
    assert_eq!(&*values, &[7]);

    values.clear();
    assert!(values.is_empty());
}

#[test]
fn vec_doubles_its_capacity() {
    let pool = pool(4 * 1024);
    let mut values = PoolVec::new_in(pool.handle(), Table::Nodes);
    let mut capacities = Vec::new();

    for value in 0..17u8 {
        values.try_push(value).unwrap();
        if capacities.last() != Some(&values.capacity()) {
            capacities.push(values.capacity());
        }
    }

    assert_eq!(capacities, [4, 8, 16, 32]);
    assert_eq!(pool.stats().allocations, 4);
}

#[test]
fn vec_grows_to_the_exact_size_when_doubling_does_not_fit() {
    let pool = pool(4 * 1024);
    let len = pool.stats().capacity.unwrap() * 3 / 5;
    let mut values = PoolVec::new_in(pool.handle(), Table::Text);

    values.try_resize(len, 1u8).unwrap();
    assert_eq!(values.capacity(), len);

    values.try_push(2).unwrap();

    assert_eq!(values.capacity(), len + 1);
    assert_eq!(values[len], 2);
}

#[test]
fn vec_reports_full_without_changing_contents() {
    let pool = pool(4 * 1024);
    let mut values = PoolVec::new_in(pool.handle(), Table::Text);
    values.try_extend_from_slice(&[1u8, 2]).unwrap();
    let too_many = pool.stats().capacity.unwrap();

    let out_of_memory = |requested| {
        Err(OutOfMemory {
            table: Table::Text,
            requested,
        })
    };

    assert_eq!(values.try_reserve(too_many), out_of_memory(too_many + 2));
    assert_eq!(values.try_resize(too_many, 0), out_of_memory(too_many));
    assert_eq!(
        values.try_extend_from_slice(&vec![0; too_many]),
        out_of_memory(too_many + 2)
    );
    assert_eq!(values.try_reserve(usize::MAX), out_of_memory(usize::MAX));
    assert_eq!(&*values, &[1, 2]);
}

#[test]
fn vec_drops_each_value_once_and_returns_its_memory() {
    let pool = pool(4 * 1024);
    let value = Rc::new(());

    {
        let mut values = PoolVec::new_in(pool.handle(), Table::Callbacks);
        values.try_resize(6, value.clone()).unwrap();
        assert_eq!(Rc::strong_count(&value), 7);

        values.truncate(2);
        assert_eq!(Rc::strong_count(&value), 3);
    }

    assert_eq!(Rc::strong_count(&value), 1);
    assert_eq!(pool.stats().used, 0);
}

#[test]
fn vec_returns_spare_capacity() {
    let pool = pool(4 * 1024);
    let mut values = PoolVec::new_in(pool.handle(), Table::Nodes);
    values.try_resize(64, 0u64).unwrap();
    let full = pool.stats().used;

    values.truncate(3);
    values.shrink_to_fit();

    assert_eq!(values.capacity(), 3);
    assert_eq!(&*values, &[0, 0, 0]);
    assert!(pool.stats().used < full);

    values.clear();
    values.shrink_to_fit();

    assert_eq!(values.capacity(), 0);
    assert_eq!(pool.stats().used, 0);
}

#[test]
fn vec_of_zero_sized_values_uses_no_memory() {
    let pool = pool(4 * 1024);
    let mut values = PoolVec::new_in(pool.handle(), Table::Events);

    values.try_resize(1_000, ()).unwrap();

    assert_eq!(values.len(), 1_000);
    assert_eq!(pool.stats().used, 0);
}

#[test]
fn lists_every_table_in_index_order() {
    for (index, table) in Table::ALL.into_iter().enumerate() {
        assert_eq!(table.index(), index);
    }
}
