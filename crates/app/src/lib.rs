#![no_std]

extern crate alloc;

mod app;
mod browser;
mod components;
mod reader;
mod screens;
mod typography;

pub use app::InkPaperApp;
pub use browser::{BrowseEntry, BrowseEntryKind, BrowseListing, BrowseRequest};
