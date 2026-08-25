use core::fmt::Write;

use heapless::String;

const MINUTES_PER_DAY: i16 = 24 * 60;
const MAX_OFFSET_MINUTES: i16 = 14 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ClockError {
    Hour { value: u8 },
    Minute { value: u8 },
    OffsetMinutes { value: i16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct TimeOfDay {
    hour: u8,
    minute: u8,
}

impl TimeOfDay {
    pub const fn new(hour: u8, minute: u8) -> Result<Self, ClockError> {
        if hour > 23 {
            return Err(ClockError::Hour { value: hour });
        }
        if minute > 59 {
            return Err(ClockError::Minute { value: minute });
        }

        Ok(Self { hour, minute })
    }

    pub const fn hour(self) -> u8 {
        self.hour
    }

    pub const fn minute(self) -> u8 {
        self.minute
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct UtcOffset {
    minutes: i16,
}

impl UtcOffset {
    pub const UTC: Self = Self { minutes: 0 };

    pub const fn new(minutes: i16) -> Result<Self, ClockError> {
        if minutes < -MAX_OFFSET_MINUTES || minutes > MAX_OFFSET_MINUTES {
            return Err(ClockError::OffsetMinutes { value: minutes });
        }

        Ok(Self { minutes })
    }

    pub const fn minutes(self) -> i16 {
        self.minutes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Clock {
    utc: Option<TimeOfDay>,
    offset: UtcOffset,
    label: String<5>,
}

impl Clock {
    pub fn new(offset: UtcOffset) -> Self {
        let mut clock = Self {
            utc: None,
            offset,
            label: String::new(),
        };

        clock.rebuild_label();

        clock
    }

    pub const fn utc_time(&self) -> Option<TimeOfDay> {
        self.utc
    }

    pub const fn offset(&self) -> UtcOffset {
        self.offset
    }

    pub const fn is_available(&self) -> bool {
        self.utc.is_some()
    }

    pub fn label(&self) -> &str {
        self.label.as_str()
    }

    pub fn local_time(&self) -> Option<TimeOfDay> {
        let utc = self.utc?;
        let utc_minutes = utc.hour() as i16 * 60 + utc.minute() as i16;
        let local_minutes = (utc_minutes + self.offset.minutes()).rem_euclid(MINUTES_PER_DAY);

        Some(TimeOfDay {
            hour: (local_minutes / 60) as u8,
            minute: (local_minutes % 60) as u8,
        })
    }

    /// updates the UTC wall-clock value.
    ///
    /// returns true when the displayed local-time label changed.
    pub fn set_utc_time(&mut self, hour: u8, minute: u8) -> Result<bool, ClockError> {
        self.utc = Some(TimeOfDay::new(hour, minute)?);
        Ok(self.rebuild_label())
    }

    /// changes the fixed UTC offset.
    ///
    /// returns true when the displayed local-time label changed.
    pub fn set_offset(&mut self, offset: UtcOffset) -> bool {
        if self.offset == offset {
            return false;
        }

        self.offset = offset;
        self.rebuild_label()
    }

    /// marks the clock unavailable.
    ///
    /// returns true when the displayed label changed.
    pub fn invalidate(&mut self) -> bool {
        if self.utc.is_none() {
            return false;
        }

        self.utc = None;
        self.rebuild_label()
    }

    fn rebuild_label(&mut self) -> bool {
        let mut label = String::<5>::new();

        match self.local_time() {
            Some(time) => write!(&mut label, "{:02}:{:02}", time.hour(), time.minute(),)
                .expect("HH:MM must fit in five bytes"),
            None => label
                .push_str("--:--")
                .expect("invalid clock label must fit"),
        }

        if self.label == label {
            return false;
        }

        self.label = label;

        true
    }
}
