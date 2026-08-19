#![no_std]

mod app;
mod model;

pub mod components;
pub mod screens;
pub mod theme;

pub use app::{InkPaperApp, Route};
pub use model::{AppModel, BookSummary, ModelError, Percent};
