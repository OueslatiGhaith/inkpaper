use aligned::{A4, Aligned};
use block_device_driver::BlockDevice as RawBlockDevice;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};
use embassy_time::{Delay, Duration, Timer};
use esp_hal::{
    peripherals::{GPIO40, GPIO41, GPIO42, SDHOST},
    sdmmc::{Config, SdHostController, SlotConfig},
};
use esp_println::{print, println};
use hadris_fat::r#async::{DirectoryEntry, FatVolume, FileEntry};
use hadris_io::{
    Error as HadrisError, ErrorKind as HadrisErrorKind, Result as HadrisResult, SeekFrom,
    r#async::{Read as HadrisRead, Seek as HadrisSeek},
};
use sdio::{BlockDevice, MmcBus, sd::Card};

use crate::firmware::power::SdPower;

const TARGET_FREQUENCY_HZ: u32 = 40_000_000;
const MOUNT_ATTEMPTS: usize = 4;
const POWER_OFF_MS: u64 = 80;
const POWER_SETTLE_MS: u64 = 120;
const SECTOR_SIZE: usize = 512;
const MBR_PARTITION_TABLE_OFFSET: usize = 446;
const MBR_PARTITION_ENTRY_SIZE: usize = 16;
const MBR_PARTITION_COUNT: usize = 4;
const COMMAND_CAPACITY: usize = 4;

static COMMANDS: Channel<CriticalSectionRawMutex, Command, COMMAND_CAPACITY> = Channel::new();
static READY: Signal<CriticalSectionRawMutex, bool> = Signal::new();
static ROOT_LIST_DONE: Signal<CriticalSectionRawMutex, bool> = Signal::new();
static SHUTDOWN_DONE: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    ListRoot,
    Shutdown,
}

pub async fn wait_ready() -> bool {
    READY.wait().await
}

pub async fn list_root_and_wait() -> bool {
    ROOT_LIST_DONE.reset();
    COMMANDS.send(Command::ListRoot).await;
    ROOT_LIST_DONE.wait().await
}

pub async fn shutdown_and_wait() {
    SHUTDOWN_DONE.reset();
    COMMANDS.send(Command::Shutdown).await;
    SHUTDOWN_DONE.wait().await;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Partition {
    index: usize,
    partition_type: u8,
    first_lba: u32,
    sectors: u32,
}

struct SdPartition<'a, D> {
    device: &'a mut D,
    first_lba: u32,
    sector_count: u32,
    position: u64,
    // hadris is byte oriented while SD card is sector oriented.
    // keep one sector cached. This is particularly useful for FAT directory entries:
    // hadris reads 32-byte entries individually, so without this cache, we'd perform
    // the same physical 512-byte read repeatedly
    cached_sector: Option<u32>,
    sector: Aligned<A4, [u8; SECTOR_SIZE]>,
}

impl<'a, D> SdPartition<'a, D>
where
    D: RawBlockDevice<SECTOR_SIZE, Align = A4>,
{
    fn new(device: &'a mut D, first_lba: u32, sector_count: u32) -> Self {
        Self {
            device,
            first_lba,
            sector_count,
            position: 0,
            cached_sector: None,
            sector: Aligned([0; SECTOR_SIZE]),
        }
    }

    fn len_bytes(&self) -> u64 {
        self.sector_count as u64 * SECTOR_SIZE as u64
    }

    async fn load_sector(&mut self, relative_sector: u32) -> HadrisResult<()> {
        if relative_sector >= self.sector_count {
            return Err(HadrisError::new(
                HadrisErrorKind::UnexpectedEof,
                "sector outside SD partition",
            ));
        }

        if self.cached_sector == Some(relative_sector) {
            return Ok(());
        }

        let physical_lba = self
            .first_lba
            .checked_add(relative_sector)
            .ok_or_else(|| HadrisError::new(HadrisErrorKind::InvalidInput, "SD LBA overflow"))?;

        if let Err(error) = self
            .device
            .read(physical_lba, core::slice::from_mut(&mut self.sector))
            .await
        {
            println!("SD read failed at LBA {physical_lba}: {error:?}");
            return Err(HadrisError::new(
                HadrisErrorKind::Other,
                "SD block read failed",
            ));
        }

        self.cached_sector = Some(relative_sector);

        Ok(())
    }
}

