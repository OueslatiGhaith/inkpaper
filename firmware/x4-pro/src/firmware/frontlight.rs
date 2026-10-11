use defmt::{info, warn};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use esp_hal::{
    gpio::DriveMode,
    ledc::{
        LSGlobalClkSource, Ledc, LowSpeed,
        channel::{ChannelIFace, Number as ChannelNumber, config::Config as ChannelConfig},
        timer::{
            LSClockSource, Number as TimerNumber, TimerIFace,
            config::{Config as TimerConfig, Duty},
        },
    },
    peripherals::{GPIO8, GPIO9, LEDC},
    time::Rate,
};
use frontlight::{DualPwmFrontlight, Setting};

use crate::firmware::rpc::{self, Reply};

const PWM_FREQUENCY_KHZ: u32 = 10;

/// X4 PRO uses 10-bit LEDC
///
/// we intentionally use 0..1023 rather than relying on the HAL's reported top value
/// so our logical scale corresponds exactly to a 10-bit register
const PWM_FULL_SCALE: u16 = 1023;

const COMMAND_CAPACITY: usize = 4;

static COMMANDS: Channel<CriticalSectionRawMutex, Command, COMMAND_CAPACITY> = Channel::new();

#[derive(Debug)]
enum Command {
    Set(Setting),
    Off(Reply<()>),
}

pub async fn set(setting: Setting) {
    COMMANDS.send(Command::Set(setting)).await;
}

pub async fn frontlight_off_and_wait() {
    rpc::call(&COMMANDS, Command::Off).await;
}

#[embassy_executor::task]
pub async fn frontlight_task(
    ledc_peripheral: LEDC<'static>,
    gpio8: GPIO8<'static>,
    gpio9: GPIO9<'static>,
) {
    let mut ledc = Ledc::new(ledc_peripheral);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

    let mut timer = ledc.timer::<LowSpeed>(TimerNumber::Timer0);
    timer
        .configure(TimerConfig {
            duty: Duty::Duty10Bit,
            clock_source: LSClockSource::APBClk,
            frequency: Rate::from_khz(PWM_FREQUENCY_KHZ),
        })
        .unwrap();

    // confirmed on physical hardware
    // - GPIO8 = cool LEDs
    // - GPIO9 = warm LEDs
    let mut cool = ledc.channel::<LowSpeed>(ChannelNumber::Channel0, gpio8);
    cool.configure(ChannelConfig {
        timer: &timer,
        duty_pct: 0,
        drive_mode: DriveMode::PushPull,
    })
    .unwrap();

    let mut warm = ledc.channel::<LowSpeed>(ChannelNumber::Channel1, gpio9);
    warm.configure(ChannelConfig {
        timer: &timer,
        duty_pct: 0,
        drive_mode: DriveMode::PushPull,
    })
    .unwrap();

    let mut frontlight = DualPwmFrontlight::new(cool, warm, PWM_FULL_SCALE).unwrap();
    frontlight.off().unwrap();

    info!(
        "frontlight ready cool_gpio=8 warm_gpio=9 frequency_khz={} resolution_bits=10",
        PWM_FREQUENCY_KHZ
    );

    loop {
        match COMMANDS.receive().await {
            Command::Set(setting) => {
                if let Err(error) = frontlight.set(setting) {
                    warn!("frontlight set failed: {:?}", error);
                }
            }
            Command::Off(reply) => {
                if let Err(error) = frontlight.off() {
                    warn!("frontlight off failed: {:?}", error);
                }

                // reply even after an unexpected PWM error. The power path must never
                // deadlock while trying to enter deep sleep
                reply.send(());
            }
        }
    }
}
