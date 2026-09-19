#[cfg(feature = "trace")]
use inkpaper_trace::TraceSession;
use inkpaper_trace::{TraceEvent, profile_expr, profile_span};
use inkpaper_ui::{
    RuntimeResources,
    backend::{EInkPaintReport, EInkPainter, EInkTone, EInkUiMode},
    prelude::*,
};

#[cfg(feature = "performance")]
use crate::firmware::perf::{CycleTimer, FramePerfReport, RenderTimings};
use crate::firmware::{
    framebuffer::{
        Framebuffer, FramebufferStorage, LOGICAL_HEIGHT, LOGICAL_WIDTH, Orientation,
        PHYSICAL_HEIGHT, PHYSICAL_WIDTH, Region,
    },
    refresh_policy::{
        BinaryOverGrayMode, EInkCapabilities, RefreshContent, RefreshContext, RefreshPolicy,
        RefreshRequest,
    },
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
const UI_GLYPH_CACHE_SLOTS: usize = 128;
const UI_GLYPH_CACHE_BYTES: usize = 16 * 1024;

const UI_IMAGE_SLOTS: usize = 32;

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
    frame_id: u32,
    physical_damage: Region,
    eink_report: EInkPaintReport,
    paint_report: PaintReport,

    #[cfg(feature = "performance")]
    perf_report: FramePerfReport,
    #[cfg(feature = "ui-metrics")]
    framebuffer_draw_iter_pixels: u64,
}

impl RenderedFrame {
    const fn is_full_damage(self) -> bool {
        self.physical_damage.x == 0
            && self.physical_damage.y == 0
            && self.physical_damage.width == PHYSICAL_WIDTH as u16
            && self.physical_damage.height == PHYSICAL_HEIGHT as u16
    }
}

#[derive(Debug, defmt::Format, Clone, Copy, PartialEq, Eq)]
pub enum PresentationMode {
    Binary,
    BinaryPreservingGray,
    Gray4,
}