impl<D> HadrisRead for SdPartition<'_, D>
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
        let mut copied = 0;

        while copied < bytes_to_read {
            let relative_sector =
                u32::try_from(self.position / SECTOR_SIZE as u64).map_err(|_| {
                    HadrisError::new(HadrisErrorKind::InvalidInput, "SD sector index overflow")
                })?;

            let offset = (self.position % SECTOR_SIZE as u64) as usize;

            self.load_sector(relative_sector).await?;

            let available = SECTOR_SIZE - offset;
            let wanted = bytes_to_read - copied;
            let count = core::cmp::min(available, wanted);

            buf[copied..copied + count].copy_from_slice(&self.sector[offset..offset + count]);
            copied += count;
            self.position += count as u64;
        }

        Ok(copied)
    }
}

impl<D> HadrisSeek for SdPartition<'_, D>
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

#[embassy_executor::task]
pub async fn storage_task(
    sdhost: SDHOST<'static>,
    clk: GPIO41<'static>,
    cmd: GPIO42<'static>,
    data0: GPIO40<'static>,
    mut sd_power: SdPower<'static>,
) {
    println!();
    println!("starting storage...");
    println!("SDMMC slot 1, native 1-bit");
    println!("  CLK  = GPIO41");
    println!("  CMD  = GPIO42");
    println!("  DAT0 = GPIO40");
    println!("  EN   = GPIO5 active-low");

    let controller = match SdHostController::new(sdhost, Config::default()) {
        Ok(controller) => controller,
        Err(error) => {
            println!("SDHOST initialization failed: {error:?}");
            serve_unavailable(&mut sd_power).await;
            return;
        }
    };

    let mut slot = match controller.slot::<1>(SlotConfig::default()) {
        Ok(slot) => slot
            .with_clk(clk)
            .with_cmd(cmd)
            .with_data0(data0)
            .into_async(),
        Err(error) => {
            println!("SD slot initialization failed: {error:?}");
            serve_unavailable(&mut sd_power).await;
            return;
        }
    };

    run_storage(&mut slot, &mut sd_power).await;
}

