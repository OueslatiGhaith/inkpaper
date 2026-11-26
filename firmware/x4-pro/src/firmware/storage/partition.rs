use aligned::A4;
use block_device_driver::BlockDevice as RawBlockDevice;
use defmt::{Debug2Format, warn};
use hadris_io::{
    Error as HadrisError, ErrorKind as HadrisErrorKind, Result as HadrisResult, SeekFrom,
    r#async::{Read as HadrisRead, Seek as HadrisSeek, Write as HadrisWrite},
};

use super::cache::SectorCache;
pub(super) use super::cache::SECTOR_SIZE;

const MBR_PARTITION_TABLE_OFFSET: usize = 446;
const MBR_PARTITION_ENTRY_SIZE: usize = 16;
const MBR_PARTITION_COUNT: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Partition {
    pub(super) index: usize,
    pub(super) partition_type: u8,
    pub(super) first_lba: u32,
    pub(super) sectors: u32,
}

pub(super) struct SdPartition<D> {
    device: D,
    first_lba: u32,
    sector_count: u32,
    position: u64,
    cache: SectorCache,
}

impl<D> SdPartition<D>
where
    D: RawBlockDevice<SECTOR_SIZE, Align = A4>,
{
    pub(super) fn new(device: D, first_lba: u32, sector_count: u32) -> Self {
        Self {
            device,
            first_lba,
            sector_count,
            position: 0,
            cache: SectorCache::new(),
        }
    }

    pub(super) fn into_inner(self) -> D {
        self.device
    }

    fn len_bytes(&self) -> u64 {
        self.sector_count as u64 * SECTOR_SIZE as u64
    }

    fn physical_lba(&self, relative_sector: u32) -> HadrisResult<u32> {
        if relative_sector >= self.sector_count {
            return Err(HadrisError::new(
                HadrisErrorKind::UnexpectedEof,
                "sector outside SD partition",
            ));
        }

        self.first_lba
            .checked_add(relative_sector)
            .ok_or_else(|| HadrisError::new(HadrisErrorKind::InvalidInput, "SD LBA overflow"))
    }

    async fn load_sector(&mut self, relative_sector: u32) -> HadrisResult<usize> {
        let physical_lba = self.physical_lba(relative_sector)?;

        if let Some(slot) = self.cache.lookup(relative_sector) {
            return Ok(slot);
        }

        let slot = self.cache.allocate_slot(relative_sector);

        if let Err(error) = self
            .device
            .read(physical_lba, core::slice::from_mut(self.cache.data_mut(slot)))
            .await
        {
            warn!(
                "SD read failed at LBA {}: {}",
                physical_lba,
                Debug2Format(&error),
            );

            return Err(HadrisError::new(
                HadrisErrorKind::Other,
                "SD block read failed",
            ));
        }

        self.cache.commit(slot, relative_sector);

        Ok(slot)
    }

    async fn prepare_write_slot(
        &mut self,
        relative_sector: u32,
        full_sector: bool,
    ) -> HadrisResult<usize> {
        if !full_sector {
            return self.load_sector(relative_sector).await;
        }

        // Full-sector overwrite skips reading the old contents from SD.
        let _ = self.physical_lba(relative_sector)?;
        Ok(self.cache.allocate_slot(relative_sector))
    }

    async fn store_sector(&mut self, slot: usize, relative_sector: u32) -> HadrisResult<()> {
        let physical_lba = self.physical_lba(relative_sector)?;

        if let Err(error) = self
            .device
            .write(physical_lba, core::slice::from_ref(self.cache.data(slot)))
            .await
        {
            self.cache.invalidate(slot);

            warn!(
                "SD write failed at LBA: {}: {}",
                physical_lba,
                Debug2Format(&error),
            );

            return Err(HadrisError::new(
                HadrisErrorKind::Other,
                "SD block write failed",
            ));
        }

        self.cache.commit(slot, relative_sector);

        Ok(())
    }
}

