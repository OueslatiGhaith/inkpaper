#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryStatus {
    percent: u8,
    millivolts: u16,
}

impl BatteryStatus {
    pub const fn new(percent: u8, millivolts: u16) -> Option<Self> {
        if percent > 100 {
            return None;
        }

        Some(Self {
            percent,
            millivolts,
        })
    }

    pub const fn percent(self) -> u8 {
        self.percent
    }

    pub const fn millivolts(self) -> u16 {
        self.millivolts
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockStatus {
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
}

impl ClockStatus {
    pub const fn new(year: u16, month: u8, day: u8, hour: u8, minute: u8) -> Option<Self> {
        if month == 0 || month > 12 || day == 0 || day > 31 || hour > 23 || minute > 59 {
            return None;
        }

        Some(Self {
            year,
            month,
            day,
            hour,
            minute,
        })
    }

    pub const fn year(self) -> u16 {
        self.year
    }

    pub const fn month(self) -> u8 {
        self.month
    }

    pub const fn day(self) -> u8 {
        self.day
    }

    pub const fn hour(self) -> u8 {
        self.hour
    }

    pub const fn minute(self) -> u8 {
        self.minute
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SystemStatus {
    battery: Option<BatteryStatus>,
    clock: Option<ClockStatus>,
}

impl SystemStatus {
    pub(crate) const fn battery(self) -> Option<BatteryStatus> {
        self.battery
    }

    pub(crate) const fn clock(self) -> Option<ClockStatus> {
        self.clock
    }

    /// Returns whether the UI-visible battery percentage changed.
    pub(crate) fn set_battery(&mut self, battery: BatteryStatus) -> bool {
        let changed = self.battery.map(BatteryStatus::percent) != Some(battery.percent());

        self.battery = Some(battery);

        changed
    }

    pub(crate) fn set_clock(&mut self, clock: Option<ClockStatus>) -> bool {
        if self.clock == clock {
            return false;
        }

        self.clock = clock;

        true
    }
}
