#[allow(dead_code)]
#[path = "../src/firmware/storage/cache.rs"]
mod cache;

use cache::{CACHE_SECTORS, SECTOR_SIZE, SectorCache};

#[test]
fn lru_caches_eight_sectors_and_evicts_least_recently_used() {
    let mut cache = SectorCache::<CACHE_SECTORS>::new();

    for sector in 0..CACHE_SECTORS as u32 {
        assert_eq!(cache.lookup(sector), None);
        let slot = cache.allocate_slot(sector);
        cache.data_mut(slot).fill(sector as u8);
        cache.commit(slot, sector);
    }

    // All 8 sectors hit.
    for sector in 0..CACHE_SECTORS as u32 {
        let slot = cache.lookup(sector).expect("sector should be cached");
        assert_eq!(cache.data(slot).as_ref(), &[sector as u8; SECTOR_SIZE]);
    }

    // Touch every sector except 3, making sector 3 the LRU victim.
    for sector in [0u32, 1, 2, 4, 5, 6, 7] {
        assert!(cache.lookup(sector).is_some());
    }

    let evicted_slot = cache.allocate_slot(99);
    cache.data_mut(evicted_slot).fill(0xaa);
    cache.commit(evicted_slot, 99);

    assert_eq!(cache.lookup(3), None);
    assert!(cache.lookup(99).is_some());
    for sector in [0u32, 1, 2, 4, 5, 6, 7] {
        assert!(cache.lookup(sector).is_some());
    }
}

#[test]
fn allocate_reuses_existing_slot_and_invalidate_clears_entry() {
    let mut cache = SectorCache::<CACHE_SECTORS>::new();

    let slot = cache.allocate_slot(42);
    cache.data_mut(slot).fill(0x11);
    cache.commit(slot, 42);

    // Overwriting an already-cached sector reuses the exact same slot (no duplicate).
    assert_eq!(cache.allocate_slot(42), slot);
    cache.data_mut(slot).fill(0x22);
    cache.commit(slot, 42);
    let hit = cache.lookup(42).unwrap();
    assert_eq!(cache.data(hit).as_ref(), &[0x22; SECTOR_SIZE]);

    // Invalidating removes it so failed writes never leave uncommitted data cached.
    cache.invalidate(slot);
    assert_eq!(cache.lookup(42), None);
}
