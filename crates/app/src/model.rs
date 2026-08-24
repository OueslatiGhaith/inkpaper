use core::fmt::Write;

use heapless::String;

use crate::clock::{Clock, UtcOffset};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelError {
    TextTooLong,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Percent {
    value: u8,
    label: String<4>,
}

impl Percent {
    pub fn new(value: u8) -> Self {
        let value = value.min(100);
        let mut label = String::new();

        write!(&mut label, "{value}%").expect("a clamped percentage must fit in four bytes");

        Self { value, label }
    }

    pub const fn value(&self) -> u8 {
        self.value
    }

    pub fn label(&self) -> &str {
        self.label.as_str()
    }

    pub fn set(&mut self, value: u8) {
        *self = Self::new(value);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookSummary {
    title: String<96>,
    author: String<64>,
    progress: Percent,
}

impl BookSummary {
    pub fn try_new(title: &str, author: &str, progress_percent: u8) -> Result<Self, ModelError> {
        let mut stored_title = String::new();
        stored_title
            .push_str(title)
            .map_err(|_| ModelError::TextTooLong)?;

        let mut stored_author = String::new();
        stored_author
            .push_str(author)
            .map_err(|_| ModelError::TextTooLong)?;

        Ok(Self {
            title: stored_title,
            author: stored_author,
            progress: Percent::new(progress_percent),
        })
    }

    pub fn title(&self) -> &str {
        self.title.as_str()
    }

    pub fn author(&self) -> &str {
        self.author.as_str()
    }

    pub const fn progress(&self) -> &Percent {
        &self.progress
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppModel {
    battery: Percent,
    clock: Clock,
    current_book: BookSummary,
}

impl AppModel {
    pub fn new(battery_percent: u8, current_book: BookSummary) -> Self {
        Self {
            battery: Percent::new(battery_percent),
            clock: Clock::new(UtcOffset::UTC),
            current_book,
        }
    }

    pub const fn battery(&self) -> &Percent {
        &self.battery
    }

    pub const fn clock(&self) -> &Clock {
        &self.clock
    }

    pub fn clock_mut(&mut self) -> &mut Clock {
        &mut self.clock
    }

    pub const fn current_book(&self) -> &BookSummary {
        &self.current_book
    }

    pub fn set_battery_percent(&mut self, percent: u8) {
        self.battery.set(percent);
    }

    pub fn set_current_book(&mut self, book: BookSummary) {
        self.current_book = book;
    }
}
