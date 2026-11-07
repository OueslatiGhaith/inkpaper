#![no_std]
#![cfg_attr(
    test,
    allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap
    )
)]

#[cfg(test)]
extern crate std;

#[cfg(any(test, feature = "alloc", feature = "hinting"))]
extern crate alloc;

#[cfg(feature = "embedded-graphics")]
pub mod backend;

mod callback;
mod color;
mod context;
mod dither;
mod element;
mod emitter;
mod entity;
mod float;
mod font;
mod frame;
mod geometry;
mod global;
mod image;
mod interaction;
mod layout;
mod metrics;
mod pool;
mod rendering;
mod resources;
mod runtime;
mod slot_table;
mod storage;
mod style;
mod svg_source;
mod text_layout;
mod text_shaping;
mod vector;

#[cfg(test)]
pub(crate) use callback::CallbackArena;
pub use callback::{CallbackAllocError, Listener, ListenerInvokeError};
pub use color::*;
pub use context::*;
pub use dither::*;
pub use element::*;
pub use emitter::{EventEmitter, EventError};
#[cfg(test)]
pub(crate) use entity::EntityArena;
pub use entity::{Entity, EntityAccessError, EntityAllocError, EntityId};
pub use font::*;
pub use frame::*;
pub use geometry::*;
#[cfg(test)]
pub(crate) use global::GlobalArena;
pub use global::{Global, GlobalAccessError, GlobalMut, GlobalRef, GlobalSetError};
pub use image::*;
pub use interaction::*;
pub use layout::*;
pub use metrics::*;
pub use pool::{MemoryStats, Pool, Table};
pub use rendering::*;
pub use resources::*;
pub use runtime::*;
pub(crate) use slot_table::BorrowKind;
pub use style::*;
pub use svg_source::*;
pub use text_shaping::*;
pub use vector::*;

#[cfg(feature = "ttf")]
pub use ttf::*;

pub mod prelude {
    pub use crate::{
        ActivateEvent, AffineTransform, AppContext, Canvas, CanvasPainter, Children, Color,
        ComponentChildren, ComponentSlot, ConditionalElementExt, Context, DamageRegion, Div,
        Either, Element, ElementId, EmptySlot, Entity, EventEmitter, EventTarget, FillRule,
        FontFamilyId, FontId, FontResources, FontStyle, FontWeight, FrameBuildError, FramePhase,
        Global, GlobalAccessError, GlobalMut, GlobalRef, GlobalSetError, IdentifiableElementExt,
        Image, ImageColorMode, ImageDither, ImageFit, ImageId, ImagePaint, ImagePosition,
        ImageRegistry, ImageRegistryError, ImageResource, ImageSampling, ImageSource, IntoElement,
        Invalidation, LineHeight, Listener, Luminance, MountCx, MountError, NoChildren, NodeId,
        Offset, OnEvent, PaintCx, PaintReport, PaintResources, ParentElement,
        ParentElementChildrenExt, PathCommand, PathFill, PathStroke, Pixels, Point, Pool, Rect,
        Render, RenderInvalidation, RenderOnce, RenderRuntimeApi, ResourceRuntimeApi, Runtime,
        RuntimeApi, Size, StatefulInteractiveElementExt, StrokeCap, StrokeJoin, Style, Styled, Svg,
        SvgFill, SvgPaint, SvgPath, SvgSource, SvgStroke, SvgViewBox, TextAlign, TextDarkness,
        TextMaxLines, TextOverflow, TextStyled, TextWrap, VectorPath, VectorPoint, canvas, div,
        image, px, svg, text,
    };

    #[cfg(feature = "alloc")]
    pub use crate::AnyElement;

    #[cfg(feature = "macros")]
    pub use inkpaper_ui_macros::{component, include_svg, rsx};
}
