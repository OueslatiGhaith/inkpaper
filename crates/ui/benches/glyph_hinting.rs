//! what autohinting costs per glyph, and what a new size costs before the first
//! glyph, compared with the unhinted rasterizer.
//!
//! also prints the heap a hinted font holds, which criterion can't report.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering},
};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use inkpaper_ui::{
    FontData, FontFace, FontProperties, FontWeight, GlyphId, HintedTtfFont, TtfFont,
};

const INTER: TtfFont<'static> = TtfFont::from_data(
    FontData::new(include_bytes!("../../app/assets/fonts/InterVariable.ttf")),
    0,
);

const SIZES: [u16; 4] = [14, 20, 26, 32];

const TEXT: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.,;:!?";

struct CountingAllocator;

static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn glyphs() -> Vec<GlyphId> {
    TEXT.chars()
        .map(|character| {
            INTER
                .glyph_id(character)
                .expect("Inter covers the bench text")
        })
        .collect()
}

/// what the glyph cache does on a miss
fn rasterize_all(font: &dyn FontFace, glyphs: &[GlyphId], size: u16, coverage: &mut Vec<u8>) {
    let properties = FontProperties::new(FontWeight::NORMAL);

    for glyph in glyphs {
        let metrics = font
            .glyph_metrics_with_properties(properties, *glyph, size)
            .expect("glyph has metrics");

        coverage.resize(metrics.coverage_bytes().expect("glyph fits"), 0);

        font.rasterize_with_properties(properties, *glyph, size, coverage)
            .expect("glyph rasterizes");

        black_box(&coverage);
    }
}

fn report_heap(glyphs: &[GlyphId]) {
    let mut coverage = Vec::new();
    let before = LIVE_BYTES.load(Ordering::Relaxed);
    let font = HintedTtfFont::new(INTER);

    rasterize_all(&font, glyphs, SIZES[0], &mut coverage);
    let first_size = LIVE_BYTES.load(Ordering::Relaxed) - before;

    for size in &SIZES[1..] {
        rasterize_all(&font, glyphs, *size, &mut coverage);
    }
    let all_sizes = LIVE_BYTES.load(Ordering::Relaxed) - before;

    eprintln!(
        "hinted font heap: {first_size} bytes after one size, {all_sizes} bytes after {} sizes",
        SIZES.len()
    );

    drop(font);
}

fn bench_glyph_hinting(c: &mut Criterion) {
    let glyphs = glyphs();

    report_heap(&glyphs);

    let mut group = c.benchmark_group("glyph_hinting");
    let mut coverage = Vec::new();

    group.throughput(Throughput::Elements(glyphs.len() as u64));

    for size in SIZES {
        group.bench_with_input(BenchmarkId::new("unhinted", size), &size, |b, size| {
            b.iter(|| rasterize_all(&INTER, &glyphs, *size, &mut coverage));
        });

        let hinted = HintedTtfFont::new(INTER);
        rasterize_all(&hinted, &glyphs, size, &mut coverage);

        group.bench_with_input(BenchmarkId::new("hinted", size), &size, |b, size| {
            b.iter(|| rasterize_all(&hinted, &glyphs, *size, &mut coverage));
        });
    }

    group.finish();

    let mut group = c.benchmark_group("glyph_hinting_first_glyph");

    // a font that has never hinted anything first scans the whole font for styles
    group.bench_function("new_font", |b| {
        b.iter(|| {
            let font = HintedTtfFont::new(INTER);
            rasterize_all(&font, &glyphs[..1], SIZES[1], &mut coverage);
        });
    });

    // then each new size builds a hinting instance
    let font = HintedTtfFont::new(INTER);
    let mut size = 10u16;

    group.bench_function("new_size", |b| {
        b.iter(|| {
            size = if size >= 60 { 10 } else { size + 1 };
            rasterize_all(&font, &glyphs[..1], size, &mut coverage);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_glyph_hinting);
criterion_main!(benches);
