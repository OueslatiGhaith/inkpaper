use inkpaper_ui::{
    RuntimeResources,
    backend::{EInkPaintReport, EInkPainter, EInkTone, EInkUiMode},
    prelude::*,
};

use crate::firmware::{
    framebuffer::{
        Framebuffer, FramebufferStorage, LOGICAL_HEIGHT, LOGICAL_WIDTH, Orientation,
        PHYSICAL_HEIGHT, PHYSICAL_WIDTH, Region,
    },
    refresh_policy::{EInkCapabilities, RefreshContext, RefreshPolicy, RefreshRequest},
};

const DISPLAY_SIZE: Size = Size::new(px(LOGICAL_WIDTH as i32), px(LOGICAL_HEIGHT as i32));
const DISPLAY_BOUNDS: Rect = Rect::new(Point::ZERO, DISPLAY_SIZE);

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

#[derive(Debug, Clone, Copy)]
struct RenderedFrame {
    physical_damage: Region,
    eink_report: EInkPaintReport,
    paint_report: PaintReport,
}

impl RenderedFrame {
    const fn is_full_damage(self) -> bool {
        self.physical_damage.x == 0
            && self.physical_damage.y == 0
            && self.physical_damage.width == PHYSICAL_WIDTH as u16
            && self.physical_damage.height == PHYSICAL_HEIGHT as u16
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameUpdate {
    refresh: RefreshRequest,
    physical_damage: Region,
    eink_report: EInkPaintReport,
    presentation_tone: EInkTone,
    paint_report: PaintReport,
}

impl FrameUpdate {
    pub const fn new(
        refresh: RefreshRequest,
        physical_damage: Region,
        eink_report: EInkPaintReport,
        presentation_tone: EInkTone,
        paint_report: PaintReport,
    ) -> Self {
        Self {
            refresh,
            physical_damage,
            eink_report,
            presentation_tone,
            paint_report,
        }
    }

    pub const fn refresh(self) -> RefreshRequest {
        self.refresh
    }

    pub const fn physical_damage(self) -> Region {
        self.physical_damage
    }

    pub const fn eink_report(self) -> EInkPaintReport {
        self.eink_report
    }

    pub const fn presentation_tone(self) -> EInkTone {
        self.presentation_tone
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

pub struct Presenter {
    refresh_policy: RefreshPolicy,
    panel_tone: EInkTone,
}

impl Default for Presenter {
    fn default() -> Self {
        Self {
            refresh_policy: RefreshPolicy::default(),
            panel_tone: EInkTone::Binary,
        }
    }
}

impl Presenter {
    pub fn render_initial(
        &mut self,
        runtime: &mut UiRuntime,
        frame: &mut FramebufferStorage,
    ) -> FrameUpdate {
        let invalidation = RenderInvalidation::full(Invalidation::Rebuild);

        let rendered = render_invalidation(runtime, frame, invalidation)
            .expect("a full initial render must produce physical damage");

        // initial presentation is explicitly full regardless of policy.
        // reset history so subsequent updates begin from a clean panel
        self.refresh_policy.record_full_refresh();

        let tone = rendered.eink_report.tone();
        self.panel_tone = tone;

        FrameUpdate::new(
            RefreshRequest::Full,
            rendered.physical_damage,
            rendered.eink_report,
            tone,
            rendered.paint_report,
        )
    }

    pub fn render_pending(
        &mut self,
        runtime: &mut UiRuntime,
        frame: &mut FramebufferStorage,
        capabilities: EInkCapabilities,
    ) -> Option<FrameUpdate> {
        let invalidation = runtime.take_render_invalidation();
        if invalidation.is_none() {
            return None;
        }

        let rendered = render_invalidation(runtime, frame, invalidation)?;

        let presentation_tone = self.presentation_tone(rendered.eink_report.tone(), capabilities);

        let refresh =
            if presentation_tone == EInkTone::Gray4 && !capabilities.supports_partial_grayscale() {
                self.refresh_policy.record_full_refresh();
                RefreshRequest::Full
            } else {
                self.refresh_policy.select(refresh_context(rendered))
            };

        self.record_presented_frame(rendered, presentation_tone);

        Some(FrameUpdate::new(
            refresh,
            rendered.physical_damage,
            rendered.eink_report,
            presentation_tone,
            rendered.paint_report,
        ))
    }

    fn presentation_tone(&self, damage_tone: EInkTone, capabilities: EInkCapabilities) -> EInkTone {
        if damage_tone == EInkTone::Gray4 {
            return EInkTone::Gray4;
        }

        if self.panel_tone == EInkTone::Gray4 && !capabilities.can_binary_update_over_grayscale() {
            return EInkTone::Gray4;
        }

        EInkTone::Binary
    }

    fn record_presented_frame(&mut self, rendered: RenderedFrame, presentation_tone: EInkTone) {
        if rendered.is_full_damage() {
            // the entire framebuffer now descrines the panel, regardless of which waveform
            // was required to get there
            self.panel_tone = rendered.eink_report.tone();
            return;
        }

        if rendered.eink_report.tone() == EInkTone::Gray4 {
            self.panel_tone = EInkTone::Gray4;
            return;
        }

        if self.panel_tone == EInkTone::Binary {
            self.panel_tone = EInkTone::Binary;
            return;
        }

        // a binary partial update can remove some existing gray, but without tracking
        // tone spatially we cannot prove it removed the last gray pixel. Stay conservative.
        // `presentation_tone` is intentionally consumed here: either a binary window
        // preserved gray elsewhere, or a grayscale presentation preserved/replayed it.
        let _ = presentation_tone;
        self.panel_tone = EInkTone::Gray4;
    }
}

fn render_invalidation(
    runtime: &mut UiRuntime,
    frame: &mut FramebufferStorage,
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

    let (paint_report, eink_report) = {
        let mut painter = EInkPainter::new(&mut display).with_ui_mode(EInkUiMode::BinaryDither);

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

        let paint_report = runtime
            .paint_with_damage(damage, &mut painter)
            .unwrap()
            .expect("painting requires a mounted root");

        let eink_report = painter.report();

        (paint_report, eink_report)
    };

    let physical_damage = physical_damage(&display, damage)?;

    // framebuffer owns a mutable borrow of `frame`.
    // release it before querying storage directly.
    drop(display);

    Some(RenderedFrame {
        physical_damage,
        eink_report,
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

fn refresh_context(rendered: RenderedFrame) -> RefreshContext {
    let damage = rendered.physical_damage;

    let damaged_pixels = u32::from(damage.width).saturating_mul(u32::from(damage.height));

    RefreshContext::new(
        rendered.paint_report.damage().is_full(),
        rendered.paint_report.content().has_continuous_tone_images(),
        damaged_pixels,
        physical_display_pixels(),
    )
}

const fn physical_display_pixels() -> u32 {
    (PHYSICAL_WIDTH as u32) * (PHYSICAL_HEIGHT as u32)
}
