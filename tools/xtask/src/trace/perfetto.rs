use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::trace::{Metric, MetricDefinition};

use super::{Callsite, Capture, Span, Value, parse_captures};

const PERFETTO_ROOT_TRACK: u64 = 1;
const PERFETTO_PACKET_SEQUENCE_ID: u64 = 1;

const PROTO_WIRE_VARINT: u8 = 0;
const PROTO_WIRE_LENGTH_DELIMITED: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventKind {
    Close,
    Instant,
    Open,
}

#[derive(Debug)]
enum AnnotationValue {
    Unsigned(u64),
    Signed(i64),
    Bool(bool),
    String(String),
}

#[derive(Debug)]
struct TimedEvent {
    kind: EventKind,
    timestamp_ns: u64,
    track_uuid: u64,
    name: Option<String>,
    category: Option<String>,
    annotations: Vec<(String, AnnotationValue)>,
    depth: u8,
}

pub fn convert_perfetto(input: &Path, output: Option<&Path>) -> Result<PathBuf> {
    let log = std::fs::read_to_string(input)
        .with_context(|| format!("failed to read {}", input.display()))?;

    let captures = parse_captures(&log)?;

    let trace = build_perfetto(&captures)?;

    let output = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_perfetto_output_path(input));

    std::fs::write(&output, trace)
        .with_context(|| format!("failed to write {}", output.display()))?;

    Ok(output)
}

fn build_perfetto(captures: &[Capture]) -> Result<Vec<u8>> {
    let first = captures
        .first()
        .context("cannot build an empty Perfetto trace")?;

    let clock_hz = first.clock_hz;

    if captures.iter().any(|capture| capture.clock_hz != clock_hz) {
        bail!("Perfetto export requires one stable trace clock frequency");
    }

    let origins = unwrap_capture_origins(captures)?;

    let timeline_base = origins[0];

    let tracks = build_target_tracks(captures);

    let mut events = Vec::new();

    for (capture, origin) in captures.iter().zip(origins.iter().copied()) {
        let relative_origin = origin
            .checked_sub(timeline_base)
            .context("trace origin moved before timeline base")?;

        for span in &capture.spans {
            let callsite = capture.callsites.get(&span.callsite_id).with_context(|| {
                format!(
                    "session {} references missing callsite {}",
                    capture.session_id, span.callsite_id,
                )
            })?;

            let track_uuid = *tracks.get(&callsite.target).with_context(|| {
                format!("missing Perfetto track for target `{}`", callsite.target)
            })?;

            let start_cycles = relative_origin
                .checked_add(u64::from(span.start_cycles))
                .context("trace start timestamp overflow")?;

            let end_cycles = start_cycles
                .checked_add(u64::from(span.duration_cycles))
                .context("trace end timestamp overflow")?;

            let start_ns = cycles_to_ns(start_cycles, clock_hz);

            let raw_end_ns = cycles_to_ns(end_cycles, clock_hz);

            // Perfetto slices are BEGIN/END pairs.
            // Preserve zero-cycle and sub-nanosecond spans as the smallest representable
            // positive slice.
            let end_ns = raw_end_ns.max(start_ns.saturating_add(1));

            push_slice(&mut events, track_uuid, callsite, span, start_ns, end_ns);
        }

        let metric_timestamp_ns = cycles_to_ns(relative_origin, clock_hz);

        for metric in capture.metrics.values() {
            let definition = capture
                .metric_definitions
                .get(&metric.id)
                .with_context(|| {
                    format!(
                        "session {} references missing metric definition {}",
                        capture.session_id, metric.id,
                    )
                })?;

            let track_uuid = *tracks.get(&definition.target).with_context(|| {
                format!(
                    "missing Perfetto track for metric target `{}`",
                    definition.target,
                )
            })?;

            push_metric(
                &mut events,
                track_uuid,
                definition,
                metric,
                metric_timestamp_ns,
            );
        }
    }

    events.sort_by(compare_events);

    let mut trace = Vec::new();

    push_track_descriptor(&mut trace, PERFETTO_ROOT_TRACK, None, "InkPaper");

    for (target, uuid) in &tracks {
        push_track_descriptor(&mut trace, *uuid, Some(PERFETTO_ROOT_TRACK), target);
    }

    for event in events {
        push_track_event(
            &mut trace,
            event.timestamp_ns,
            event.track_uuid,
            event.kind,
            event.name.as_deref(),
            event.category.as_deref(),
            &event.annotations,
        );
    }

    Ok(trace)
}

