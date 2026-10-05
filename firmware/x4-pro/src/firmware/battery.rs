use cw2017::{Cw2017, profile::BatteryProfile};
use defmt::{Format, debug, info, warn};
use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Delay, Duration, Instant, Timer};
use esp_hal::{
    gpio::{Input, InputConfig},
    peripherals::GPIO21,
};

use crate::firmware::i2c_bus::SharedI2cDevice;

const SAMPLE_INTERVAL_SECS: u64 = 30;
const RETRY_INTERVAL_SECS: u64 = 1;
const CHARGE_STATUS_SETTLE_MS: u64 = 50;

/// exact 80-byte BATINFO profile recovered from the X4 PRO OEM Cw2017PowerHal
#[rustfmt::skip]
const X4_PRO_BATTERY_PROFILE: BatteryProfile = BatteryProfile::new([
    0x50, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00,
    0xaa, 0xbf, 0xb5, 0xb4,
    0xa4, 0x9c, 0xeb, 0xe2,

    0xdf, 0xe5, 0xca, 0xa0,
    0x8a, 0x62, 0x53, 0x48,
    0x40, 0x3a, 0x32, 0xb1,
    0xae, 0xda, 0xb5, 0xff,

    0xff, 0xff, 0xe8, 0xdb,
    0xd9, 0xd6, 0xd4, 0xd2,
    0xd0, 0xcb, 0xc3, 0xbc,
    0x9e, 0x87, 0x7b, 0x71,

    0x72, 0x7c, 0x8c, 0xa3,
    0xb7, 0xc8, 0xa5, 0x4f,
    0x00, 0x00, 0xab, 0x02,
    0x00, 0x00, 0x00, 0x00,

    0x00, 0x00, 0x64, 0x00,
    0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x23,
]);

#[derive(Debug, Format, Clone, Copy, PartialEq, Eq)]
pub struct BatteryReading {
    percent: u8,
    millivolts: u16,
    charging: bool,
}

impl BatteryReading {
    pub const fn new(percent: u8, millivolts: u16, charging: bool) -> Self {
        Self {
            percent,
            millivolts,
            charging,
        }
    }

    pub const fn percent(self) -> u8 {
        self.percent
    }

    pub const fn millivolts(self) -> u16 {
        self.millivolts
    }

    pub const fn charging(self) -> bool {
        self.charging
    }
}

pub static BATTERY_UPDATES: Signal<CriticalSectionRawMutex, BatteryReading> = Signal::new();

/// the charger's STAT line on GPIO21 is high while charging. The CW2017 has no current
/// register, so this is the only charging signal. Adapted from CrossPoint / FreeInk,
/// which recovered it from the stock firmware
#[embassy_executor::task]
pub async fn battery_task(i2c: SharedI2cDevice, charge_status: GPIO21<'static>) {
    let mut gauge = Cw2017::new(i2c);
    let mut charge_status = Input::new(charge_status, InputConfig::default());
    let mut delay = Delay;

    loop {
        // initialization is intentionally retrieved instead of permanently accepting
        // one transient shared bus failure
        match gauge.initialize(&mut delay, &X4_PRO_BATTERY_PROFILE).await {
            Ok(stats) => info!("CW2017 initialized: {}", stats),
            Err(error) => {
                warn!("CW2017 initialization failed: {:?}", error);
                Timer::after(Duration::from_secs(RETRY_INTERVAL_SECS)).await;
                continue;
            }
        }

        // stay in the sampling loop while the gauge is healthy.
        // a read failure drops us back to initialization so a sleeping reset or otherwise
        // inconsistent gauge can recover
        loop {
            let charge = match gauge.state_of_charge().await {
                Ok(charge) if charge.is_valid() => charge,
                Ok(charge) => {
                    warn!(
                        "CW2017 invalid SoC: {}.{:03}%",
                        charge.whole_percent(),
                        charge.fraction_256ths() as u32 * 1000 / 256,
                    );
                    break;
                }
                Err(error) => {
                    warn!("CW2017 SoC read failed: {:?}", error);
                    break;
                }
            };

            let voltage = match gauge.voltage().await {
                Ok(voltage) => voltage,
                Err(error) => {
                    warn!("CW2017 voltage read failed: {:?}", error);
                    break;
                }
            };

            let mut reading = BatteryReading::new(
                charge.whole_percent(),
                voltage.millivolts(),
                charge_status.is_high(),
            );
            debug!(
                "battery: {}%, {} mV, charging={}",
                reading.percent(),
                reading.millivolts(),
                reading.charging(),
            );

            BATTERY_UPDATES.signal(reading);

            // a charger plugged in or removed between samples re-signals the last
            // reading right away, without waiting for the next gauge read
            let next_sample = Instant::now() + Duration::from_secs(SAMPLE_INTERVAL_SECS);
            while let Either::Second(()) =
                select(Timer::at(next_sample), charge_status.wait_for_any_edge()).await
            {
                Timer::after(Duration::from_millis(CHARGE_STATUS_SETTLE_MS)).await;

                let charging = charge_status.is_high();
                if charging != reading.charging() {
                    reading =
                        BatteryReading::new(reading.percent(), reading.millivolts(), charging);
                    debug!("battery: charging={}", charging);
                    BATTERY_UPDATES.signal(reading);
                }
            }
        }

        Timer::after(Duration::from_secs(RETRY_INTERVAL_SECS)).await;
    }
}
