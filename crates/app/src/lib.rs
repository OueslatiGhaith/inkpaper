#![no_std]

pub mod app;
pub mod clock;
pub mod components;
pub mod event;
pub mod model;
pub mod screens;
pub mod storage;
pub mod theme;

pub use app::{InkPaperApp, Route};

pub use event::{
    AppEvent, Button, ButtonEdge, ButtonEvent, ClockState, InputEvent, PlatformAction, ScrollEvent,
    TouchEvent, TouchPosition,
};

pub use model::{
    AppModel, BOOK_PATH_BYTES, BookSummary, LIBRARY_PAGE_CAPACITY, LibraryEntry, LibraryPage,
    ModelError, Percent,
};
