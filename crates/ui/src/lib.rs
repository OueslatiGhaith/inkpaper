#![no_std]

#[cfg(test)]
extern crate std;

mod arena;
mod color;
mod context;
mod div;
mod element;
mod element_state;
mod entity;
mod entity_store;
mod event;
mod frame;
mod geometry;
mod identity;
mod layout;
mod listener;
mod listener_store;
mod paint;
mod render;
mod runtime;
mod stateful;
mod style;
mod units;

pub use arena::*;
pub use color::*;
pub use context::*;
pub use div::*;
pub use element::*;
pub use entity::*;
pub use event::*;
pub use frame::*;
pub use geometry::*;
pub use identity::*;
pub use layout::*;
pub use listener::*;
pub use paint::*;
pub use render::*;
pub use runtime::*;
pub use stateful::*;
pub use style::*;
pub use units::*;

pub mod prelude {
    // TODO: fill predlude
    pub use crate::{
        Color, Context, Entity, IntoElement, ParentElement, Render, Runtime, Size, Styled, div, px,
    };
}