async fn run_storage<B>(bus: &mut B, sd_power: &mut SdPower<'_>)
where
    B: MmcBus,
{
    for attempt in 1..=MOUNT_ATTEMPTS {
        println!();
        println!("SD mount attempt {attempt}/{MOUNT_ATTEMPTS}");

        power_cycle_sd(sd_power).await;

        // this BlockDevice is intentionally reconstructed for every attempt
        // `sdio::BlockDevice` retains transfer-error state after a failed read. A fresh
        // object means a failed sector 0 test cannot poison the following power-cycle
        let mut card: BlockDevice<Card, _, _, SECTOR_SIZE> =
            match BlockDevice::new(&mut *bus, Delay, TARGET_FREQUENCY_HZ).await {
                Ok(card) => {
                    println!("SD card initialized at {} Hz", card.freq());
                    card
                }
                Err(error) => {
                    println!("SD card initialization failed: {error:?}");
                    continue;
                }
            };

        let mut sector_zero = [Aligned::<A4, _>([0u8; SECTOR_SIZE])];

        if let Err(error) = card.read(0, &mut sector_zero).await {
            println!("SD sector 0 read failed: {error:?}");
            continue;
        }

        println!("SD sector 0 read succeeded");

        match card.size().await {
            Ok(size) => println!("SD capacity: {size} bytes / {} MiB", size / 1024 / 1024),
            Err(error) => println!("could not read SD capacity: {error:?}"),
        }

        let Some(partition) = find_fat_partition(&sector_zero[0]) else {
            println!("no supported FAT partition found");
            continue;
        };

        println!("FAT partition {}:", partition.index);
        println!("  type: 0x{:02x}", partition.partition_type);
        println!("  first LBA: {}", partition.first_lba);
        println!("  sectors: {}", partition.sectors);

        let partition_device = SdPartition::new(&mut card, partition.first_lba, partition.sectors);
        let filesystem = match FatVolume::open(partition_device).await {
            Ok(filesystem) => filesystem,
            Err(error) => {
                println!("FAT mount failed: {error:?}");
                continue;
            }
        };

        let volume = filesystem.volume_info();
        println!("FAT filesystem mounted");
        println!("  OEM: {}", volume.oem_name());
        println!("  volume label: {}", volume.volume_label());
        println!("  filesystem field: {}", volume.fs_type_str());

        // from this point until shutdown, GPIO5 remains LOW and this task owns the complete
        // SD -> block device -> partition -> FAT stack
        READY.signal(true);

        serve_filesystem(&filesystem).await;

        // Command::Shutdown has been received
        // drop the filesystem before the block device, then drive GPIO5 HIGH.
        // the shutdown acknowledgement is not sent until that electrical state is established
        drop(filesystem);
        drop(card);
        sd_power.disable();
        println!("storage shut down; SD power off");

        SHUTDOWN_DONE.signal(());

        // keep ownership of the GPIO5 Output alive while main latches the RTC pad and
        // the power task enters deep sleep
        hold_forever().await;
        return;
    }

    serve_unavailable(sd_power).await;
}

async fn serve_filesystem<D>(filesystem: &FatVolume<D>)
where
    D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    loop {
        match COMMANDS.receive().await {
            Command::ListRoot => {
                let success = list_root(filesystem).await;
                ROOT_LIST_DONE.signal(success);
            }
            Command::Shutdown => {
                println!("storage shutdown requested");
                return;
            }
        }
    }
}

async fn serve_unavailable(sd_power: &mut SdPower<'_>) {
    sd_power.disable();
    println!("storage unavailable");

    READY.signal(false);

    loop {
        match COMMANDS.receive().await {
            Command::ListRoot => ROOT_LIST_DONE.signal(false),
            Command::Shutdown => {
                // GPIO5 is already HIGH, but establish it explicitly before
                // acknowledging the power path
                sd_power.disable();
                SHUTDOWN_DONE.signal(());

                hold_forever().await;
                return;
            }
        }
    }
}

async fn list_root<D>(filesystem: &FatVolume<D>) -> bool
where
    D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    println!();
    println!("root directory:");

    let root = filesystem.root_dir();
    let mut entries = root.entries();
    let mut count = 0usize;

    loop {
        let Some(result) = entries.next_entry().await else {
            break;
        };

        let entry = match result {
            Ok(DirectoryEntry::Entry(entry)) => entry,
            Err(error) => {
                println!("root directory read failed: {error:?}");
                return false;
            }
        };

        print_directory_entry(&entry);

        count += 1;
    }

    println!();
    println!("root entries: {count}");

    true
}

fn print_directory_entry(entry: &FileEntry) {
    if entry.is_directory() {
        print!("  [dir ] ");
    } else {
        print!("  [file] ");
    }

    print_entry_name(entry);

    if entry.is_file() {
        println!("  {} bytes", entry.len());
    } else {
        println!();
    }
}

fn print_entry_name(entry: &FileEntry) {
    if let Some(name) = entry.long_name() {
        for character in name.chars() {
            print!("{character}");
        }

        return;
    }

    match entry.short_name().try_as_str() {
        Ok(name) => print!("{name}"),
        Err(_) => print!("<OEM name {:02x?}>", entry.short_name().raw_bytes(),),
    }
}

fn find_fat_partition(sector: &[u8; SECTOR_SIZE]) -> Option<Partition> {
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

async fn power_cycle_sd(sd_power: &mut SdPower<'_>) {
    sd_power.disable();
    Timer::after(Duration::from_millis(POWER_OFF_MS)).await;

    sd_power.enable();
    Timer::after(Duration::from_millis(POWER_SETTLE_MS)).await;
}

async fn hold_forever() {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
