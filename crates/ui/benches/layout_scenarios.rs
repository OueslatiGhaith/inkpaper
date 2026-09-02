use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

use inkpaper_ui::{Offset, Point, px};
use support::{
    BenchScenario, MIXED_SCREEN_SIZES, REPRESENTATIVE_CASES, SCROLL_LIST_SIZES, VIEWPORT, setup,
};

use crate::support::{DEEP_CHAIN_SIZES, NESTED_SCROLL_DEPTHS};

mod support;

fn benchmark_layout_scenarios(criterion: &mut Criterion) {
    for &(scenario, sizes) in REPRESENTATIVE_CASES {
        let mut group = criterion.benchmark_group(format!("layout_scenarios/{}", scenario.name(),));

        for &size in sizes {
            group.throughput(Throughput::Elements(size as u64));

            let (mut runtime, _, painter) = setup(scenario, size);

            group.bench_with_input(BenchmarkId::from_parameter(size), &size, |bencher, _| {
                bencher.iter(|| {
                    black_box(
                        runtime
                            .layout_with_measurer(black_box(VIEWPORT), black_box(&painter))
                            .unwrap(),
                    );
                });
            });
        }

        group.finish();
    }
}

fn benchmark_paint_scenario(criterion: &mut Criterion, scenario: BenchScenario, sizes: &[usize]) {
    let mut group = criterion.benchmark_group(format!("paint_scenarios/{}", scenario.name(),));

    for &size in sizes {
        group.throughput(Throughput::Elements(size as u64));

        let (mut runtime, _, mut painter) = setup(scenario, size);

        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |bencher, _| {
            bencher.iter(|| {
                black_box(runtime.paint(black_box(&mut painter)).unwrap());

                black_box(painter.draw_calls());
            });
        });
    }

    group.finish();
}

fn benchmark_focus_scroll_into_view(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("focus_scroll_into_view/nested_scroll");

    for &depth in NESTED_SCROLL_DEPTHS {
        group.throughput(Throughput::Elements(depth as u64));

        let (mut runtime, _, _) = setup(BenchScenario::NestedScrollFocus, depth);

        // establish focus and the final scroll offsets before timing.
        //
        // there is only one focusable element, so the following focus_next() calls
        // wrap back to the same target and exercise the "ensure focused element is
        // visible" path without changing the benchmark state each iteration.
        assert!(runtime.focus_next(),);

        group.bench_with_input(BenchmarkId::from_parameter(depth), &depth, |bencher, _| {
            bencher.iter(|| {
                black_box(runtime.focus_next());
            });
        });
    }

    group.finish();
}

fn benchmark_paint_scenarios(criterion: &mut Criterion) {
    benchmark_paint_scenario(criterion, BenchScenario::ScrollList, SCROLL_LIST_SIZES);

    benchmark_paint_scenario(criterion, BenchScenario::MixedScreen, MIXED_SCREEN_SIZES);

    benchmark_paint_scenario(criterion, BenchScenario::DeepChain, DEEP_CHAIN_SIZES);
}

fn benchmark_mixed_full_frame(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("full_frame_scenarios/mixed_screen");

    for &size in MIXED_SCREEN_SIZES {
        group.throughput(Throughput::Elements(size as u64));

        let (mut runtime, app, mut painter) = setup(BenchScenario::MixedScreen, size);

        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |bencher, _| {
            bencher.iter(|| {
                runtime.rebuild(black_box(app)).unwrap();

                black_box(
                    runtime
                        .layout_with_measurer(black_box(VIEWPORT), black_box(&painter))
                        .unwrap(),
                );

                black_box(runtime.paint(black_box(&mut painter)).unwrap());

                black_box(painter.draw_calls());
            });
        });
    }

    group.finish();
}

fn benchmark_scroll_hit_test(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("hit_test_scroll/scroll_list");

    let position = Point::new(px(8), px(8));

    for &size in SCROLL_LIST_SIZES {
        group.throughput(Throughput::Elements(size as u64));

        let (mut runtime, _, _) = setup(BenchScenario::ScrollList, size);

        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |bencher, _| {
            bencher.iter(|| {
                black_box(runtime.scroll_at(black_box(position), black_box(Offset::ZERO)));
            });
        });
    }

    group.finish();
}

criterion_group!(
    scenarios,
    benchmark_layout_scenarios,
    benchmark_paint_scenarios,
    benchmark_scroll_hit_test,
    benchmark_focus_scroll_into_view,
    benchmark_mixed_full_frame,
);

criterion_main!(scenarios);
