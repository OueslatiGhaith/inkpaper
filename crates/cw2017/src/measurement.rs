use crate::register::{SocRegister, VcellRegister};

/// `CW2017` silicon version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version(u8);

impl Version {
    pub(crate) const fn from_raw(raw: u8) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u8 {
        self.0
    }
}

/// battery state of charge
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateOfCharge {
    whole_percent: u8,
    fraction_256ths: u8,
}

impl StateOfCharge {
    pub(crate) fn from_register(register: SocRegister) -> Self {
        Self {
            whole_percent: register.whole_percent(),
            fraction_256ths: register.fraction_256ths(),
        }
    }

    pub const fn whole_percent(self) -> u8 {
        self.whole_percent
    }

    pub const fn fraction_256ths(self) -> u8 {
        self.fraction_256ths
    }

    /// whether the integer portion represents a physically meaningful SoC.
    ///
    /// values above 100 can appear while the gauge has not yet converged
    /// after initialization
    pub const fn is_valid(self) -> bool {
        self.whole_percent <= 100
    }

    /// state of charge in thousandths of a percent
    ///
    /// for example: 73.5% becomes ~ `73_500`
    pub const fn milli_percent(self) -> u32 {
        self.whole_percent as u32 * 1000 + self.fraction_256ths as u32 * 1000 / 256
    }
}

/// battery terminal voltage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Voltage {
    counts: u16,
}

impl Voltage {
    pub(crate) fn from_register(register: VcellRegister) -> Self {
        Self {
            counts: register.counts(),
        }
    }

    /// raw 14-bit ADC result.
    pub const fn raw_counts(self) -> u16 {
        self.counts
    }

    /// voltage rounded to the nearest millivolt.
    ///
    /// one `CW2017` ADC count represents `312.5 µV`.
    pub const fn millivolts(self) -> u16 {
        ((self.counts as u32 * 5 + 8) >> 4) as u16
    }

    /// voltage in microvolts, rounded down to the nearest whole microvolt.
    pub const fn microvolts(self) -> u32 {
        self.counts as u32 * 625 / 2
    }
}

/// battery temperature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Temperature {
    half_degrees_celsius: i16,
}

impl Temperature {
    pub(crate) const fn from_raw(raw: u8) -> Self {
        Self {
            half_degrees_celsius: raw as i16 - 80,
        }
    }

    /// temperature in units of `0.5 °C`.
    ///
    /// for example, 47 means `23.5 °C` and -10 means `-5 °C`.
    pub const fn half_degrees_celsius(self) -> i16 {
        self.half_degrees_celsius
    }

    /// temperature multiplied by ten.
    ///
    /// this avoids requiring floating-point arithmetic in the firmware:
    /// `23.5 °C` is returned as 235.
    pub const fn celsius_x10(self) -> i16 {
        self.half_degrees_celsius * 5
    }
}
