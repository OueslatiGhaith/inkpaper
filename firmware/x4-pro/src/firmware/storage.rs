use aligned::{A4, Aligned};
use block_device_driver::BlockDevice as _;
use embassy_time::{Delay, Duration, Timer};
use embedded_hal_async::delay::DelayNs;
use esp_println::println;
use sdio::{BlockDevice, MmcBus, sd::Card};

use crate::firmware::power::PowerRails;

const TARGET_FREQUENCY_HZ: u32 = 40_000_000;
const MOUNT_ATTEMPTS: usize = 4;
const POWER_OFF_MS: u64 = 80;
const POWER_SETTLE_MS: u64 = 120;
const SECTOR_SIZE: usize = 512;
const MBR_PARTITION_TABLE_OFFSET: usize = 446;
const MBR_PARTITION_ENTRY_SIZE: usize = 16;
const MBR_PARTITION_COUNT: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Partition {
    index: usize,
    bootable: bool,
    partition_type: u8,
    first_lba: u32,
    sectors: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MbrInspection {
    first_data_partition: Option<Partition>,
    populated_entries: usize,
    protective_gpt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FatKind {
    Fat12,
    Fat16,
    Fat32,
}

impl FatKind {
    fn name(self) -> &'static str {
        match self {
            FatKind::Fat12 => "FAT12",
            FatKind::Fat16 => "FAT16",
            FatKind::Fat32 => "FAT32",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FatInfo {
    kind: FatKind,
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    fats: u8,
    fat_size_sectors: u32,
    total_sectors: u32,
    cluster_count: u32,
    root_cluster: Option<u32>,
}

pub async fn probe_sd_card<B>(bus: &mut B, rails: &mut PowerRails<'_>) -> bool
where
    B: MmcBus,
{
    println!();
    println!("probing SD card...");
    println!("SDMMC slot 1, native 1-bit");
    println!("  CLK  = GPIO41");
    println!("  CMD  = GPIO42");
    println!("  DAT0 = GPIO40");
    println!("  EN   = GPIO5 active-low");

    for attempt in 1..=MOUNT_ATTEMPTS {
        println!();
        println!("SD mount attempt {attempt}/{MOUNT_ATTEMPTS}");

        power_cycle_sd(rails).await;

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

        let mut sector_0 = [Aligned::<A4, _>([0u8; SECTOR_SIZE])];
        println!("reading sector 0...");

        if let Err(error) = card.read(0, &mut sector_0).await {
            println!("SD sector 0 read failed: {error:?}");
            continue;
        }
        println!("SD sector 0 read succeeded");

        match card.size().await {
            Ok(size) => println!("SD capacity: {size} bytes / {} MiB", size / 1024 / 1024),
            Err(error) => println!("could not read SD capacity: {error:?}"),
        }

        inspect_disk_layout(&mut card, &sector_0[0]).await;

        print_sector_zero(&sector_0[0]);

        rails.disable_sd();
        println!("SDMMC hardware probe passed");

        return true;
    }

    rails.disable_sd();

    println!();
    println!("SDMMC hardware probe failed after {MOUNT_ATTEMPTS} attempts");

    false
}

async fn power_cycle_sd(rails: &mut PowerRails<'_>) {
    // X4 PRO SD data-path/power enable is active-low on GPIO5
    // a complete reset is:
    // HIGH -> 80ms -> LOW -> 120ms
    // the card remains enabled LOW after initialization
    rails.disable_sd();
    Timer::after(Duration::from_millis(POWER_OFF_MS)).await;

    rails.enable_sd();
    Timer::after(Duration::from_millis(POWER_SETTLE_MS)).await;
}

fn print_sector_zero(sector: &[u8; SECTOR_SIZE]) {
    println!("SD sector 0 first 64 bytes:");

    for row in 0..4 {
        let start = row * 16;
        let end = start + 16;

        println!("{start:03x}: {:02x?}", &sector[start..end]);
    }

    println!("SD boot signature: {:02x} {:02x}", sector[510], sector[511]);
}

async fn inspect_disk_layout<B, D>(
    card: &mut BlockDevice<Card, &mut B, D, SECTOR_SIZE>,
    sector_0: &[u8; SECTOR_SIZE],
) where
    B: MmcBus,
    D: DelayNs,
{
    println!();
    println!("disk layout:");

    // a FAT or exFAT volume can legally occupy the entire device without an MBR
    // partition table ("superfloppy")
    // check that first. This also avoids accidentally interpreting bytes in a FAT
    // boot sector as MBR partition entries
    if looks_like_volume_boot_sector(sector_0) {
        println!("  no partition offset: filesystem starts at LBA 0");
        inspect_volume_boot_sector(0, sector_0);
        return;
    }

    if !has_boot_signature(sector_0) {
        println!("  LBA 0 has no 55 aa signature");
        println!("  partition layout could not be identified");
        return;
    }

    let mbr = inspect_mbr(sector_0);
    if mbr.protective_gpt {
        println!();
        println!("protective GPT MBR detected");
        println!("GPT partition parsing is not implemented in this probe");
        return;
    }

    let Some(partition) = mbr.first_data_partition else {
        if mbr.populated_entries == 0 {
            println!("MBR contains no populated partition entries");
        } else {
            println!("MBR contains no directly inspectable primary data partition");
        }

        return;
    };

    println!();
    println!(
        "reading partition {} boot sector at LBA {}...",
        partition.index, partition.first_lba,
    );

    let mut boot_sector = [Aligned::<A4, _>([0u8; SECTOR_SIZE])];

    match card.read(partition.first_lba, &mut boot_sector).await {
        Ok(()) => {
            println!("partition boot sector read succeeded");
            inspect_volume_boot_sector(partition.first_lba, &boot_sector[0]);
        }
        Err(error) => println!("partition boot sector read failed: {error:?}"),
    }
}

fn inspect_mbr(sector: &[u8; SECTOR_SIZE]) -> MbrInspection {
    println!("  partition scheme: MBR");
    println!("  signature: {:02x} {:02x}", sector[510], sector[511]);

    let mut first_data_partition = None;
    let mut populated_entries = 0;
    let mut protective_gpt = false;

    for index in 0..MBR_PARTITION_COUNT {
        let offset = MBR_PARTITION_TABLE_OFFSET + index * MBR_PARTITION_ENTRY_SIZE;
        let entry = &sector[offset..offset + MBR_PARTITION_ENTRY_SIZE];
        let partition = Partition {
            index,
            bootable: entry[0] == 0x80,
            partition_type: entry[4],
            first_lba: le_u32(&entry[8..12]),
            sectors: le_u32(&entry[12..16]),
        };

        if partition.partition_type == 0 || partition.sectors == 0 {
            println!("  partition {index}: empty");
            continue;
        }

        populated_entries += 1;

        if partition.partition_type == 0xee {
            protective_gpt = true;
        }

        println!("  partition {}:", partition.index);
        println!("    bootable: {}", partition.bootable);
        println!(
            "    type: 0x{:02x} ({})",
            partition.partition_type,
            partition_type_name(partition.partition_type),
        );
        println!("    first LBA: {}", partition.first_lba);
        println!("    sectors: {}", partition.sectors);
        println!(
            "    size: {} MiB",
            partition.sectors as u64 * SECTOR_SIZE as u64 / 1024 / 1024,
        );

        if first_data_partition.is_none() && is_primary_data_partition(partition.partition_type) {
            first_data_partition = Some(partition);
        }
    }

    MbrInspection {
        first_data_partition,
        populated_entries,
        protective_gpt,
    }
}

fn inspect_volume_boot_sector(lba: u32, sector: &[u8; SECTOR_SIZE]) {
    println!();
    println!("volume boot sector at LBA {lba}:");
    println!("  signature: {:02x} {:02x}", sector[510], sector[511]);

    if is_exfat(sector) {
        inspect_exfat(sector);
        return;
    }

    if is_ntfs(sector) {
        println!("  filesystem: NTFS");
        println!("  OEM: {}", ascii_field(&sector[3..11]));
        return;
    }

    if let Some(info) = parse_fat_info(sector) {
        inspect_fat(sector, info);
        return;
    }

    println!("  filesystem: unknown");
    println!("  OEM bytes: {:02x?}", &sector[3..11]);
    println!("  first 16 bytes: {:02x?}", &sector[..16]);
}

fn inspect_fat(sector: &[u8; SECTOR_SIZE], info: FatInfo) {
    println!("  filesystem: {}", info.kind.name());
    println!("  OEM: {}", ascii_field(&sector[3..11]));
    println!("  bytes/sector: {}", info.bytes_per_sector);
    println!("  sectors/cluster: {}", info.sectors_per_cluster);
    println!(
        "  cluster size: {} bytes",
        info.bytes_per_sector as u32 * info.sectors_per_cluster as u32,
    );
    println!("  reserved sectors: {}", info.reserved_sectors);
    println!("  FAT copies: {}", info.fats);
    println!("  sectors/FAT: {}", info.fat_size_sectors);
    println!("  total sectors: {}", info.total_sectors);
    println!("  data clusters: {}", info.cluster_count);

    match info.kind {
        FatKind::Fat32 => {
            println!("  root cluster: {}", info.root_cluster.unwrap_or(0));
            println!("  volume label: {}", ascii_field(&sector[71..82]));
            println!("  filesystem type field: {}", ascii_field(&sector[82..90]));
        }
        FatKind::Fat12 | FatKind::Fat16 => {
            println!("  volume label: {}", ascii_field(&sector[43..54]));
            println!("  filesystem type field: {}", ascii_field(&sector[54..62]));
        }
    }
}

fn inspect_exfat(sector: &[u8; SECTOR_SIZE]) {
    let partition_offset = le_u64(&sector[64..72]);
    let volume_length = le_u64(&sector[72..80]);
    let fat_offset = le_u32(&sector[80..84]);
    let fat_length = le_u32(&sector[84..88]);
    let cluster_heap_offset = le_u32(&sector[88..92]);
    let cluster_count = le_u32(&sector[92..96]);
    let root_cluster = le_u32(&sector[96..100]);
    let bytes_per_sector_shift = sector[108];
    let sectors_per_cluster_shift = sector[109];
    let bytes_per_sector = 1u32.checked_shl(bytes_per_sector_shift as u32);
    let sectors_per_cluster = 1u32.checked_shl(sectors_per_cluster_shift as u32);

    println!("  filesystem: exFAT");
    println!("  OEM: {}", ascii_field(&sector[3..11]));
    println!("  partition offset: {partition_offset}");
    println!("  volume length: {volume_length} sectors");
    println!("  FAT offset: {fat_offset}");
    println!("  FAT length: {fat_length} sectors");
    println!("  cluster heap offset: {cluster_heap_offset}");
    println!("  cluster count: {cluster_count}");
    println!("  root directory cluster: {root_cluster}");

    match bytes_per_sector {
        Some(value) => println!("  bytes/sector: {value}"),
        None => println!("  invalid bytes/sector shift: {bytes_per_sector_shift}"),
    }

    match sectors_per_cluster {
        Some(value) => {
            println!("  sectors/cluster: {value}");
            if let Some(bytes_per_sector) = bytes_per_sector {
                println!(
                    "  cluster size: {} bytes",
                    bytes_per_sector as u64 * value as u64,
                );
            }
        }
        None => println!("  invalid sectors/cluster shift: {sectors_per_cluster_shift}"),
    }

    println!("  FAT copies: {}", sector[110]);
    println!("  percent in use: {}", sector[112]);
}

fn parse_fat_info(sector: &[u8; SECTOR_SIZE]) -> Option<FatInfo> {
    let bytes_per_sector = le_u16(&sector[11..13]);
    if !matches!(bytes_per_sector, 512 | 1024 | 2048 | 4096) {
        return None;
    }

    let sectors_per_cluster = sector[13];
    if sectors_per_cluster == 0 || !sectors_per_cluster.is_power_of_two() {
        return None;
    }

    let reserved_sectors = le_u16(&sector[14..16]);
    let fats = sector[16];

    if reserved_sectors == 0 || fats == 0 {
        return None;
    }

    let root_entry_count = le_u16(&sector[17..19]);
    let total_sectors_16 = le_u16(&sector[19..21]);
    let fat_size_16 = le_u16(&sector[22..24]);
    let total_sectors_32 = le_u32(&sector[32..36]);
    let fat_size_32 = le_u32(&sector[36..40]);
    let total_sectors = if total_sectors_16 != 0 {
        total_sectors_16 as u32
    } else {
        total_sectors_32
    };
    let fat_size_sectors = if fat_size_16 != 0 {
        fat_size_16 as u32
    } else {
        fat_size_32
    };
    if total_sectors == 0 || fat_size_sectors == 0 {
        return None;
    }
    let root_dir_sectors =
        (root_entry_count as u32 * 32 + bytes_per_sector as u32 - 1) / bytes_per_sector as u32;
    let non_data_sectors =
        reserved_sectors as u32 + fats as u32 * fat_size_sectors + root_dir_sectors;
    let data_sectors = total_sectors.checked_sub(non_data_sectors)?;
    let cluster_count = data_sectors / sectors_per_cluster as u32;

    if cluster_count == 0 {
        return None;
    }

    let kind = if cluster_count < 4_085 {
        FatKind::Fat12
    } else if cluster_count < 65_525 {
        FatKind::Fat16
    } else {
        FatKind::Fat32
    };

    let root_cluster = if kind == FatKind::Fat32 {
        Some(le_u32(&sector[44..48]))
    } else {
        None
    };

    Some(FatInfo {
        kind,
        bytes_per_sector,
        sectors_per_cluster,
        reserved_sectors,
        fats,
        fat_size_sectors,
        total_sectors,
        cluster_count,
        root_cluster,
    })
}

fn looks_like_volume_boot_sector(sector: &[u8; SECTOR_SIZE]) -> bool {
    if !has_boot_signature(sector) {
        return false;
    }

    is_exfat(sector) || is_ntfs(sector) || parse_fat_info(sector).is_some()
}

fn is_exfat(sector: &[u8; SECTOR_SIZE]) -> bool {
    &sector[3..11] == b"EXFAT   "
}

fn is_ntfs(sector: &[u8; SECTOR_SIZE]) -> bool {
    &sector[3..11] == b"NTFS    "
}

fn has_boot_signature(sector: &[u8; SECTOR_SIZE]) -> bool {
    sector[510] == 0x55 && sector[511] == 0xaa
}

fn is_primary_data_partition(partition_type: u8) -> bool {
    !matches!(partition_type, 0x00 | 0x05 | 0x0f | 0x85 | 0xee)
}

fn partition_type_name(partition_type: u8) -> &'static str {
    match partition_type {
        0x01 => "FAT12",
        0x04 => "FAT16 <32 MiB",
        0x05 => "extended",
        0x06 => "FAT16",
        0x07 => "NTFS/exFAT",
        0x0b => "FAT32",
        0x0c => "FAT32 LBA",
        0x0e => "FAT16 LBA",
        0x0f => "extended LBA",
        0x82 => "Linux swap",
        0x83 => "Linux",
        0x85 => "Linux extended",
        0xee => "GPT protective",
        0xef => "EFI system",
        _ => "unknown",
    }
}

fn ascii_field(bytes: &[u8]) -> &str {
    match core::str::from_utf8(bytes) {
        Ok(value) => value.trim(),
        Err(_) => "<non-ASCII>",
    }
}

fn le_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn le_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ])
}
