use bm8563::{Bm8563, DateTime};
use embassy_futures::select::{Either, select};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};
use embassy_time::{Duration, Timer};
use esp_println::println;

use crate::firmware::i2c_bus::SharedI2cDevice;

const INVALID_RECHECK_SECS: u64 = 30;
const ERROR_RETRY_SECS: u64 = 5;
const READBACK_TOLERANCE_SECS: u64 = 2;
const COMMAND_CAPACITY: usize = 2;

type MinuteKey = (u16, u8, u8, u8, u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcState {
    Invalid,
    Valid(DateTime),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcSyncFailure {
    Write,
    ReadBack,
    VoltageLow,
    Verification {
        requested: DateTime,
        actual: DateTime,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcSyncResult {
    Applied(DateTime),
    Failed(RtcSyncFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RtcCommand {
    SetUtc(DateTime),
}

pub static RTC_UPDATES: Signal<CriticalSectionRawMutex, RtcState> = Signal::new();
static RTC_COMMANDS: Channel<CriticalSectionRawMutex, RtcCommand, COMMAND_CAPACITY> =
    Channel::new();
static RTC_SYNC_RESULTS: Channel<CriticalSectionRawMutex, RtcSyncResult, COMMAND_CAPACITY> =
    Channel::new();

/// ask the RTC task to replace the BM8563 clock with a trusted UTC datetime.
///
/// the function returns only after the task has:
/// 1. written the datetime
/// 2. read the clock back
/// 3. checked the voltage-low flag
/// 4. verified that the read-back is within the allowed timing tolerance
///
/// There should currently be only 1 synchronization producer at a time
pub async fn sync_utc(datetime: DateTime) -> RtcSyncResult {
    RTC_COMMANDS.send(RtcCommand::SetUtc(datetime)).await;
    RTC_SYNC_RESULTS.receive().await
}

#[embassy_executor::task]
pub async fn rtc_task(i2c: SharedI2cDevice) {
    let mut rtc = Bm8563::new(i2c);
    let mut last_minute = None;
    let mut invalid_reported = false;
    // poll immediately on startup
    let mut next_poll = Duration::from_millis(1);

    println!("BM8563 service started at 0x51");

    loop {
        match select(RTC_COMMANDS.receive(), Timer::after(next_poll)).await {
            Either::First(command) => match command {
                RtcCommand::SetUtc(requested) => {
                    let result = synchronize(&mut rtc, requested).await;
                    match result {
                        RtcSyncResult::Applied(actual) => {
                            invalid_reported = false;
                            last_minute = Some(minute_key(actual));
                            // publish immediately. The application will replace an
                            // unavailable clock with the synchronized value
                            RTC_UPDATES.signal(RtcState::Valid(actual));
                            next_poll = delay_until_next_minute(actual);
                        }
                        RtcSyncResult::Failed(_) => {
                            // don't claim the requested time is valid. Return to ordinary
                            // polling shortly so the existing hardware state can be
                            // observed
                            next_poll = Duration::from_secs(ERROR_RETRY_SECS);
                        }
                    }
                }
            },
            Either::Second(_) => {
                next_poll = poll_rtc(&mut rtc, &mut last_minute, &mut invalid_reported).await;
            }
        }
    }
}

async fn synchronize(rtc: &mut Bm8563<SharedI2cDevice>, requested: DateTime) -> RtcSyncResult {
    println!(
        "BM8563 sync requested: {:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        requested.year(),
        requested.month(),
        requested.day(),
        requested.hour(),
        requested.minute(),
        requested.second(),
    );

    if let Err(error) = rtc.set_datetime(requested).await {
        println!("BM8563 sync write failed: {error:?}");
        return RtcSyncResult::Failed(RtcSyncFailure::Write);
    }

    let reading = match rtc.read().await {
        Ok(reading) => reading,
        Err(error) => {
            println!("BM8563 sync read-back failed: {error:?}");
            return RtcSyncResult::Failed(RtcSyncFailure::ReadBack);
        }
    };

    if reading.voltage_low() {
        println!("BM8563 sync read-back still has voltage-low flag set");
        return RtcSyncResult::Failed(RtcSyncFailure::VoltageLow);
    }

    let actual = reading.datetime();
    let verified = requested
        .elapsed_seconds_to(actual)
        .is_some_and(|elapsed| elapsed <= READBACK_TOLERANCE_SECS);

    if !verified {
        println!("BM8563 sync verification failed");
        println!(
            "  requested: {:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            requested.year(),
            requested.month(),
            requested.day(),
            requested.hour(),
            requested.minute(),
            requested.second(),
        );
        println!(
            "  actual:    {:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            actual.year(),
            actual.month(),
            actual.day(),
            actual.hour(),
            actual.minute(),
            actual.second(),
        );

        return RtcSyncResult::Failed(RtcSyncFailure::Verification { requested, actual });
    }

    println!(
        "BM8563 synchronized: {:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        actual.year(),
        actual.month(),
        actual.day(),
        actual.hour(),
        actual.minute(),
        actual.second(),
    );

    RtcSyncResult::Applied(actual)
}

async fn poll_rtc(
    rtc: &mut Bm8563<SharedI2cDevice>,
    last_minute: &mut Option<MinuteKey>,
    invalid_reported: &mut bool,
) -> Duration {
    let reading = match rtc.read().await {
        Ok(reading) => reading,
        Err(error) => {
            println!("BM8563 read failed: {error:?}");
            return Duration::from_secs(ERROR_RETRY_SECS);
        }
    };

    if reading.voltage_low() {
        if !*invalid_reported {
            println!("BM8563 clock invalid: voltage-low flag is set");
            RTC_UPDATES.signal(RtcState::Invalid);
            *invalid_reported = true;
            *last_minute = None;
        }

        return Duration::from_secs(INVALID_RECHECK_SECS);
    }

    *invalid_reported = false;

    let datetime = reading.datetime();
    let current_minute = minute_key(datetime);

    if *last_minute != Some(current_minute) {
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
        *last_minute = Some(current_minute);
    }

    delay_until_next_minute(datetime)
}

fn minute_key(datetime: DateTime) -> MinuteKey {
    (
        datetime.year(),
        datetime.month(),
        datetime.day(),
        datetime.hour(),
        datetime.minute(),
    )
}

fn delay_until_next_minute(datetime: DateTime) -> Duration {
    let second = datetime.second().min(59) as u64;
    Duration::from_secs((60 - second).max(1))
}
