use defmt::Format;
use embedded_graphics::{
    geometry::Point as EgPoint,
    mono_font::ascii::{FONT_6X10, FONT_10X20},
    pixelcolor::Rgb888,
};
use inkpaper_ui::{
    RuntimeResources,
    backend::{CoverageMode, EmbeddedGraphicsPainter, MonoFontFace},
    prelude::*,
};

use crate::firmware::framebuffer::{
    FRAMEBUFFER_LEN, Framebuffer, LOGICAL_HEIGHT, LOGICAL_WIDTH, Orientation, PHYSICAL_HEIGHT,
    PHYSICAL_WIDTH, Region,
};

const DISPLAY_SIZE: Size = Size::new(px(LOGICAL_WIDTH as i32), px(LOGICAL_HEIGHT as i32));
const DISPLAY_BOUNDS: Rect = Rect::new(Point::ZERO, DISPLAY_SIZE);

static BODY_FONT: MonoFontFace = MonoFontFace::ascii(&FONT_6X10);
static HEADING_FONT: MonoFontFace = MonoFontFace::ascii(&FONT_10X20);

const UI_ENTITY_BYTES: usize = 4_096;
const UI_ENTITY_SLOTS: usize = 8;

const UI_CALLBACK_BYTES: usize = 2_048;
const UI_CALLBACK_SLOTS: usize = 16;

const UI_FRAME_NODES: usize = 96;
const UI_FRAME_TEXT_BYTES: usize = 2_048;

const UI_ELEMENT_STATES: usize = 32;

const UI_GLOBAL_BYTES: usize = 256;
const UI_GLOBAL_SLOTS: usize = 4;

const UI_FONT_SLOTS: usize = 2;
const UI_GLYPH_CACHE_SLOTS: usize = 64;
const UI_GLYPH_CACHE_BYTES: usize = 8 * 1024;
const UI_IMAGE_SLOTS: usize = 0;

pub type UiRuntime = Runtime<
    UI_ENTITY_BYTES,
    UI_ENTITY_SLOTS,
    UI_CALLBACK_BYTES,
    UI_CALLBACK_SLOTS,
    UI_FRAME_NODES,
    UI_FRAME_TEXT_BYTES,
    UI_ELEMENT_STATES,
    UI_GLOBAL_BYTES,
    UI_GLOBAL_SLOTS,
    RuntimeResources<
        'static,
        UI_FONT_SLOTS,
        UI_GLYPH_CACHE_SLOTS,
        UI_GLYPH_CACHE_BYTES,
        UI_IMAGE_SLOTS,
    >,
>;

#[derive(Debug, Format, Clone, Copy, PartialEq, Eq)]
pub enum RefreshRequest {
    Full,
    Fast,
}

#[derive(Debug, Clone, Copy)]
struct RenderedFrame {
    physical_damage: Region,
    paint_report: PaintReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameUpdate {
    refresh: RefreshRequest,
    physical_damage: Region,
    paint_report: PaintReport,
}

impl FrameUpdate {
    pub const fn new(
        refresh: RefreshRequest,
        physical_damage: Region,
        paint_report: PaintReport,
    ) -> Self {
        Self {
            refresh,
            physical_damage,
            paint_report,
        }
    }

    pub const fn refresh(self) -> RefreshRequest {
        self.refresh
    }

    pub const fn physical_damage(self) -> Region {
        self.physical_damage
    }

    pub const fn paint_report(self) -> PaintReport {
        self.paint_report
    }

    pub const fn is_full_damage(self) -> bool {
        self.physical_damage.x == 0
            && self.physical_damage.y == 0
            && self.physical_damage.width == PHYSICAL_WIDTH as u16
            && self.physical_damage.height == PHYSICAL_HEIGHT as u16
    }
}

pub struct Presenter;

impl Presenter {
    pub fn new(runtime: &mut UiRuntime) -> Self {
        let body = runtime
            .register_font(&BODY_FONT)
            .expect("body font slot must fit");

        let heading = runtime
            .register_font(&HEADING_FONT)
            .expect("heading font slot must fit");

        defmt::assert_eq!(body, FontId::DEFAULT);
        defmt::assert_eq!(heading, FontId::new(1));

        Self
    }

