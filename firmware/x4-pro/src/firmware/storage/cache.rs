use aligned::{A4, Aligned};

pub(super) const SECTOR_SIZE: usize = 512;

/// Number of 512-byte sectors held in the inline LRU cache (4 KiB total).
///
/// FAT directory scans, FAT chain walks, and file reads ping-pong across
/// several sectors at once; keeping 8 sectors cached avoids repeated SD reads
/// while remaining small enough to stay in internal SRAM for SDHOST IDMAC.
pub(super) const CACHE_SECTORS: usize = 8;

struct CacheSlot {
    sector: Option<u32>,
    last_used: u64,
    data: Aligned<A4, [u8; SECTOR_SIZE]>,
}

impl CacheSlot {
    const fn empty() -> Self {
        Self {
            sector: None,
            last_used: 0,
            data: Aligned([0; SECTOR_SIZE]),
        }
    }
}

pub(super) struct SectorCache<const N: usize = CACHE_SECTORS> {
    slots: [CacheSlot; N],
    clock: u64,
}

impl<const N: usize> SectorCache<N> {
    pub(super) const fn new() -> Self {
        Self {
            slots: [const { CacheSlot::empty() }; N],
            clock: 0,
        }
    }

    fn next_tick(&mut self) -> u64 {
        self.clock = self.clock.wrapping_add(1);
        self.clock
    }

    /// Returns the slot index if `sector` is already cached, refreshing its LRU timestamp.
    pub(super) fn lookup(&mut self, sector: u32) -> Option<usize> {
        let index = self
            .slots
            .iter()
            .position(|slot| slot.sector == Some(sector))?;
        self.slots[index].last_used = self.next_tick();
        Some(index)
    }

    /// Selects a slot for loading or overwriting `sector`: reuses an existing
    /// entry for `sector` if present, otherwise an empty slot or the LRU victim.
    ///
    /// Invalidates the slot's tag when evicting so a failed I/O never leaves
    /// stale data tagged with an old sector number.
    pub(super) fn allocate_slot(&mut self, sector: u32) -> usize {
        if let Some(index) = self
            .slots
            .iter()
            .position(|slot| slot.sector == Some(sector))
        {
            self.slots[index].last_used = self.next_tick();
            return index;
        }

        let victim = self.find_victim();
        self.slots[victim].sector = None;
        victim
    }

    pub(super) fn commit(&mut self, slot: usize, sector: u32) {
        self.slots[slot].sector = Some(sector);
        self.slots[slot].last_used = self.next_tick();
    }

    pub(super) fn invalidate(&mut self, slot: usize) {
        self.slots[slot].sector = None;
    }

    pub(super) fn data(&self, slot: usize) -> &Aligned<A4, [u8; SECTOR_SIZE]> {
        &self.slots[slot].data
    }

    pub(super) fn data_mut(&mut self, slot: usize) -> &mut Aligned<A4, [u8; SECTOR_SIZE]> {
        &mut self.slots[slot].data
    }

    fn find_victim(&self) -> usize {
        if let Some(empty) = self.slots.iter().position(|slot| slot.sector.is_none()) {
            return empty;
        }

        let mut victim = 0;
        let mut oldest = self.slots[0].last_used;
        for (index, slot) in self.slots.iter().enumerate().skip(1) {
            if slot.last_used < oldest {
                oldest = slot.last_used;
                victim = index;
            }
        }
        victim
    }
}