impl<D> HadrisRead for SdPartition<D>
where
    D: RawBlockDevice<SECTOR_SIZE, Align = A4>,
{
    type Error = HadrisErrorKind;

    async fn read(&mut self, buf: &mut [u8]) -> HadrisResult<usize, Self::Error> {
        if buf.is_empty() {
            return Ok(0);
        }

        let remaining = self.len_bytes().saturating_sub(self.position);

        if remaining == 0 {
            return Ok(0);
        }

        let bytes_to_read = core::cmp::min(buf.len() as u64, remaining) as usize;
        let mut copied = 0usize;

        while copied < bytes_to_read {
            let relative_sector =
                u32::try_from(self.position / SECTOR_SIZE as u64).map_err(|_| {
                    HadrisError::new(HadrisErrorKind::InvalidInput, "SD sector index overflow")
                })?;

            let offset = (self.position % SECTOR_SIZE as u64) as usize;
            let slot = self.load_sector(relative_sector).await?;

            let available = SECTOR_SIZE - offset;
            let wanted = bytes_to_read - copied;
            let count = core::cmp::min(available, wanted);

            buf[copied..copied + count]
                .copy_from_slice(&self.cache.data(slot)[offset..offset + count]);

            copied += count;
            self.position += count as u64;
        }

        Ok(copied)
    }
}

impl<D> HadrisWrite for SdPartition<D>
where
    D: RawBlockDevice<SECTOR_SIZE, Align = A4>,
{
    type Error = HadrisErrorKind;

    async fn write(&mut self, buf: &[u8]) -> HadrisResult<usize, Self::Error> {
        if buf.is_empty() {
            return Ok(0);
        }

        let remaining = self.len_bytes().saturating_sub(self.position);

        if remaining == 0 {
            return Ok(0);
        }

        let bytes_to_write = core::cmp::min(buf.len() as u64, remaining) as usize;
        let mut written = 0usize;

        while written < bytes_to_write {
            let relative_sector =
                u32::try_from(self.position / SECTOR_SIZE as u64).map_err(|_| {
                    HadrisError::new(HadrisErrorKind::InvalidInput, "SD sector index overflow")
                })?;

            let offset = (self.position % SECTOR_SIZE as u64) as usize;

            let available = SECTOR_SIZE - offset;
            let wanted = bytes_to_write - written;
            let count = core::cmp::min(available, wanted);
            let full_sector = offset == 0 && count == SECTOR_SIZE;

            let slot = self.prepare_write_slot(relative_sector, full_sector).await?;

            self.cache.data_mut(slot)[offset..offset + count]
                .copy_from_slice(&buf[written..written + count]);

            self.store_sector(slot, relative_sector).await?;

            written += count;
            self.position += count as u64;
        }

        Ok(written)
    }

    async fn flush(&mut self) -> HadrisResult<(), Self::Error> {
        // Sector writes are write-through and committed immediately.
        Ok(())
    }
}

impl<D> HadrisSeek for SdPartition<D>
where
    D: RawBlockDevice<SECTOR_SIZE, Align = A4>,
{
    type Error = HadrisErrorKind;

    async fn seek(&mut self, position: SeekFrom) -> HadrisResult<u64, Self::Error> {
        let new_position = match position {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => add_signed_offset(self.len_bytes(), offset)?,
            SeekFrom::Current(offset) => add_signed_offset(self.position, offset)?,
        };

        self.position = new_position;

        Ok(self.position)
    }
}

pub(super) fn find_fat_partition(sector: &[u8; SECTOR_SIZE]) -> Option<Partition> {
    if sector[510] != 0x55 || sector[511] != 0xaa {
        return None;
    }

    for index in 0..MBR_PARTITION_COUNT {
        let offset = MBR_PARTITION_TABLE_OFFSET + index * MBR_PARTITION_ENTRY_SIZE;

        let entry = &sector[offset..offset + MBR_PARTITION_ENTRY_SIZE];

        let partition_type = entry[4];
        let first_lba = le_u32(&entry[8..12]);

        let sectors = le_u32(&entry[12..16]);

        if sectors == 0 || !is_fat_partition_type(partition_type) {
            continue;
        }

        return Some(Partition {
            index,
            partition_type,
            first_lba,
            sectors,
        });
    }

    None
}

fn is_fat_partition_type(partition_type: u8) -> bool {
    matches!(partition_type, 0x01 | 0x04 | 0x06 | 0x0b | 0x0c | 0x0e)
}

fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn add_signed_offset(base: u64, offset: i64) -> HadrisResult<u64> {
    if offset >= 0 {
        base.checked_add(offset as u64)
            .ok_or_else(|| HadrisError::new(HadrisErrorKind::InvalidInput, "seek overflow"))
    } else {
        base.checked_sub(offset.unsigned_abs())
            .ok_or_else(|| HadrisError::new(HadrisErrorKind::InvalidInput, "seek before start"))
    }
}
