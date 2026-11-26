use defmt::{info, warn};
use esp_hal::{
    gpio::DriveMode,
    ledc::{
        LSGlobalClkSource, Ledc, LowSpeed,
        channel::{
            Channel, ChannelIFace, Number as ChannelNumber, config::Config as ChannelConfig,
        },
        timer::{
            LSClockSource, Number as TimerNumber, Timer, TimerIFace,
            config::{Config as TimerConfig, Duty},
        },
    },
    peripherals::{GPIO8, GPIO9, LEDC},
    time::Rate,
};
use frontlight::{DualPwmFrontlight, Setting};
use static_cell::StaticCell;

const PWM_FREQUENCY_KHZ: u32 = 10;

/// X4 PRO uses 10-bit LEDC.
///
/// We intentionally use 0..1023 rather than relying on the HAL's reported top value
/// so our logical scale corresponds exactly to a 10-bit register.
const PWM_FULL_SCALE: u16 = 1023;

static LEDC_DRIVER: StaticCell<Ledc<'static>> = StaticCell::new();
static PWM_TIMER: StaticCell<Timer<'static, LowSpeed>> = StaticCell::new();

pub struct Frontlight {
    driver: DualPwmFrontlight<Channel<'static, LowSpeed>, Channel<'static, LowSpeed>>,
}

impl Frontlight {
    pub fn new(ledc_peripheral: LEDC<'static>, gpio8: GPIO8<'static>, gpio9: GPIO9<'static>) -> Self {
        let ledc = LEDC_DRIVER.init_with(|| {
            let mut ledc = Ledc::new(ledc_peripheral);
            ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
            ledc
        });

        let timer = PWM_TIMER.init_with(|| {
            let mut timer = ledc.timer::<LowSpeed>(TimerNumber::Timer0);
            timer
                .configure(TimerConfig {
                    duty: Duty::Duty10Bit,
                    clock_source: LSClockSource::APBClk,
                    frequency: Rate::from_khz(PWM_FREQUENCY_KHZ),
                })
                .unwrap();
            timer
        });

        // Confirmed on physical hardware:
        // - GPIO8 = cool LEDs
        // - GPIO9 = warm LEDs
        let mut cool = ledc.channel::<LowSpeed>(ChannelNumber::Channel0, gpio8);
        cool.configure(ChannelConfig {
            timer,
            duty_pct: 0,
            drive_mode: DriveMode::PushPull,
        })
        .unwrap();

        let mut warm = ledc.channel::<LowSpeed>(ChannelNumber::Channel1, gpio9);
        warm.configure(ChannelConfig {
            timer,
            duty_pct: 0,
            drive_mode: DriveMode::PushPull,
        })
        .unwrap();

        let mut driver = DualPwmFrontlight::new(cool, warm, PWM_FULL_SCALE).unwrap();
        driver.off().unwrap();

        info!(
            "frontlight ready cool_gpio=8 warm_gpio=9 frequency_khz={} resolution_bits=10",
            PWM_FREQUENCY_KHZ
        );

        Self { driver }
    }

    pub fn set(&mut self, setting: Setting) {
        if let Err(error) = self.driver.set(setting) {
            warn!("frontlight set failed: {:?}", error);
        }
    }

    pub fn off(&mut self) {
        if let Err(error) = self.driver.off() {
            warn!("frontlight off failed: {:?}", error);
        }
    }
}
