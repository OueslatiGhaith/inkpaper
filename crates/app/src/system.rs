#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryStatus {
    percent: u8,
    millivolts: u16,
    charging: bool,
}

impl BatteryStatus {
    pub const fn new(percent: u8, millivolts: u16, charging: bool) -> Option<Self> {
        if percent > 100 {
            return None;
        }

        Some(Self {
            percent,
            millivolts,
            charging,
        })
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

    /// The same instant `minutes` away, carrying into the day, month and year.
    /// Offsets stay within a day, so at most one day carries.
    pub(crate) fn offset_by(self, minutes: i32) -> Self {
        let total = i32::from(self.hour) * 60 + i32::from(self.minute) + minutes;
        let minute_of_day = total.rem_euclid(24 * 60);

        let (mut year, mut month, mut day) = (self.year, self.month, self.day);

        match total.div_euclid(24 * 60) {
            1.. if day == days_in_month(year, month) => {
                day = 1;
                month = month % 12 + 1;
                if month == 1 {
                    year += 1;
                }
            }
            1.. => day += 1,
            ..0 if day == 1 => {
                month = if month == 1 { 12 } else { month - 1 };
                if month == 12 {
                    year -= 1;
                }
                day = days_in_month(year, month);
            }
            ..0 => day -= 1,
            0 => {}
        }

        Self {
            year,
            month,
            day,
            hour: (minute_of_day / 60) as u8,
            minute: (minute_of_day % 60) as u8,
        }
    }
}

const fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        _ => 31,
    }
}

/// the battery as last reported. Renders that show it read this entity, so they render
/// again when its visible percentage or charging state changes
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BatteryState {
    battery: Option<BatteryStatus>,
}

impl BatteryState {
    pub(crate) const fn get(self) -> Option<BatteryStatus> {
        self.battery
    }

    /// Returns whether the UI-visible battery percentage or charging state changed.
    pub(crate) fn set(&mut self, battery: BatteryStatus) -> bool {
        let visible = |battery: BatteryStatus| (battery.percent(), battery.charging());
        let changed = self.battery.map(visible) != Some(visible(battery));

        self.battery = Some(battery);

        changed
    }
}

/// the RTC's UTC time as last reported. Renders that show it read this entity, so they
/// render again when it changes
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RtcState {
    clock: Option<ClockStatus>,
}

impl RtcState {
    pub(crate) const fn get(self) -> Option<ClockStatus> {
        self.clock
    }

    /// Returns whether the time changed.
    pub(crate) fn set(&mut self, clock: Option<ClockStatus>) -> bool {
        if self.clock == clock {
            return false;
        }

        self.clock = clock;

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock(year: u16, month: u8, day: u8, hour: u8, minute: u8) -> ClockStatus {
        ClockStatus::new(year, month, day, hour, minute).unwrap()
    }

    #[test]
    fn offset_carries_into_the_next_day_month_and_year() {
        assert_eq!(
            clock(2026, 9, 27, 22, 50).offset_by(90),
            clock(2026, 9, 28, 0, 20)
        );
        assert_eq!(
            clock(2028, 2, 28, 23, 0).offset_by(60),
            clock(2028, 2, 29, 0, 0)
        );
        assert_eq!(
            clock(2026, 12, 31, 20, 0).offset_by(14 * 60),
            clock(2027, 1, 1, 10, 0)
        );
    }

    #[test]
    fn offset_carries_into_the_previous_day_month_and_year() {
        assert_eq!(
            clock(2026, 9, 27, 1, 0).offset_by(-210),
            clock(2026, 9, 26, 21, 30)
        );
        assert_eq!(
            clock(2026, 3, 1, 2, 0).offset_by(-3 * 60),
            clock(2026, 2, 28, 23, 0)
        );
        assert_eq!(
            clock(2027, 1, 1, 5, 0).offset_by(-12 * 60),
            clock(2026, 12, 31, 17, 0)
        );
    }
}
