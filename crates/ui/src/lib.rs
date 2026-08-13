#![no_std]

#[cfg(test)]
extern crate std;

#[cfg(feature = "embedded-graphics")]
pub mod backend;

mod arena;
mod callback;
mod callback_store;
mod canvas;
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
mod image;
mod input;
mod invalidation;
mod layout;
mod listener;
mod paint;
mod render;
mod runtime;
mod scroll;
mod stateful;
mod style;
mod text_layout;
mod text_style;
mod units;
mod visual;

pub use arena::*;
pub use canvas::*;
pub use color::*;
pub use context::*;
pub use div::*;
pub use element::*;
pub use entity::*;
pub use event::*;
pub use frame::*;
pub use geometry::*;
pub use identity::*;
pub use image::*;
pub use invalidation::*;
pub use layout::*;
pub use listener::*;
pub use paint::*;
pub use render::*;
pub use runtime::*;
pub use stateful::*;
pub use style::*;
pub use text_style::*;
pub use units::*;

pub mod prelude {
    // TODO: fill predlude
    pub use crate::{
        CanvasPainter, ActivateEvent, Color, Context, Entity, FontId, Image, ImageFit, ImageId,
        ImageSource, InteractiveElement, IntoElement, Invalidation, Listener, Offset, OnEvent,
        ParentElement, Pixels, Point, Rect, Render, Runtime, Size, StatefulInteractiveElementExt,
        Styled, TextAlign, TextStyled, TextWrap, canvas::canvas, div::div, image::image, px, text,
    };
}