    pub fn render_initial(
        &mut self,
        runtime: &mut UiRuntime,
        frame: &mut [u8; FRAMEBUFFER_LEN],
    ) -> FrameUpdate {
        let invalidation = RenderInvalidation::full(Invalidation::Rebuild);

        let rendered = render_invalidation(runtime, frame, invalidation)
            .expect("a full initial render must produce physical damage");

        FrameUpdate::new(
            RefreshRequest::Full,
            rendered.physical_damage,
            rendered.paint_report,
        )
    }

    pub fn render_pending(
        &mut self,
        runtime: &mut UiRuntime,
        frame: &mut [u8; FRAMEBUFFER_LEN],
    ) -> Option<FrameUpdate> {
        let invalidation = runtime.take_render_invalidation();
        if invalidation.is_none() {
            return None;
        }

        let rendered = render_invalidation(runtime, frame, invalidation)?;

        Some(FrameUpdate::new(
            RefreshRequest::Fast,
            rendered.physical_damage,
            rendered.paint_report,
        ))
    }
}

fn render_invalidation(
    runtime: &mut UiRuntime,
    frame: &mut [u8; FRAMEBUFFER_LEN],
    invalidation: RenderInvalidation,
) -> Option<RenderedFrame> {
    if invalidation.is_none() {
        return None;
    }

    let damage = normalize_damage(invalidation.damage());
    if damage.is_none() {
        return None;
    }

    let mut display = Framebuffer::new(frame, Orientation::Portrait);

    let paint_report = {
        let mut painter = EmbeddedGraphicsPainter::new(&mut display).with_coverage_mode(
            CoverageMode::AlphaBlend {
                read_pixel: read_framebuffer_pixel,
            },
        );

        match invalidation.kind() {
            Invalidation::None => return None,
            Invalidation::Paint => {}
            Invalidation::Layout => {
                runtime
                    .layout(DISPLAY_SIZE)
                    .expect("layout requires a mounted root");
            }
            Invalidation::Rebuild => {
                runtime.rebuild().expect("UI rebuild capacity exceeded");
                runtime
                    .layout(DISPLAY_SIZE)
                    .expect("rebuilt UI must have a root");
            }
        }

        painter.clear_damage(damage, Color::WHITE).unwrap();
        runtime
            .paint_with_damage(damage, &mut painter)
            .unwrap()
            .expect("painting requires a mounted root")
    };

    let physical_damage = physical_damage(&display, damage)?;

    Some(RenderedFrame {
        physical_damage,
        paint_report,
    })
}

fn normalize_damage(damage: DamageRegion) -> DamageRegion {
    if damage.is_none() {
        return DamageRegion::none();
    }
    if damage.is_full() {
        return DamageRegion::full();
    }

    damage.clipped_to(DISPLAY_BOUNDS)
}

fn physical_damage(framebuffer: &Framebuffer<'_>, damage: DamageRegion) -> Option<Region> {
    if damage.is_none() {
        return None;
    }
    if damage.is_full() {
        return Some(Region::new(
            0,
            0,
            PHYSICAL_WIDTH as u16,
            PHYSICAL_HEIGHT as u16,
        ));
    }

    let bounds = damage.partial_damage_bounds();
    let logical = ui_rect_to_region(bounds)?;

    framebuffer.physical_damage_region(logical)
}

fn ui_rect_to_region(rect: Rect) -> Option<Region> {
    let x = rect.x().get();
    let y = rect.y().get();
    let width = rect.width().get();
    let height = rect.height().get();
    if x < 0 || y < 0 || width <= 0 || height <= 0 {
        return None;
    }

    Some(Region::new(
        u16::try_from(x).ok()?,
        u16::try_from(y).ok()?,
        u16::try_from(width).ok()?,
        u16::try_from(height).ok()?,
    ))
}

fn read_framebuffer_pixel(framebuffer: &Framebuffer<'_>, point: EgPoint) -> Option<Rgb888> {
    framebuffer.get_pixel(point)
}