impl PresentationMode {
    pub const fn tone(self) -> EInkTone {
        match self {
            Self::Binary | Self::BinaryPreservingGray => EInkTone::Binary,
            Self::Gray4 => EInkTone::Gray4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameUpdate {
    frame_id: u32,
    refresh: RefreshRequest,
    physical_damage: Region,
    eink_report: EInkPaintReport,
    presentation: PresentationMode,
    paint_report: PaintReport,

    #[cfg(feature = "performance")]
    perf_report: FramePerfReport,
}

impl FrameUpdate {
    pub const fn new(
        frame_id: u32,
        refresh: RefreshRequest,
        physical_damage: Region,
        eink_report: EInkPaintReport,
        presentation: PresentationMode,
        paint_report: PaintReport,
    ) -> Self {
        Self {
            frame_id,
            refresh,
            physical_damage,
            eink_report,
            presentation,
            paint_report,

            #[cfg(feature = "performance")]
            perf_report: FramePerfReport::EMPTY,
        }
    }

    pub const fn frame_id(self) -> u32 {
        self.frame_id
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

    pub const fn presentation(self) -> PresentationMode {
        self.presentation
    }

    pub const fn presentation_tone(self) -> EInkTone {
        self.presentation.tone()
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

    #[cfg(feature = "performance")]
    pub(crate) const fn with_perf_report(mut self, report: FramePerfReport) -> Self {
        self.perf_report = report;
        self
    }

    #[cfg(feature = "performance")]
    pub(crate) const fn perf_report(self) -> FramePerfReport {
        self.perf_report
    }
}

pub struct Presenter {
    refresh_policy: RefreshPolicy,
    panel_tone: EInkTone,
    next_frame_id: u32,
}

impl Default for Presenter {
    fn default() -> Self {
        Self {
            refresh_policy: RefreshPolicy::default(),
            panel_tone: EInkTone::Binary,
            next_frame_id: 1,
        }
    }
}

impl Presenter {
    fn allocate_frame_id(&mut self) -> u32 {
        let frame_id = self.next_frame_id;
        self.next_frame_id = self.next_frame_id.wrapping_add(1);

        frame_id
    }

    pub fn render_initial(
        &mut self,
        runtime: &mut UiRuntime,
        frame: &mut FramebufferStorage,
    ) -> FrameUpdate {
        #[cfg(feature = "ui-metrics")]
        {
            runtime.reset_performance_metrics();
            runtime.reset_glyph_cache_metrics();
        }

        let invalidation = RenderInvalidation::full(Invalidation::Rebuild);

        let frame_id = self.allocate_frame_id();

        let rendered = render_invalidation(frame_id, runtime, frame, invalidation)
            .expect("a full initial render must produce physical damage");

        #[cfg(feature = "ui-metrics")]
        {
            crate::firmware::perf::log_ui_metrics(rendered.frame_id, runtime.performance_metrics());
            crate::firmware::perf::log_text_metrics(
                rendered.frame_id,
                rendered.eink_report,
                runtime.glyph_cache_metrics(),
                runtime.glyph_cache_used_bytes(),
                runtime.glyph_cache_capacity_bytes(),
            );
            crate::firmware::perf::log_coverage_metrics(
                rendered.frame_id,
                rendered.eink_report,
                rendered.framebuffer_draw_iter_pixels,
            );
        }

        // initial presentation is explicitly full regardless of policy.
        // reset history so subsequent updates begin from a clean panel
        self.refresh_policy.record_full_refresh();

        let tone = rendered.eink_report.tone();
        self.panel_tone = tone;

        let presentation = match tone {
            EInkTone::Binary => PresentationMode::Binary,
            EInkTone::Gray4 => PresentationMode::Gray4,
        };

        make_frame_update(rendered, RefreshRequest::Full, presentation)
    }

    pub fn render_pending(
        &mut self,
        runtime: &mut UiRuntime,
        frame: &mut FramebufferStorage,
        capabilities: EInkCapabilities,
    ) -> Option<FrameUpdate> {
        #[cfg(feature = "ui-metrics")]
        {
            runtime.reset_performance_metrics();
            runtime.reset_glyph_cache_metrics();
        }

        let invalidation = runtime.take_render_invalidation();
        if invalidation.is_none() {
            return None;
        }

        let frame_id = self.allocate_frame_id();

        let rendered = render_invalidation(frame_id, runtime, frame, invalidation)?;

        #[cfg(feature = "ui-metrics")]
        {
            crate::firmware::perf::log_ui_metrics(rendered.frame_id, runtime.performance_metrics());
            crate::firmware::perf::log_text_metrics(
                rendered.frame_id,
                rendered.eink_report,
                runtime.glyph_cache_metrics(),
                runtime.glyph_cache_used_bytes(),
                runtime.glyph_cache_capacity_bytes(),
            );
            crate::firmware::perf::log_coverage_metrics(
                rendered.frame_id,
                rendered.eink_report,
                rendered.framebuffer_draw_iter_pixels,
            );
        }

        let presentation = self.presentation_mode(rendered, capabilities);

        let refresh = self.refresh_policy.select(refresh_context(
            rendered,
            presentation,
            capabilities,
            self.panel_tone,
        ));

        self.record_presented_frame(rendered);

        Some(make_frame_update(rendered, refresh, presentation))
    }

    fn presentation_mode(
        &self,
        rendered: RenderedFrame,
        capabilities: EInkCapabilities,
    ) -> PresentationMode {
        if rendered.eink_report.tone() == EInkTone::Gray4 {
            return PresentationMode::Gray4;
        }

        // a complete binary repaint overwrites every physical pixel, so there is no
        // previous grayscale to preserve.
        if rendered.is_full_damage() || self.panel_tone == EInkTone::Binary {
            return PresentationMode::Binary;
        }

        match capabilities.binary_over_gray() {
            BinaryOverGrayMode::NativeWindow | BinaryOverGrayMode::PreconditionedWindow => {
                PresentationMode::BinaryPreservingGray
            }
            BinaryOverGrayMode::Unsupported => PresentationMode::Gray4,
        }
    }

    fn record_presented_frame(&mut self, rendered: RenderedFrame) {
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

        // a partial binary update might remove the last grayscale area, but without
        // spatial tone tracking we cannot prove that cheaply.
        // keep the conservative state until a full binary frame is presented.
        self.panel_tone = EInkTone::Gray4;
    }
}

fn render_invalidation(
    frame_id: u32,
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

    #[cfg(feature = "trace")]
    let trace_session = TraceSession::start();

    let render_trace = profile_span!(TraceEvent::Render);

    #[cfg(feature = "performance")]
    let mut timings = RenderTimings::default();

    let mut display = Framebuffer::new(frame, Orientation::Portrait);

    let (paint_report, eink_report) = {
        let mut painter = EInkPainter::new(&mut display)
            .with_ordered_coverage_blitter(Framebuffer::draw_ordered_coverage_bitmap)
            .with_ui_mode(EInkUiMode::BinaryDither);

        match invalidation.kind() {
            Invalidation::None => return None,
            Invalidation::Paint => {}
            Invalidation::Layout => {
                #[cfg(feature = "performance")]
                let layout_timer = CycleTimer::start();

                profile_expr!(
                    TraceEvent::Layout,
                    runtime
                        .layout(DISPLAY_SIZE)
                        .expect("layout requires a mounted root",),
                );

                #[cfg(feature = "performance")]
                {
                    timings.layout_cycles = layout_timer.elapsed();
                }
            }
            Invalidation::Rebuild => {
                #[cfg(feature = "performance")]
                let rebuild_timer = CycleTimer::start();

                profile_expr!(
                    TraceEvent::Rebuild,
                    runtime.rebuild().expect("UI rebuild capacity exceeded",),
                );

                #[cfg(feature = "performance")]
                {
                    timings.rebuild_cycles = rebuild_timer.elapsed();
                }

                #[cfg(feature = "performance")]
                let layout_timer = CycleTimer::start();

                profile_expr!(
                    TraceEvent::Layout,
                    runtime
                        .layout(DISPLAY_SIZE)
                        .expect("rebuilt UI must have a root",),
                );

                #[cfg(feature = "performance")]
                {
                    timings.layout_cycles = layout_timer.elapsed();
                }
            }
        }

        #[cfg(feature = "performance")]
        let clear_timer = CycleTimer::start();

        profile_expr!(
            TraceEvent::Clear,
            painter.clear_damage(damage, Color::WHITE,).unwrap(),
        );

        #[cfg(feature = "performance")]
        {
            timings.clear_cycles = clear_timer.elapsed();
        }

        #[cfg(feature = "performance")]
        let paint_timer = CycleTimer::start();

        let paint_report = profile_expr!(
            TraceEvent::Paint,
            runtime
                .paint_with_damage(damage, &mut painter,)
                .unwrap()
                .expect("painting requires a mounted root",),
        );

        #[cfg(feature = "performance")]
        {
            timings.paint_cycles = paint_timer.elapsed();
        }

        let eink_report = painter.report();

        (paint_report, eink_report)
    };

    #[cfg(feature = "performance")]
    let damage_timer = CycleTimer::start();

    let physical_damage = profile_expr!(TraceEvent::Damage, physical_damage(&display, damage,)?,);

    #[cfg(feature = "performance")]
    {
        timings.damage_cycles = damage_timer.elapsed();
    }

    #[cfg(feature = "ui-metrics")]
    let framebuffer_draw_iter_pixels = display.draw_iter_pixels();

    #[cfg(feature = "performance")]
    let ordered_coverage_calls = display.ordered_coverage_calls();

    #[cfg(feature = "performance")]
    let ordered_coverage_pixels = display.ordered_coverage_pixels();

    #[cfg(feature = "performance")]
    let ordered_coverage_cycles = display.ordered_coverage_cycles();

    drop(display);

    // the outer render span must close before the session is finalized so it is included
    // in the exported trace.
    drop(render_trace);

    #[cfg(feature = "trace")]
    let trace_summary = trace_session.finish();

    #[cfg(feature = "performance")]
    {
        crate::firmware::perf::log_render(frame_id, timings);
        crate::firmware::perf::log_ordered_coverage(
            frame_id,
            ordered_coverage_calls,
            ordered_coverage_pixels,
            ordered_coverage_cycles,
        );
    }

    #[cfg(feature = "trace")]
    crate::firmware::perf::log_trace(frame_id, trace_summary);

    #[cfg(feature = "performance")]
    let perf_report = FramePerfReport::new(timings);

    #[cfg(all(feature = "performance", feature = "ui-metrics"))]
    let perf_report = perf_report.with_framebuffer_pixels(framebuffer_draw_iter_pixels);

    Some(RenderedFrame {
        frame_id,
        physical_damage,
        eink_report,
        paint_report,

        #[cfg(feature = "performance")]
        perf_report,
        #[cfg(feature = "ui-metrics")]
        framebuffer_draw_iter_pixels,
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

fn refresh_context(
    rendered: RenderedFrame,
    presentation: PresentationMode,
    capabilities: EInkCapabilities,
    panel_tone: EInkTone,
) -> RefreshContext {
    let damage = rendered.physical_damage;
    let damaged_pixels = u32::from(damage.width).saturating_mul(u32::from(damage.height));

    let content = match presentation {
        PresentationMode::Binary | PresentationMode::BinaryPreservingGray => RefreshContent::Binary,
        PresentationMode::Gray4 => {
            let native_grayscale = rendered.eink_report.tone() == EInkTone::Gray4;
            let window_eligible = native_grayscale
                && panel_tone == EInkTone::Gray4
                && capabilities.supports_partial_grayscale();

            RefreshContent::Grayscale { window_eligible }
        }
    };

    RefreshContext::new(
        content,
        rendered.paint_report.damage().is_full(),
        rendered.paint_report.content().has_continuous_tone_images(),
        damaged_pixels,
        physical_display_pixels(),
    )
}

const fn physical_display_pixels() -> u32 {
    (PHYSICAL_WIDTH as u32) * (PHYSICAL_HEIGHT as u32)
}

fn make_frame_update(
    rendered: RenderedFrame,
    refresh: RefreshRequest,
    presentation: PresentationMode,
) -> FrameUpdate {
    let update = FrameUpdate::new(
        rendered.frame_id,
        refresh,
        rendered.physical_damage,
        rendered.eink_report,
        presentation,
        rendered.paint_report,
    );

    #[cfg(feature = "performance")]
    let update = update.with_perf_report(rendered.perf_report);

    update
}
