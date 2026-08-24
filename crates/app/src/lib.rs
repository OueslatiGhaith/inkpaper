#![no_std]

pub mod app;
pub mod clock;
pub mod components;
pub mod model;
pub mod screens;
pub mod theme;

pub use app::{InkPaperApp, Route};
pub use model::{AppModel, BookSummary, ModelError, Percent};
