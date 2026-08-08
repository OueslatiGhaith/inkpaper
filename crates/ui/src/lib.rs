#![no_std]

#[cfg(test)]
extern crate std;

mod arena;
mod context;
mod div;
mod element;
mod element_state;
mod entity;
mod entity_store;
mod event;
mod frame;
mod identity;
mod listener;
mod listener_store;
mod render;
mod stateful;
mod style;
mod units;

pub use arena::*;
pub use context::*;
pub use div::*;
pub use element::*;
pub use entity::*;
pub use event::*;
pub use frame::*;
pub use identity::*;
pub use listener::*;
pub use render::*;
pub use stateful::*;
pub use style::*;
pub use units::*;

pub mod prelude {
    // TODO: fill predlude
    pub use crate::{Context, Entity, IntoElement, ParentElement, Render, Styled, div, px};
}
