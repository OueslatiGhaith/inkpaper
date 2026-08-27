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
mod component;
mod conditional;
mod context;
mod div;
mod element;
mod element_state;
mod entity;
mod entity_store;
mod event;
mod font;
mod frame;
mod geometry;
mod global;
mod identity;
mod image;
mod input;
mod invalidation;
mod layout;
mod listener;
mod metrics;
mod paint;
mod presentation;
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
pub use component::*;
pub use conditional::*;
pub use context::*;
pub use div::*;
pub use element::*;
pub use entity::*;
pub use event::*;
pub use font::*;
pub use frame::*;
pub use geometry::*;
pub use global::*;
pub use identity::*;
pub use image::*;
pub use invalidation::*;
pub use layout::*;
pub use listener::*;
pub use metrics::*;
pub use paint::*;
pub use presentation::*;
pub use render::*;
pub use runtime::*;
pub use stateful::*;
pub use style::*;
pub use text_style::*;
pub use units::*;

pub mod prelude {
    // TODO: fill predlude
    pub use crate::{
        ActivateEvent, AppContext, CanvasPainter, Color, ComponentSlot, ConditionalElementExt,
        Context, DamageRegion, Either, EmptySlot, Entity, EventTarget, FontId, Global,
        GlobalAccessError, GlobalMut, GlobalRef, GlobalSetError, IdentifiableElementExt, Image,
        ImageFit, ImageId, ImageSource, IntoElement, Invalidation, Listener, Offset, OnEvent,
        ParentElement, Pixels, Point, Rect, Render, RenderOnce, Runtime, Size,
        StatefulInteractiveElementExt, Style, Styled, TextAlign, TextStyled, TextWrap,
        canvas::canvas, div::div, image::image, px, text,
    };
}
