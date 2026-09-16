#![no_std]

extern crate alloc;

mod app;
mod browser;
mod components;
mod reader;
mod reader_page;
mod screens;
mod typography;

pub use app::InkPaperApp;
pub use browser::{BrowseEntry, BrowseEntryKind, BrowseListing, BrowseRequest};
pub use reader::{ReaderDocument, ReaderLoadError, ReaderRequest, load_reader_document};