fn build_target_tracks(captures: &[Capture]) -> BTreeMap<String, u64> {
    let mut targets = BTreeSet::new();

    for capture in captures {
        for callsite in capture.callsites.values() {
            targets.insert(callsite.target.clone());
        }

        for metric in capture.metric_definitions.values() {
            targets.insert(metric.target.clone());
        }
    }

    targets
        .into_iter()
        .enumerate()
        .map(|(index, target)| {
            let uuid = 2 + u64::try_from(index).unwrap_or(u64::MAX - 2);

            (target, uuid)
        })
        .collect()
}

fn unwrap_capture_origins(captures: &[Capture]) -> Result<Vec<u64>> {
    let first = captures
        .first()
        .context("cannot unwrap origins for an empty trace")?;

    let mut origins = Vec::with_capacity(captures.len());

    let mut previous_raw = first.origin_cycles;

    let mut unwrapped = u64::from(first.origin_cycles);

    origins.push(unwrapped);

    for capture in &captures[1..] {
        let raw = capture.origin_cycles;

        let delta = raw.wrapping_sub(previous_raw);

        unwrapped = unwrapped
            .checked_add(u64::from(delta))
            .context("trace timeline overflow while unwrapping cycle counter")?;

        origins.push(unwrapped);

        previous_raw = raw;
    }

    Ok(origins)
}

fn push_slice(
    events: &mut Vec<TimedEvent>,
    track_uuid: u64,
    callsite: &Callsite,
    span: &Span,
    start_ns: u64,
    end_ns: u64,
) {
    let annotations = callsite
        .fields
        .iter()
        .cloned()
        .zip(span.values.iter().copied().map(annotation_from_value))
        .collect();

    events.push(TimedEvent {
        kind: EventKind::Open,
        timestamp_ns: start_ns,
        track_uuid,
        name: Some(callsite.name.clone()),
        category: Some(callsite.target.clone()),
        annotations,
        depth: span.depth,
    });

    events.push(TimedEvent {
        kind: EventKind::Close,
        timestamp_ns: end_ns,
        track_uuid,
        name: None,
        category: None,
        annotations: Vec::new(),
        depth: span.depth,
    });
}

fn annotation_from_value(value: Value) -> AnnotationValue {
    match value {
        Value::Unsigned(value) => AnnotationValue::Unsigned(u64::from(value)),

        Value::Signed(value) => AnnotationValue::Signed(i64::from(value)),

        Value::Bool(value) => AnnotationValue::Bool(value),
    }
}

fn push_metric(
    events: &mut Vec<TimedEvent>,
    track_uuid: u64,
    definition: &MetricDefinition,
    metric: &Metric,
    timestamp_ns: u64,
) {
    let annotations = vec![
        (
            "count".to_owned(),
            AnnotationValue::Unsigned(u64::from(metric.count)),
        ),
        ("sum".to_owned(), AnnotationValue::Unsigned(metric.sum)),
        (
            "max".to_owned(),
            AnnotationValue::Unsigned(u64::from(metric.max)),
        ),
        (
            "kind".to_owned(),
            AnnotationValue::String(definition.kind.as_str().to_owned()),
        ),
        (
            "unit".to_owned(),
            AnnotationValue::String(definition.unit.clone()),
        ),
    ];

    events.push(TimedEvent {
        kind: EventKind::Instant,
        timestamp_ns,
        track_uuid,
        name: Some(definition.name.clone()),
        category: Some(definition.target.clone()),
        annotations,
        depth: 0,
    });
}

fn compare_events(left: &TimedEvent, right: &TimedEvent) -> core::cmp::Ordering {
    left.timestamp_ns
        .cmp(&right.timestamp_ns)
        .then_with(|| left.track_uuid.cmp(&right.track_uuid))
        .then_with(|| event_kind_rank(left.kind).cmp(&event_kind_rank(right.kind)))
        .then_with(|| match (left.kind, right.kind) {
            (EventKind::Open, EventKind::Open) => left.depth.cmp(&right.depth),

            (EventKind::Close, EventKind::Close) => right.depth.cmp(&left.depth),

            _ => core::cmp::Ordering::Equal,
        })
}

const fn event_kind_rank(kind: EventKind) -> u8 {
    match kind {
        EventKind::Close => 0,
        EventKind::Instant => 1,
        EventKind::Open => 2,
    }
}

fn cycles_to_ns(cycles: u64, clock_hz: u32) -> u64 {
    let nanoseconds = u128::from(cycles) * 1_000_000_000u128 / u128::from(clock_hz);

    u64::try_from(nanoseconds).unwrap_or(u64::MAX)
}

fn default_perfetto_output_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("inkpaper-trace");

    input.with_file_name(format!("{stem}.perfetto-trace"))
}

