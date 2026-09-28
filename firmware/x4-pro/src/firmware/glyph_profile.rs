//! splits the cost of drawing a new glyph on the device into its phases, so we know
//! which one to make cheaper. Runs once at boot, before the scheduler starts, so
//! nothing preempts it.
//!
//! every phase draws the same glyphs at the same sizes as the `glyph_hinting` bench,
//! so the cycles here line up with the host numbers.

use defmt::info;
use esp_hal::xtensa_lx::timer::get_cycle_count;
use inkpaper_ui::{
    FontData, FontFace, FontProperties, FontWeight, GlyphId, HintedTtfFont, TtfFont,
};
use skrifa::{
    FontRef, MetadataProvider,
    instance::Size,
    outline::{
        DrawSettings, Engine, GlyphStyles, HintingInstance, HintingOptions, OutlinePen,
        SmoothMode, Target,
    },
};

const INTER: TtfFont<'static> = TtfFont::from_data(
    FontData::new(include_bytes!("../../../../crates/app/assets/fonts/InterVariable.ttf")),
    0,
);

const SIZES: [u16; 3] = [14, 20, 32];

const TEXT: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.,;:!?";

const PROPERTIES: FontProperties = FontProperties::new(FontWeight::NORMAL);

/// the same target `HintedTtfFont` uses
const TARGET: Target = Target::Smooth {
    mode: SmoothMode::Normal,
    symmetric_rendering: true,
    preserve_linear_metrics: false,
};

/// the glyph cache lives in internal RAM, so the coverage does too
static mut COVERAGE: [u8; 64 * 64] = [0; 64 * 64];

const CYCLES_PER_US: u32 = 240;

/// cycles summed over every glyph of one size
#[derive(Default)]
struct Totals {
    unhinted_metrics: u64,
    unhinted_fill: u64,
    decode: u64,
    decode_warm: u64,
    decode_and_hint: u64,
    hinted_outline: u64,
    hinted_fill: u64,
    sample_rows: u64,
    commands: u64,
}

/// counts path commands and draws nothing, so drawing into it costs only what
/// skrifa does
#[derive(Default)]
struct CountingPen {
    commands: u64,
}

impl OutlinePen for CountingPen {
    fn move_to(&mut self, _x: f32, _y: f32) {
        self.commands += 1;
    }

    fn line_to(&mut self, _x: f32, _y: f32) {
        self.commands += 1;
    }

    fn quad_to(&mut self, _cx0: f32, _cy0: f32, _x: f32, _y: f32) {
        self.commands += 1;
    }

    fn curve_to(&mut self, _cx0: f32, _cy0: f32, _cx1: f32, _cy1: f32, _x: f32, _y: f32) {
        self.commands += 1;
    }

    fn close(&mut self) {
        self.commands += 1;
    }
}

fn cycles(run: impl FnOnce()) -> u64 {
    let started_at = get_cycle_count();
    run();
    u64::from(get_cycle_count().wrapping_sub(started_at))
}

fn us(cycles: u64) -> u64 {
    cycles / u64::from(CYCLES_PER_US)
}

pub(crate) fn run() {
    info!("glyph profile: start");

    #[allow(static_mut_refs)]
    // SAFETY: only this function touches the buffer, and it runs once at boot
    let coverage = unsafe { &mut COVERAGE };

    let glyphs = TEXT.chars().filter_map(|character| INTER.glyph_id(character));
    let font = FontRef::new(INTER.data().bytes()).expect("Inter parses");
    let outlines = font.outline_glyphs();
    let location = font.axes().location([("wght", 400.0)]);

    let styles_cycles = cycles(|| {
        core::hint::black_box(GlyphStyles::new(&outlines));
    });
    let styles = GlyphStyles::new(&outlines);

    info!(
        "glyph profile: glyph styles {=u64} us, once per font",
        us(styles_cycles)
    );

    let hinted = HintedTtfFont::new(INTER);

    // builds the font's glyph styles, so the per-size instance below costs only itself
    let _ = hinted.glyph_metrics_with_properties(PROPERTIES, GlyphId::new(0), 99);

    for size in SIZES {
        let mut instance = None;
        let instance_cycles = cycles(|| {
            instance = HintingInstance::new(
                &outlines,
                Size::new(f32::from(size)),
                &location,
                HintingOptions {
                    engine: Engine::Auto(Some(styles.clone())),
                    target: TARGET,
                },
            )
            .ok();
        });
        let instance = instance.expect("Inter hints");

        // builds `hinted`'s own instance for this size
        let _ = hinted.glyph_metrics_with_properties(PROPERTIES, GlyphId::new(0), size);

        let mut totals = Totals::default();
        let mut count = 0u64;

        for glyph in glyphs.clone() {
            let outline = outlines
                .get(skrifa::GlyphId::new(u32::from(glyph.value())))
                .expect("Inter has every bench glyph");

            let mut pen = CountingPen::default();

            totals.decode += cycles(|| {
                let _ = outline.draw(
                    DrawSettings::unhinted(Size::new(f32::from(size)), &location),
                    &mut pen,
                );
            });
            totals.commands += pen.commands;

            // the same glyph again, with its font data already in the flash cache
            totals.decode_warm += cycles(|| {
                let _ = outline.draw(
                    DrawSettings::unhinted(Size::new(f32::from(size)), &location),
                    &mut CountingPen::default(),
                );
            });

            totals.decode_and_hint += cycles(|| {
                let _ = outline.draw(
                    DrawSettings::hinted(&instance, false),
                    &mut CountingPen::default(),
                );
            });

            // what the glyph cache does on a miss, in the same order
            let mut metrics = None;
            totals.hinted_outline += cycles(|| {
                metrics = hinted.glyph_metrics_with_properties(PROPERTIES, glyph, size);
            });
            let metrics = metrics.expect("hinted glyph has metrics");
            let bytes = metrics.coverage_bytes().expect("glyph fits");

            totals.hinted_fill += cycles(|| {
                let _ = hinted.rasterize_with_properties(
                    PROPERTIES,
                    glyph,
                    size,
                    &mut coverage[..bytes],
                );
            });
            totals.sample_rows += u64::from(metrics.height) * 4;

            let mut metrics = None;
            totals.unhinted_metrics += cycles(|| {
                metrics = INTER.glyph_metrics_with_properties(PROPERTIES, glyph, size);
            });
            let bytes = metrics
                .expect("glyph has metrics")
                .coverage_bytes()
                .expect("glyph fits");

            totals.unhinted_fill += cycles(|| {
                let _ = INTER.rasterize_with_properties(
                    PROPERTIES,
                    glyph,
                    size,
                    &mut coverage[..bytes],
                );
            });

            count += 1;
        }

        let per_glyph = |total: u64| us(total / count);

        info!(
            "glyph profile: {=u16}px, {=u64} glyphs, us per glyph: new instance {=u64} (once) | unhinted: metrics {=u64}, fill {=u64} | skrifa: decode {=u64} (warm {=u64}), decode+hint {=u64} | hinted: outline {=u64}, fill {=u64} | {=u64} sample rows and {=u64} commands per glyph",
            size,
            count,
            us(instance_cycles),
            per_glyph(totals.unhinted_metrics),
            per_glyph(totals.unhinted_fill),
            per_glyph(totals.decode),
            per_glyph(totals.decode_warm),
            per_glyph(totals.decode_and_hint),
            per_glyph(totals.hinted_outline),
            per_glyph(totals.hinted_fill),
            totals.sample_rows / count,
            totals.commands / count,
        );
    }

    info!("glyph profile: done");
}
