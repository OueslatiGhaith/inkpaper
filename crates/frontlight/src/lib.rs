#![no_std]

use embedded_hal::pwm::SetDutyCycle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PercentError {
    value: u8,
}

impl PercentError {
    pub const fn value(self) -> u8 {
        self.value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Percent(u8);

impl Percent {
    pub const ZERO: Self = Self(0);
    pub const FULL: Self = Self(100);

    pub const fn new(value: u8) -> Result<Self, PercentError> {
        if value <= 100 {
            Ok(Self(value))
        } else {
            Err(PercentError { value })
        }
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Setting {
    brightness: Percent,

    /// 0%   = channel A only
    /// 100% = channel B only
    mix: Percent,
}

impl Setting {
    pub const OFF: Self = Self {
        brightness: Percent::ZERO,
        mix: Percent::ZERO,
    };

    pub const fn new(brightness: Percent, mix: Percent) -> Self {
        Self { brightness, mix }
    }

    pub const fn brightness(self) -> Percent {
        self.brightness
    }

    pub const fn mix(self) -> Percent {
        self.mix
    }
}

impl Default for Setting {
    fn default() -> Self {
        Self::OFF
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitError {
    ZeroFullScale,
    DutyRange {
        requested: u16,
        channel_a_max: u16,
        channel_b_max: u16,
    },
}

#[derive(Debug)]
pub enum Error<A, B> {
    ChannelA(A),
    ChannelB(B),
}

pub struct DualPwmFrontlight<A, B> {
    channel_a: A,
    channel_b: B,
    full_scale: u16,
    setting: Setting,
}

impl<A, B> DualPwmFrontlight<A, B>
where
    A: SetDutyCycle,
    B: SetDutyCycle,
{
    pub fn new(channel_a: A, channel_b: B, full_scale: u16) -> Result<Self, InitError> {
        if full_scale == 0 {
            return Err(InitError::ZeroFullScale);
        }

        let channel_a_max = channel_a.max_duty_cycle();
        let channel_b_max = channel_b.max_duty_cycle();
        if full_scale > channel_a_max || full_scale > channel_b_max {
            return Err(InitError::DutyRange {
                requested: full_scale,
                channel_a_max,
                channel_b_max,
            });
        }

        Ok(Self {
            channel_a,
            channel_b,
            full_scale,
            setting: Setting::OFF,
        })
    }

    pub const fn setting(&self) -> Setting {
        self.setting
    }

    pub const fn full_scale(&self) -> u16 {
        self.full_scale
    }

    pub fn set(&mut self, setting: Setting) -> Result<(), Error<A::Error, B::Error>> {
        let total = perceptual_duty(setting.brightness, self.full_scale);

        // split AFTER mapping brightness to the PWM domain
        // doing the split in integer percentage space first makes very low brightness
        // settings collapse to zero on both channels
        let channel_b = (total as u32 * setting.mix.get() as u32 + 50) / 100;
        let channel_b = channel_b as u16;
        let channel_a = total - channel_b;

        self.channel_a
            .set_duty_cycle(channel_a)
            .map_err(Error::ChannelA)?;
        self.channel_b
            .set_duty_cycle(channel_b)
            .map_err(Error::ChannelB)?;

        self.setting = setting;

        Ok(())
    }

    pub fn off(&mut self) -> Result<(), Error<A::Error, B::Error>> {
        let setting = Setting::new(Percent::ZERO, self.setting.mix);
        self.set(setting)
    }

    pub fn release(self) -> (A, B) {
        (self.channel_a, self.channel_b)
    }
}

/// perception-weighted brightness curve.
///
/// this is deliberately integer-only and doesn't require a floating-point impl or
/// lookup table.
///
/// `y = x(x + 10) / 11000`
///
/// it is darker than a linear curve while still mapping every 1% brightness step to
/// a distinct value on a 10-bit, 0..1023 PWM range
fn perceptual_duty(brightness: Percent, full_scale: u16) -> u16 {
    let brightness = brightness.get() as u32;
    if brightness == 0 {
        return 0;
    }

    let curve = brightness * (brightness + 10);
    let duty = (full_scale as u32 * curve + 5_500) / 11_000;

    duty.max(1).min(full_scale as u32) as u16
}
