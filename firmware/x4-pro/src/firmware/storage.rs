use aligned::{A4, Aligned};
use block_device_driver::BlockDevice as _;
use embassy_time::{Delay, Duration, Timer};
use esp_println::println;
use sdio::{BlockDevice, MmcBus, sd::Card};

use crate::firmware::power::PowerRails;

const TARGET_FREQUENCY_HZ: u32 = 40_000_000;
const MOUNT_ATTEMPTS: usize = 4;
const POWER_OFF_MS: u64 = 80;
const POWER_SETTLE_MS: u64 = 120;
const SECTOR_SIZE: usize = 512;

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
            Ok(size) => println!("SD capacity: {size} bytes / {} MiB", size / 1024 / 1024,),
            Err(error) => println!("could not read SD capacity: {error:?}"),
        }

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

        println!("{start:03x}: {:02x?}", &sector[start..end],);
    }

    println!("SD boot signature: {:02x} {:02x}", sector[510], sector[511],);
}
