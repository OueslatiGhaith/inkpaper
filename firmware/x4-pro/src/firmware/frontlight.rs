use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};
use embassy_time::{Duration, Timer};
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
use esp_println::println;
use frontlight::{DualPwmFrontlight, Percent, Setting};

const PWM_FREQUENCY_KHZ: u32 = 10;

/// X4 PRO uses 10-bit LEDC
///
/// we intentionally use 0..1023 rather than relying on the HAL's reported top value
/// so our logical scale corresponds exactly to a 10-bit register
const PWM_FULL_SCALE: u16 = 1023;

const COMMAND_CAPACITY: usize = 4;

static COMMANDS: Channel<CriticalSectionRawMutex, Command, COMMAND_CAPACITY> = Channel::new();
static READY: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static OFF_APPLIED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Set(Setting),
    Off,
}

pub async fn wait_ready() {
    READY.wait().await;
}

pub async fn set(setting: Setting) {
    COMMANDS.send(Command::Set(setting)).await;
}

pub async fn frontlight_off_and_wait() {
    // remove any stale acknowledgement before issuing the command whose completion
    // we actually care about
    OFF_APPLIED.reset();

    COMMANDS.send(Command::Off).await;
    OFF_APPLIED.wait().await;
}

#[embassy_executor::task]
pub async fn frontligh_task(
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

    let mut channel_a = ledc.channel::<LowSpeed>(ChannelNumber::Channel0, gpio8);
    channel_a
        .configure(ChannelConfig {
            timer: &timer,
            duty_pct: 0,
            drive_mode: DriveMode::PushPull,
        })
        .unwrap();

    let mut channel_b = ledc.channel::<LowSpeed>(ChannelNumber::Channel1, gpio9);
    channel_b
        .configure(ChannelConfig {
            timer: &timer,
            duty_pct: 0,
            drive_mode: DriveMode::PushPull,
        })
        .unwrap();

    let mut frontlight = DualPwmFrontlight::new(channel_a, channel_b, PWM_FULL_SCALE).unwrap();
    frontlight.off().unwrap();

    println!("frontlight ready: GPIO8=A GPIO9=B, 10 kHz / 10-bit");
    READY.signal(());

    loop {
        match COMMANDS.receive().await {
            Command::Set(setting) => {
                if let Err(error) = frontlight.set(setting) {
                    println!("frontlight set failed: {error:?}");
                }
            }
            Command::Off => {
                if let Err(error) = frontlight.off() {
                    println!("frontlight off failed: {error:?}");
                }

                // signal even after an unexpected PWM error. The power path must never
                // deadlock while trying to ender deep sleep
                OFF_APPLIED.signal(());
            }
        }
    }
}

pub async fn run_bringup_test() {
    wait_ready().await;

    let brightness = Percent::new(30).unwrap();

    println!("frontlight test: GPIO8 / channel A");
    set(Setting::new(brightness, Percent::ZERO)).await;
    Timer::after(Duration::from_secs(2)).await;

    println!("frontlight test: GPIO9 / channel B");
    set(Setting::new(brightness, Percent::FULL)).await;
    Timer::after(Duration::from_secs(2)).await;

    println!("frontlight test: 50/50 mix");
    set(Setting::new(brightness, Percent::new(50).unwrap())).await;
    Timer::after(Duration::from_secs(2)).await;

    println!("frontlight test: off");
    frontlight_off_and_wait().await;

    println!("frontlight bring-up complete");
}
