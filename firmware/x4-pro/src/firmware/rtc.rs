use bm8563::{Bm8563, DateTime};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use esp_println::println;

use crate::firmware::i2c_bus::SharedI2cDevice;

const INVALID_RECHECK_SECS: u64 = 30;
const ERROR_RETRY_SECS: u64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcState {
    Invalid,
    Valid(DateTime),
}

pub static RTC_UPDATES: Signal<CriticalSectionRawMutex, RtcState> = Signal::new();

#[embassy_executor::task]
pub async fn rtc_task(i2c: SharedI2cDevice) {
    let mut rtc = Bm8563::new(i2c);
    let mut last_minute = None;
    let mut invalid_reported = false;
    println!("BM8563 service started at 0x51");

    loop {
        let reading = match rtc.read().await {
            Ok(reading) => reading,
            Err(error) => {
                println!("BM8563 read failed: {:?}", error,);
                Timer::after(Duration::from_secs(ERROR_RETRY_SECS)).await;
                continue;
            }
        };

        if reading.voltage_low() {
            if !invalid_reported {
                println!("BM8563 clock invalid: voltage-low flag is set");
                RTC_UPDATES.signal(RtcState::Invalid);
                invalid_reported = true;
                last_minute = None;
            }

            Timer::after(Duration::from_secs(INVALID_RECHECK_SECS)).await;
            continue;
        }

        invalid_reported = false;

        let datetime = reading.datetime();

        let minute_key = (
            datetime.year(),
            datetime.month(),
            datetime.day(),
            datetime.hour(),
            datetime.minute(),
        );

        if last_minute != Some(minute_key) {
            println!(
                "rtc: {:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                datetime.year(),
                datetime.month(),
                datetime.day(),
                datetime.hour(),
                datetime.minute(),
                datetime.second(),
            );

            RTC_UPDATES.signal(RtcState::Valid(datetime));
            last_minute = Some(minute_key);
        }

        let second = datetime.second().min(59) as u64;
        Timer::after(Duration::from_secs((60 - second).max(1))).await;
    }
}
