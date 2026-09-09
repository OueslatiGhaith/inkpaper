#![no_std]

#[cfg(test)]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "embedded-graphics")]
pub mod backend;

mod callback;
mod color;
mod context;
mod element;
mod entity;
mod font;
mod frame;
mod geometry;
mod global;
mod image;
mod interaction;
mod layout;
mod metrics;
mod rendering;
mod resources;
mod runtime;
mod style;
mod text_layout;
mod text_shaping;

pub use callback::{CallbackAllocError, Listener, ListenerInvokeError};
pub use color::*;
pub use context::*;
pub use element::*;
pub use entity::{Entity, EntityAccessError, EntityAllocError, EntityId};
pub(crate) use entity::{EntityArena, EntityBorrowKind, align_up};
pub use font::*;
pub use frame::*;
pub use geometry::*;
pub use global::*;
pub use image::*;
pub use interaction::*;
pub use layout::*;
pub use metrics::*;
pub use rendering::*;
pub use resources::*;
pub use runtime::*;
pub use style::*;
pub use text_shaping::*;

#[cfg(feature = "ttf")]
pub use ttf::*;

pub mod prelude {
    pub use crate::{
        ActivateEvent, AppContext, CanvasPainter, Color, ComponentSlot, ConditionalElementExt,
        Context, DamageRegion, Either, Element, EmptySlot, Entity, EventTarget, FontId,
        FontResources, FrameBuildError, Global, GlobalAccessError, GlobalMut, GlobalRef,
        GlobalSetError, IdentifiableElementExt, Image, ImageColorMode, ImageDither, ImageFit,
        ImageId, ImagePaint, ImagePosition, ImageRegistry, ImageRegistryError, ImageResource,
        ImageSampling, ImageSource, IntoElement, Invalidation, LineHeight, Listener, MountCx,
        MountError, NodeId, Offset, OnEvent, PaintReport, ParentElement, Pixels, Point, Rect,
        Render, RenderInvalidation, RenderOnce, RenderRuntimeApi, ResourcePainter, Runtime,
        RuntimeApi, RuntimeBuilder, Size, StatefulInteractiveElementExt, Style, Styled, TextAlign,
        TextMaxLines, TextOverflow, TextStyled, TextWrap, canvas, div, image, px, text,
    };
}
