use aligned::{A4, Aligned};
use block_device_driver::BlockDevice as RawBlockDevice;
use defmt::{Debug2Format, debug, error, info, warn};
use embassy_time::{Delay, Duration, Timer};
use esp_hal::{
    peripherals::{GPIO19, GPIO20, GPIO40, GPIO41, GPIO42, SDHOST, USB_FS},
    sdmmc::{Config, SdHostController, SlotConfig},
};
use hadris_fat::r#async::FatVolume;
use sdio::{BlockDevice, MmcBus, sd::Card};

use crate::firmware::{
    power::SdPower,
    storage::{
        partition::{SECTOR_SIZE, SdPartition, find_fat_partition},
        service::{
            FilesystemExit, serve_filesystem, serve_unavailable_requests, serve_usb_drive_requests,
            signal_ready, signal_shutdown_done, signal_usb_drive_ready,
        },
    },
    usb_mass_storage::{self, UsbMassStorageExit},
};

const TARGET_FREQUENCY_HZ: u32 = 40_000_000;

const MOUNT_ATTEMPTS: usize = 4;

const POWER_OFF_MS: u64 = 80;
const POWER_SETTLE_MS: u64 = 120;

#[embassy_executor::task]
pub async fn storage_task(
    sdhost: SDHOST<'static>,
    clk: GPIO41<'static>,
    cmd: GPIO42<'static>,
    data0: GPIO40<'static>,
    mut sd_power: SdPower<'static>,
    usb_fs: USB_FS<'static>,
    usb_dp: GPIO20<'static>,
    usb_dm: GPIO19<'static>,
) {
    info!("storage service starting",);

    debug!("SDMMC configuration slot=1 clk_gpio=41 cmd_gpio=42 data0_gpio=40 enable_gpio=5",);

    let controller = match SdHostController::new(sdhost, Config::default()) {
        Ok(controller) => controller,

        Err(error) => {
            error!("SDHOST initialization failed: {:?}", error,);

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
            error!("SD slot initialization failed: {:?}", error,);

            serve_unavailable(&mut sd_power).await;

            return;
        }
    };

    run_storage(&mut slot, &mut sd_power, usb_fs, usb_dp, usb_dm).await;
}