fn push_track_descriptor(trace: &mut Vec<u8>, uuid: u64, parent_uuid: Option<u64>, name: &str) {
    let mut descriptor = Vec::new();

    // TrackDescriptor.uuid = 1
    proto_varint_field(&mut descriptor, 1, uuid);

    // TrackDescriptor.name = 2
    proto_bytes_field(&mut descriptor, 2, name.as_bytes());

    // TrackDescriptor.parent_uuid = 5
    if let Some(parent_uuid) = parent_uuid {
        proto_varint_field(&mut descriptor, 5, parent_uuid);
    }

    let mut packet = Vec::new();

    // TracePacket.track_descriptor = 60
    proto_bytes_field(&mut packet, 60, &descriptor);

    push_packet(trace, &packet);
}

fn push_track_event(
    trace: &mut Vec<u8>,
    timestamp_ns: u64,
    track_uuid: u64,
    kind: EventKind,
    name: Option<&str>,
    category: Option<&str>,
    annotations: &[(String, AnnotationValue)],
) {
    let mut event = Vec::new();

    let event_type = match kind {
        EventKind::Open => 1,
        EventKind::Close => 2,
        EventKind::Instant => 3,
    };

    // TrackEvent.type = 9
    proto_varint_field(&mut event, 9, event_type);

    // TrackEvent.track_uuid = 11
    proto_varint_field(&mut event, 11, track_uuid);

    // TrackEvent.name = 23
    if let Some(name) = name {
        proto_bytes_field(&mut event, 23, name.as_bytes());
    }

    // TrackEvent.categories = 22
    if let Some(category) = category {
        proto_bytes_field(&mut event, 22, category.as_bytes());
    }

    for (name, value) in annotations {
        push_debug_annotation(&mut event, name, value);
    }

    let mut packet = Vec::new();

    // TracePacket.timestamp = 8
    proto_varint_field(&mut packet, 8, timestamp_ns);

    // TracePacket.trusted_packet_sequence_id = 10
    proto_varint_field(&mut packet, 10, PERFETTO_PACKET_SEQUENCE_ID);

    // TracePacket.track_event = 11
    proto_bytes_field(&mut packet, 11, &event);

    push_packet(trace, &packet);
}

fn push_debug_annotation(event: &mut Vec<u8>, name: &str, value: &AnnotationValue) {
    let mut annotation = Vec::new();

    // DebugAnnotation.name = 10
    proto_bytes_field(&mut annotation, 10, name.as_bytes());

    match value {
        AnnotationValue::Bool(value) => {
            // bool_value = 2
            proto_varint_field(&mut annotation, 2, u64::from(*value));
        }

        AnnotationValue::Unsigned(value) => {
            // uint_value = 3
            proto_varint_field(&mut annotation, 3, *value);
        }

        AnnotationValue::Signed(value) => {
            // int_value = 4
            proto_varint_field(&mut annotation, 4, *value as u64);
        }

        AnnotationValue::String(value) => {
            // string_value = 6
            proto_bytes_field(&mut annotation, 6, value.as_bytes());
        }
    }

    // TrackEvent.debug_annotations = 4
    proto_bytes_field(event, 4, &annotation);
}

fn push_packet(trace: &mut Vec<u8>, packet: &[u8]) {
    // Trace.packet = 1
    proto_bytes_field(trace, 1, packet);
}

fn proto_varint_field(output: &mut Vec<u8>, field: u32, value: u64) {
    proto_key(output, field, PROTO_WIRE_VARINT);

    proto_varint(output, value);
}

fn proto_bytes_field(output: &mut Vec<u8>, field: u32, value: &[u8]) {
    proto_key(output, field, PROTO_WIRE_LENGTH_DELIMITED);

    proto_varint(output, value.len() as u64);

    output.extend_from_slice(value);
}

fn proto_key(output: &mut Vec<u8>, field: u32, wire_type: u8) {
    let key = (u64::from(field) << 3) | u64::from(wire_type);

    proto_varint(output, key);
}

