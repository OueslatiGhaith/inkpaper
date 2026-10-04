#![no_std]

#[cfg(test)]
extern crate std;

#[cfg(any(test, feature = "alloc", feature = "hinting"))]
extern crate alloc;

#[cfg(any(feature = "embedded-graphics", feature = "eink"))]
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
mod slot_table;
mod storage;
mod style;
mod svg_source;
mod text_layout;
mod text_shaping;
mod vector;

#[cfg(feature = "alloc")]
pub use callback::HeapCallbackArena;
pub use callback::{CallbackAllocError, FixedCallbackArena, Listener, ListenerInvokeError};
pub use color::*;
pub use context::*;
pub use element::*;
#[cfg(feature = "alloc")]
pub use entity::HeapEntityArena;
#[cfg(test)]
pub(crate) use entity::TestEntityArena;
pub use entity::{Entity, EntityAccessError, EntityAllocError, EntityId, FixedEntityArena};
pub use font::*;
pub use frame::*;
pub use geometry::*;
#[cfg(feature = "alloc")]
pub use global::HeapGlobalArena;
#[cfg(test)]
pub(crate) use global::TestGlobalArena;
pub use global::{
    FixedGlobalArena, Global, GlobalAccessError, GlobalMut, GlobalRef, GlobalSetError,
};
pub use image::*;
pub use interaction::*;
pub use layout::*;
pub use metrics::*;
pub use rendering::*;
pub use resources::*;
pub use runtime::*;
pub(crate) use slot_table::BorrowKind;
pub use storage::{FixedElementStates, FixedFrame, FixedStorage, RuntimeStorage};
#[cfg(feature = "alloc")]
pub use storage::{HeapElementStates, HeapFrame, HeapStorage};
#[cfg(test)]
pub(crate) use storage::{TestElementStates, TestFrame, TestStorage};
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
        Either, Element, ElementId, EmptySlot, Entity, EventTarget, FillRule, FixedStorage,
        FontFamilyId, FontId, FontResources, FontWeight, FrameBuildError, Global,
        GlobalAccessError, GlobalMut, GlobalRef, GlobalSetError, IdentifiableElementExt, Image,
        ImageColorMode, ImageDither, ImageFit, ImageId, ImagePaint, ImagePosition, ImageRegistry,
        ImageRegistryError, ImageResource, ImageSampling, ImageSource, IntoElement, Invalidation,
        LineHeight, Listener, Luminance, MountCx, MountError, NoChildren, NodeId, Offset, OnEvent,
        PaintCx, PaintReport, ParentElement, ParentElementChildrenExt, PathCommand, PathFill,
        PathStroke, Pixels, Point, Rect, Render, RenderInvalidation, RenderOnce, RenderRuntimeApi,
        ResourcePainter, ResourceRuntimeApi, Runtime, RuntimeApi, RuntimeStorage, Size,
        StatefulInteractiveElementExt, StrokeCap, StrokeJoin, Style, Styled, Svg, SvgFill,
        SvgPaint, SvgPath, SvgSource, SvgStroke, SvgViewBox, TextAlign, TextMaxLines, TextOverflow,
        TextStyled, TextWrap, VectorPath, VectorPoint, canvas, div, image, px, svg, text,
    };

    #[cfg(feature = "alloc")]
    pub use crate::{AnyElement, HeapStorage};

    #[cfg(feature = "macros")]
    pub use inkpaper_ui_macros::{component, include_svg, rsx};
}