async fn run_storage<B>(
    bus: &mut B,
    sd_power: &mut SdPower<'_>,
    usb_fs: USB_FS<'static>,
    usb_dp: GPIO20<'static>,
    usb_dm: GPIO19<'static>,
) where
    B: MmcBus,
{
    for attempt in 1..=MOUNT_ATTEMPTS {
        info!("SD mount attempt {}/{}", attempt, MOUNT_ATTEMPTS);

        power_cycle_sd(sd_power).await;

        // reconstruct the BlockDevice for each attempt. sdio::BlockDevice retains
        // transfer-error state after a failed read, so a failed sector-0 test must not
        // poison the next power-cycle.
        let mut card: BlockDevice<Card, _, _, SECTOR_SIZE> =
            match BlockDevice::new(&mut *bus, Delay, TARGET_FREQUENCY_HZ).await {
                Ok(card) => {
                    info!("SD card initialized at {} Hz", card.freq());
                    card
                }
                Err(error) => {
                    warn!("SD card initialization failed: {:?}", error);
                    continue;
                }
            };

        let mut sector_zero = [Aligned::<A4, _>([0u8; SECTOR_SIZE])];

        if let Err(error) = card.read(0, &mut sector_zero).await {
            warn!("SD sector 0 read failed: {:?}", error);
            continue;
        }

        debug!("SD sector 0 read succeeded");

        match card.size().await {
            Ok(size) => info!("SD capacity: {} bytes / {} MiB", size, size / 1024 / 1024),
            Err(error) => warn!("could not read SD capacity: {:?}", error),
        }

        let Some(partition) = find_fat_partition(&sector_zero[0]) else {
            warn!("no supported FAT partition found");
            continue;
        };

        info!(
            "FAT partition index={} type={:#04x} first_lba={} sectors={}",
            partition.index, partition.partition_type, partition.first_lba, partition.sectors,
        );

        let partition_device = SdPartition::new(&mut card, partition.first_lba, partition.sectors);

        let filesystem = match FatVolume::open(partition_device).await {
            Ok(filesystem) => filesystem,
            Err(error) => {
                warn!("FAT mount failed: {:?}", error);
                continue;
            }
        };

        let volume = filesystem.volume_info();

        info!(
            "FAT filesystem mounted oem={} label={} type={}",
            volume.oem_name(),
            volume.volume_label(),
            volume.fs_type_str(),
        );

        // From here until a transition, GPIO5 stays LOW and this task owns the entire
        // SD -> block device -> partition -> FAT stack.
        signal_ready(true);

        let exit = serve_filesystem(&filesystem).await;

        // This is the ownership boundary shared by both shutdown and USB Drive. Dropping
        // FatVolume also drops SdPartition, releasing its mutable borrow of `card`.
        drop(filesystem);

        match exit {
            FilesystemExit::Shutdown => {
                // Deep sleep does not need the raw card. Tear the SD stack down
                // and remove card power before acknowledging shutdown.
                drop(card);

                sd_power.disable();

                info!("storage shut down; SD power off");

                signal_shutdown_done();

                // Preserve ownership of the GPIO output while main prepares the RTC holds
                // and enters deep sleep.
                hold_forever().await;

                return;
            }

            FilesystemExit::UsbDrive => {
                signal_ready(false);

                let sector_count = match usb_sector_count(&mut card).await {
                    Some(sector_count) => sector_count,
                    None => {
                        error!("USB drive cannot determine raw SD capacity");

                        signal_usb_drive_ready(false);
                        drop(card);
                        serve_unavailable(sd_power).await;

                        return;
                    }
                };

                info!(
                    "FAT filesystem detached, starting USB drive sectors={}",
                    sector_count
                );

                // From this point until USB MSC exits, filesystem calls are unavailable.
                // The raw card stays exclusively owned by this task
                signal_usb_drive_ready(true);

                let exit = usb_mass_storage::run(
                    &mut card,
                    sector_count,
                    usb_fs,
                    usb_dp,
                    usb_dm,
                    serve_usb_drive_requests(),
                )
                .await;

                match exit {
                    UsbMassStorageExit::Shutdown => {
                        drop(card);
                        sd_power.disable();

                        info!("USB drive stopped for system shutdown");

                        signal_shutdown_done();
                        hold_forever().await;

                        return;
                    }
                    UsbMassStorageExit::Ejected => {
                        // The host has completed START STOP UNIT with LOEJ.
                        // USB has already been disabled by `usb_mass_storage`.
                        //
                        // Rebooting is simpler and safer than trying to rebuild every
                        // app-side storage/reader handle in place
                        drop(card);
                        sd_power.disable();

                        info!("USB drive ejected, restarting");

                        esp_hal::system::software_reset();
                    }
                }
            }
        }
    }

    serve_unavailable(sd_power).await;
}

async fn serve_unavailable(sd_power: &mut SdPower<'_>) {
    sd_power.disable();

    error!("storage unavailable",);

    // GPIO5 remains disabled for the entire unavailable service lifetime.
    serve_unavailable_requests().await;

    // establish the state again before waiting forever, mirroring the normal shutdown path.
    sd_power.disable();

    hold_forever().await;
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

async fn usb_sector_count<D>(card: &mut D) -> Option<u32>
where
    D: RawBlockDevice<SECTOR_SIZE>,
{
    let size = match card.size().await {
        Ok(size) => size,

        Err(error) => {
            warn!(
                "USB Drive SD capacity read failed: {}",
                Debug2Format(&error),
            );

            return None;
        }
    };

    if size == 0 || size % SECTOR_SIZE as u64 != 0 {
        warn!("USB Drive invalid SD size={}", size);
        return None;
    }

    let sectors = size / SECTOR_SIZE as u64;

    match u32::try_from(sectors) {
        Ok(sectors) if sectors != 0 => Some(sectors),
        _ => {
            warn!("USB Drive SD sector count out of range={}", sectors);
            None
        }
    }
}