fn proto_varint(output: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        output.push(((value as u8) & 0x7f) | 0x80);

        value >>= 7;
    }

    output.push(value as u8);
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::{build_perfetto, unwrap_capture_origins};

    use crate::trace::parse_captures;

    #[test]
    fn perfetto_export_is_generic_over_targets_and_fields() {
        let log = indoc! {r#"
            1.000 INFO trace/v2 capture session=1 hz=240000000 origin=1000 spans=2 dropped=0 open=0
            1.001 INFO trace/v2 define id=0 target=9:ui.render name=5:paint fields=1 5:frame
            1.002 INFO trace/v2 span id=0 depth=1 start=120 cycles=480 values=1 u:4
            1.003 INFO trace/v2 define id=1 target=9:ui.render name=6:render fields=1 5:frame
            1.004 INFO trace/v2 span id=1 depth=0 start=0 cycles=720 values=1 u:4
            1.005 INFO trace/v2 end session=1

            2.000 INFO trace/v2 capture session=2 hz=240000000 origin=4000 spans=2 dropped=0 open=0
            2.001 INFO trace/v2 define id=0 target=15:display.present name=9:busy_wait fields=1 5:index
            2.002 INFO trace/v2 span id=0 depth=1 start=240 cycles=480 values=1 u:0
            2.003 INFO trace/v2 define id=1 target=15:display.present name=7:present fields=1 5:frame
            2.004 INFO trace/v2 span id=1 depth=0 start=0 cycles=960 values=1 u:4
            2.005 INFO trace/v2 end session=2
        "#};

        let captures = parse_captures(log).unwrap();

        let trace = build_perfetto(&captures).unwrap();

        assert!(!trace.is_empty());

        assert!(contains_bytes(&trace, b"InkPaper"));

        assert!(contains_bytes(&trace, b"ui.render"));

        assert!(contains_bytes(&trace, b"display.present"));

        assert!(contains_bytes(&trace, b"paint"));

        assert!(contains_bytes(&trace, b"busy_wait"));

        // Annotation names should also be
        // present in the protobuf.
        assert!(contains_bytes(&trace, b"frame"));

        assert!(contains_bytes(&trace, b"index"));
    }

    #[test]
    fn perfetto_origin_unwraps_cycle_counter_wrap() {
        let log = indoc! {r#"
            1.000 INFO trace/v2 capture session=1 hz=240000000 origin=4294967000 spans=1 dropped=0 open=0
            1.001 INFO trace/v2 define id=0 target=4:test name=5:first fields=0
            1.002 INFO trace/v2 span id=0 depth=0 start=0 cycles=100 values=0
            1.003 INFO trace/v2 end session=1

            2.000 INFO trace/v2 capture session=2 hz=240000000 origin=1000 spans=1 dropped=0 open=0
            2.001 INFO trace/v2 define id=0 target=4:test name=6:second fields=0
            2.002 INFO trace/v2 span id=0 depth=0 start=0 cycles=100 values=0
            2.003 INFO trace/v2 end session=2
        "#};

        let captures = parse_captures(log).unwrap();

        let origins = unwrap_capture_origins(&captures).unwrap();

        let expected_delta = 1000u32.wrapping_sub(4_294_967_000u32);

        assert_eq!(origins.len(), 2);

        assert_eq!(origins[1] - origins[0], u64::from(expected_delta));
    }

    #[test]
    fn perfetto_preserves_signed_and_boolean_annotations() {
        let log = indoc! {r#"
            1.000 INFO trace/v2 capture session=1 hz=240000000 origin=100 spans=1 dropped=0 open=0
            1.001 INFO trace/v2 define id=0 target=4:test name=6:values fields=2 5:delta 5:ready
            1.002 INFO trace/v2 span id=0 depth=0 start=0 cycles=10 values=2 i:-17 b:0
            1.003 INFO trace/v2 end session=1
        "#};

        let captures = parse_captures(log).unwrap();

        let trace = build_perfetto(&captures).unwrap();

        assert!(contains_bytes(&trace, b"delta"));

        assert!(contains_bytes(&trace, b"ready"));
    }

    fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    #[test]
    fn perfetto_exports_metric_summaries_as_instants() {
        let log = indoc! {r#"
            1.000 INFO trace/v2 capture session=1 hz=240000000 origin=100 spans=0 dropped=0 open=0
            1.001 INFO trace/v2 metrics count=2 dropped=0
            1.002 INFO trace/v2 metric_define id=0 target=17:reader.pagination name=10:cache_hits kind=counter unit=0:
            1.003 INFO trace/v2 metric id=0 count=3 sum=5 max=3
            1.004 INFO trace/v2 metric_define id=1 target=17:reader.pagination name=12:measure_text kind=distribution unit=6:cycles
            1.005 INFO trace/v2 metric id=1 count=4 sum=120 max=50
            1.006 INFO trace/v2 end session=1
        "#};

        let captures = parse_captures(log).unwrap();

        let trace = build_perfetto(&captures).unwrap();

        assert!(contains_bytes(&trace, b"reader.pagination"));

        assert!(contains_bytes(&trace, b"cache_hits"));

        assert!(contains_bytes(&trace, b"measure_text"));

        assert!(contains_bytes(&trace, b"count"));

        assert!(contains_bytes(&trace, b"sum"));

        assert!(contains_bytes(&trace, b"max"));

        assert!(contains_bytes(&trace, b"kind"));

        assert!(contains_bytes(&trace, b"unit"));

        assert!(contains_bytes(&trace, b"distribution"));

        assert!(contains_bytes(&trace, b"cycles"));
    }
}
