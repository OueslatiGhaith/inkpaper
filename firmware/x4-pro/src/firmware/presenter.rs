use defmt::Format;
use embedded_graphics::{
    geometry::Point as EgPoint,
    mono_font::ascii::{FONT_6X10, FONT_10X20},
    pixelcolor::Rgb888,
};
use inkpaper_app::InkPaperApp;
use inkpaper_ui::{
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
pub const UI_GLYPH_CACHE_BYTES: usize = 8 * 1024;
const UI_GLYPH_CACHE_SLOTS: usize = 64;

type UiFontResources<'storage> = FontResources<'static, 'storage, 2, UI_GLYPH_CACHE_SLOTS>;

pub type UiRuntime = Runtime<
    4_096, // entity bytes
    8,     // entity slots
    2_048, // callback bytes
    16,    // callback slots
    96,    // frame nodes
    2_048, // frame text bytes
    32,    // persistent element states
    256,   // global bytes
    4,     // global slots
>;

#[derive(Debug, Format, Clone, Copy, PartialEq, Eq)]
pub enum RefreshRequest {
    Full,
    Fast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameUpdate {
    refresh: RefreshRequest,
    physical_damage: Region,
}

impl FrameUpdate {
    pub const fn new(refresh: RefreshRequest, physical_damage: Region) -> Self {
        Self {
            refresh,
            physical_damage,
        }
    }

    pub const fn refresh(self) -> RefreshRequest {
        self.refresh
    }

    pub const fn physical_damage(self) -> Region {
        self.physical_damage
    }

    pub const fn is_full_damage(self) -> bool {
        self.physical_damage.x == 0
            && self.physical_damage.y == 0
            && self.physical_damage.width == PHYSICAL_WIDTH as u16
            && self.physical_damage.height == PHYSICAL_HEIGHT as u16
    }
}

pub struct Presenter<'storage> {
    fonts: UiFontResources<'storage>,
}

impl<'storage> Presenter<'storage> {
    pub fn new(glyph_storage: &'storage mut [u8]) -> Self {
        let mut fonts = FontResources::new(glyph_storage);
        let body = fonts.register(&BODY_FONT).expect("body font slot must fit");
        let heading = fonts
            .register(&HEADING_FONT)
            .expect("heading font slot must fit");

        defmt::assert_eq!(body, FontId::DEFAULT,);
        defmt::assert_eq!(heading, FontId::new(1,),);

        Self { fonts }
    }

    pub fn render_initial(
        &mut self,
        runtime: &mut UiRuntime,
        app: Entity<InkPaperApp>,
        frame: &mut [u8; FRAMEBUFFER_LEN],
    ) -> FrameUpdate {
        let invalidation = RenderInvalidation::full(Invalidation::Rebuild);

        let physical_damage =
            render_invalidation(runtime, app, frame, &mut self.fonts, invalidation)
                .expect("a full initial render must produce physical damage");

        FrameUpdate::new(RefreshRequest::Full, physical_damage)
    }

    pub fn render_pending(
        &mut self,
        runtime: &mut UiRuntime,
        app: Entity<InkPaperApp>,
        frame: &mut [u8; FRAMEBUFFER_LEN],
    ) -> Option<FrameUpdate> {
        let invalidation = runtime.take_render_invalidation();
        if invalidation.is_none() {
            return None;
        }

        let physical_damage =
            render_invalidation(runtime, app, frame, &mut self.fonts, invalidation)?;

        Some(FrameUpdate::new(RefreshRequest::Fast, physical_damage))
    }
}

fn render_invalidation(
    runtime: &mut UiRuntime,
    app: Entity<InkPaperApp>,
    frame: &mut [u8; FRAMEBUFFER_LEN],
    fonts: &mut UiFontResources<'_>,
    invalidation: RenderInvalidation,
) -> Option<Region> {
    if invalidation.is_none() {
        return None;
    }

    let damage = normalize_damage(invalidation.damage());
    if damage.is_none() {
        return None;
    }

    let mut display = Framebuffer::new(frame, Orientation::Portrait);
    {
        let mut painter = EmbeddedGraphicsPainter::new(&mut display, fonts, []).with_coverage_mode(
            CoverageMode::AlphaBlend {
                read_pixel: read_framebuffer_pixel,
            },
        );
        match invalidation.kind() {
            Invalidation::None => return None,
            Invalidation::Paint => {}
            Invalidation::Layout => {
                runtime
                    .layout(DISPLAY_SIZE, &painter)
                    .expect("layout requires a mounted root");
            }
            Invalidation::Rebuild => {
                runtime.rebuild(app).expect("UI rebuild capacity exceeded");
                runtime
                    .layout(DISPLAY_SIZE, &painter)
                    .expect("rebuilt UI must have a root");
            }
        }

        painter.clear_damage(damage, Color::WHITE).unwrap();
        runtime
            .paint_with_damage(damage, &mut painter)
            .unwrap()
            .expect("painting requires a mounted root");
    }

    physical_damage(&display, damage)
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
