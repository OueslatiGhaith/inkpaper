#![no_std]

use embedded_hal::pwm::SetDutyCycle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PercentError {
    value: u8,
}

impl PercentError {
    pub const fn value(self) -> u8 {
        self.value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Setting {
    brightness: Percent,
    /// 0%   = fully cool
    /// 100% = fully warm
    warmth: Percent,
}

impl Setting {
    pub const OFF: Self = Self {
        brightness: Percent::ZERO,
        warmth: Percent::ZERO,
    };

    pub const fn new(brightness: Percent, warmth: Percent) -> Self {
        Self { brightness, warmth }
    }

    pub const fn brightness(self) -> Percent {
        self.brightness
    }

    pub const fn warmth(self) -> Percent {
        self.warmth
    }
}

impl Default for Setting {
    fn default() -> Self {
        Self::OFF
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum InitError {
    ZeroFullScale,
    DutyRange {
        requested: u16,
        cool_max: u16,
        warm_max: u16,
    },
}

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error<A, B> {
    Cool(A),
    Warm(B),
}

pub struct DualPwmFrontlight<COOL, WARM> {
    cool: COOL,
    warm: WARM,
    full_scale: u16,
    setting: Setting,
}

impl<COOL, WARM> DualPwmFrontlight<COOL, WARM>
where
    COOL: SetDutyCycle,
    WARM: SetDutyCycle,
{
    pub fn new(cool: COOL, warm: WARM, full_scale: u16) -> Result<Self, InitError> {
        if full_scale == 0 {
            return Err(InitError::ZeroFullScale);
        }

        let cool_max = cool.max_duty_cycle();
        let warm_max = warm.max_duty_cycle();
        if full_scale > cool_max || full_scale > warm_max {
            return Err(InitError::DutyRange {
                requested: full_scale,
                cool_max,
                warm_max,
            });
        }

        Ok(Self {
            cool,
            warm,
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

    pub fn set(&mut self, setting: Setting) -> Result<(), Error<COOL::Error, WARM::Error>> {
        let total = perceptual_duty(setting.brightness, self.full_scale);

        // split AFTER mapping brightness to the PWM domain
        // doing the split in integer percentage space first makes very low brightness
        // settings collapse to zero on both channels
        let warm_duty = (total as u32 * setting.warmth.get() as u32 + 50) / 100;
        let warm_duty = warm_duty as u16;
        let cool_duty = total - warm_duty;

        self.cool.set_duty_cycle(cool_duty).map_err(Error::Cool)?;
        self.warm.set_duty_cycle(warm_duty).map_err(Error::Warm)?;

        self.setting = setting;

        Ok(())
    }

    pub fn off(&mut self) -> Result<(), Error<COOL::Error, WARM::Error>> {
        let setting = Setting::new(Percent::ZERO, self.setting.warmth);
        self.set(setting)
    }

    pub fn release(self) -> (COOL, WARM) {
        (self.cool, self.warm)
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
