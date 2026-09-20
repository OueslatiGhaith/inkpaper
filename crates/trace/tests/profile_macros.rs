use inkpaper_trace::{
    TraceAggregate, TraceAggregates, TraceEvent, profile, profile_aggregate_expr, profile_expr,
    profile_scope, profile_span, trace_aggregate,
};

#[profile(TraceEvent::Paint)]
fn profiled_add(left: u32, right: u32) -> u32 {
    left + right
}

#[profile(TraceEvent::TextRun, arg = value)]
fn profiled_identity(value: usize) -> usize {
    value
}

struct Example;

impl Example {
    #[profile(TraceEvent::Layout)]
    fn profiled_method(&self, value: u32) -> u32 {
        value.saturating_mul(2)
    }
}

#[test]
fn profiling_macros_preserve_expression_results() {
    let result = profile_expr!(TraceEvent::Shape, arg = 12usize, 20u32 + 22u32,);

    assert_eq!(result, 42);

    {
        profile_scope!(TraceEvent::Glyphs, arg = 8usize);
    }

    let span = profile_span!(TraceEvent::Damage);
    drop(span);
}

#[test]
fn profile_attribute_preserves_function_behavior() {
    assert_eq!(profiled_add(20, 22), 42);
    assert_eq!(profiled_identity(37), 37);
    assert_eq!(Example.profiled_method(21), 42);
}

#[test]
fn aggregate_macros_preserve_expression_results() {
    let mut aggregates = TraceAggregates::new();

    trace_aggregate!(aggregates, TraceAggregate::ReaderPaginationWords,);

    trace_aggregate!(aggregates, TraceAggregate::ReaderMeasureTextBytes, 42usize,);

    let result =
        profile_aggregate_expr!(aggregates, TraceAggregate::ReaderMeasureText, 20u32 + 22u32,);

    assert_eq!(result, 42);
}
