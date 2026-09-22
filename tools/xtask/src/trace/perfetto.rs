use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::trace::{Metric, MetricDefinition, SpanKind};

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

#[derive(Debug)]
struct AsyncTrackDescriptor {
    uuid: u64,
    parent_uuid: u64,
    name: String,
}

#[derive(Debug)]
struct TrackLayout {
    target_tracks: BTreeMap<String, u64>,
    async_span_tracks: BTreeMap<(u32, usize), u64>,
    async_tracks: Vec<AsyncTrackDescriptor>,
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

    let timeline_base = timeline_base(captures)?;

    let tracks = build_tracks(captures)?;

    let mut events = Vec::new();

    for capture in captures {
        for (span_index, span) in capture.spans.iter().enumerate() {
            let callsite = capture.callsites.get(&span.callsite_id).with_context(|| {
                format!(
                    "capture {} references missing callsite {}",
                    capture.capture_id, span.callsite_id,
                )
            })?;

            let track_uuid = match span.kind {
                SpanKind::Sync => {
                    *tracks
                        .target_tracks
                        .get(&callsite.target)
                        .with_context(|| {
                            format!("missing Perfetto track for target `{}`", callsite.target)
                        })?
                }

                SpanKind::Async => *tracks
                    .async_span_tracks
                    .get(&(capture.capture_id, span_index))
                    .with_context(|| {
                        format!(
                            "missing Perfetto async lane for capture {} span {}",
                            capture.capture_id, span_index,
                        )
                    })?,
            };

            let start_cycles = span
                .start_cycles
                .checked_sub(timeline_base)
                .context("span starts before trace timeline base")?;

            let end_cycles = start_cycles
                .checked_add(span.duration_cycles)
                .context("trace end timestamp overflow")?;

            let start_ns = cycles_to_ns(start_cycles, clock_hz);

            let raw_end_ns = cycles_to_ns(end_cycles, clock_hz);

            // Perfetto slices are BEGIN/END pairs.
            // Preserve zero-cycle and sub-nanosecond spans as the smallest representable
            // positive slice.
            let end_ns = raw_end_ns.max(start_ns.saturating_add(1));

            push_slice(&mut events, track_uuid, callsite, span, start_ns, end_ns);
        }

        let capture_cycles = capture
            .captured_at_cycles
            .checked_sub(timeline_base)
            .context("capture timestamp before trace timeline base")?;

        let capture_ns = cycles_to_ns(capture_cycles, clock_hz);

        for metric in capture.metrics.values() {
            let definition = capture
                .metric_definitions
                .get(&metric.id)
                .with_context(|| {
                    format!(
                        "capture {} references missing metric definition {}",
                        capture.capture_id, metric.id,
                    )
                })?;

            let track_uuid = *tracks
                .target_tracks
                .get(&definition.target)
                .with_context(|| {
                    format!(
                        "missing Perfetto track for metric target `{}`",
                        definition.target,
                    )
                })?;

            push_metric(&mut events, track_uuid, definition, metric, capture_ns);
        }

        if capture.overwritten != 0 {
            push_loss_event(
                &mut events,
                "trace.overwritten",
                capture.overwritten,
                capture_ns,
            );
        }

        if capture.metric_dropped != 0 {
            push_loss_event(
                &mut events,
                "trace.metric_dropped",
                u64::from(capture.metric_dropped),
                capture_ns,
            );
        }
    }

    events.sort_by(compare_events);

    let mut trace = Vec::new();

    push_track_descriptor(&mut trace, PERFETTO_ROOT_TRACK, None, "InkPaper");

    for (target, uuid) in &tracks.target_tracks {
        push_track_descriptor(&mut trace, *uuid, Some(PERFETTO_ROOT_TRACK), target);
    }

    for track in &tracks.async_tracks {
        push_track_descriptor(&mut trace, track.uuid, Some(track.parent_uuid), &track.name);
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

fn build_tracks(captures: &[Capture]) -> Result<TrackLayout> {
    let mut targets = BTreeSet::new();

    for capture in captures {
        for callsite in capture.callsites.values() {
            targets.insert(callsite.target.clone());
        }

        for metric in capture.metric_definitions.values() {
            targets.insert(metric.target.clone());
        }
    }

    let mut target_tracks = BTreeMap::new();

    let mut next_uuid = 2u64;

    for target in targets {
        let uuid = next_uuid;

        next_uuid = next_uuid
            .checked_add(1)
            .context("Perfetto track UUID overflow")?;

        target_tracks.insert(target, uuid);
    }

    let mut async_span_tracks = BTreeMap::new();
    let mut async_tracks = Vec::new();

    for (target, parent_uuid) in &target_tracks {
        let mut intervals = Vec::new();

        for capture in captures {
            for (span_index, span) in capture.spans.iter().enumerate() {
                if span.kind != SpanKind::Async {
                    continue;
                }

                let callsite = capture.callsites.get(&span.callsite_id).with_context(|| {
                    format!(
                        "capture {} references missing callsite {}",
                        capture.capture_id, span.callsite_id,
                    )
                })?;

                if &callsite.target != target {
                    continue;
                }

                let end = span
                    .start_cycles
                    .checked_add(span.duration_cycles)
                    .context("async span end timestamp overflow")?;

                intervals.push((span.start_cycles, end, capture.capture_id, span_index));
            }
        }

        intervals.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
                .then_with(|| left.3.cmp(&right.3))
        });

        let mut lane_ends: Vec<u64> = Vec::new();
        let mut lane_uuids: Vec<u64> = Vec::new();

        for (start, end, capture_id, span_index) in intervals {
            let lane = if let Some(lane) = lane_ends.iter().position(|lane_end| *lane_end <= start)
            {
                lane
            } else {
                let lane = lane_ends.len();

                let uuid = next_uuid;

                next_uuid = next_uuid
                    .checked_add(1)
                    .context("Perfetto track UUID overflow")?;

                lane_ends.push(0);
                lane_uuids.push(uuid);

                async_tracks.push(AsyncTrackDescriptor {
                    uuid,
                    parent_uuid: *parent_uuid,
                    name: format!("async {lane}"),
                });

                lane
            };

            lane_ends[lane] = end;

            async_span_tracks.insert((capture_id, span_index), lane_uuids[lane]);
        }
    }

    Ok(TrackLayout {
        target_tracks,
        async_span_tracks,
        async_tracks,
    })
}

fn timeline_base(captures: &[Capture]) -> Result<u64> {
    captures
        .iter()
        .flat_map(|capture| {
            core::iter::once(capture.captured_at_cycles)
                .chain(capture.spans.iter().map(|span| span.start_cycles))
        })
        .min()
        .context("cannot establish trace timeline base")
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

    let depth = match span.kind {
        SpanKind::Sync => span.depth,
        SpanKind::Async => 0,
    };

    events.push(TimedEvent {
        kind: EventKind::Open,
        timestamp_ns: start_ns,
        track_uuid,
        name: Some(callsite.name.clone()),
        category: Some(callsite.target.clone()),
        annotations,
        depth,
    });

    events.push(TimedEvent {
        kind: EventKind::Close,
        timestamp_ns: end_ns,
        track_uuid,
        name: None,
        category: None,
        annotations: Vec::new(),
        depth,
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

fn push_loss_event(events: &mut Vec<TimedEvent>, name: &str, count: u64, timestamp_ns: u64) {
    events.push(TimedEvent {
        kind: EventKind::Instant,

        timestamp_ns,

        track_uuid: PERFETTO_ROOT_TRACK,

        name: Some(name.to_owned()),

        category: Some("trace".to_owned()),

        annotations: vec![("count".to_owned(), AnnotationValue::Unsigned(count))],

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

    use super::build_perfetto;

    use crate::trace::{
        parse_captures,
        perfetto::{build_tracks, timeline_base},
    };

    #[test]
    fn perfetto_export_is_generic_over_targets_and_fields() {
        let log = indoc! {r#"
            1.000 INFO trace/v3 capture id=1 hz=240000000 at=1800 spans=2 overwritten=0 metrics=0 metric_dropped=0
            1.001 INFO trace/v3 define id=0 target=9:ui.render name=5:paint fields=1 5:frame
            1.002 INFO trace/v3 span id=0 kind=sync depth=1 start=1120 cycles=480 values=1 u:4
            1.003 INFO trace/v3 define id=1 target=9:ui.render name=6:render fields=1 5:frame
            1.004 INFO trace/v3 span id=1 kind=sync depth=0 start=1000 cycles=720 values=1 u:4
            1.005 INFO trace/v3 end id=1

            2.000 INFO trace/v3 capture id=2 hz=240000000 at=5000 spans=2 overwritten=0 metrics=0 metric_dropped=0
            2.001 INFO trace/v3 define id=0 target=15:display.present name=9:busy_wait fields=1 5:index
            2.002 INFO trace/v3 span id=0 kind=sync depth=1 start=4240 cycles=480 values=1 u:0
            2.003 INFO trace/v3 define id=1 target=15:display.present name=7:present fields=1 5:frame
            2.004 INFO trace/v3 span id=1 kind=sync depth=0 start=4000 cycles=960 values=1 u:4
            2.005 INFO trace/v3 end id=2
        "#};

        let captures = parse_captures(log).unwrap();

        let trace = build_perfetto(&captures).unwrap();

        assert!(!trace.is_empty());

        assert!(contains_bytes(&trace, b"InkPaper"));
        assert!(contains_bytes(&trace, b"ui.render"));
        assert!(contains_bytes(&trace, b"display.present"));
        assert!(contains_bytes(&trace, b"paint"));
        assert!(contains_bytes(&trace, b"busy_wait"));
        assert!(contains_bytes(&trace, b"frame"));
        assert!(contains_bytes(&trace, b"index"));
    }

    #[test]
    fn capture_chunks_share_one_absolute_timeline() {
        let log = indoc! {r#"
            1.000 INFO trace/v3 capture id=1 hz=240000000 at=200 spans=1 overwritten=0 metrics=0 metric_dropped=0
            1.001 INFO trace/v3 define id=0 target=4:test name=5:first fields=0
            1.002 INFO trace/v3 span id=0 kind=sync depth=0 start=100 cycles=50 values=0
            1.003 INFO trace/v3 end id=1

            2.000 INFO trace/v3 capture id=2 hz=240000000 at=500 spans=1 overwritten=0 metrics=0 metric_dropped=0
            2.001 INFO trace/v3 define id=0 target=4:test name=6:second fields=0
            2.002 INFO trace/v3 span id=0 kind=sync depth=0 start=300 cycles=100 values=0
            2.003 INFO trace/v3 end id=2
        "#};

        let captures = parse_captures(log).unwrap();

        assert_eq!(timeline_base(&captures).unwrap(), 100);

        let trace = build_perfetto(&captures).unwrap();

        assert!(contains_bytes(&trace, b"first"));
        assert!(contains_bytes(&trace, b"second"));
    }

    #[test]
    fn perfetto_preserves_signed_and_boolean_annotations() {
        let log = indoc! {r#"
            1.000 INFO trace/v3 capture id=1 hz=240000000 at=120 spans=1 overwritten=0 metrics=0 metric_dropped=0
            1.001 INFO trace/v3 define id=0 target=4:test name=6:values fields=2 5:delta 5:ready
            1.002 INFO trace/v3 span id=0 kind=sync depth=0 start=100 cycles=10 values=2 i:-17 b:0
            1.003 INFO trace/v3 end id=1
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
        1.000 INFO trace/v3 capture id=1 hz=240000000 at=100 spans=0 overwritten=0 metrics=2 metric_dropped=0
        1.001 INFO trace/v3 metric_define id=0 target=17:reader.pagination name=10:cache_hits kind=counter unit=0:
        1.002 INFO trace/v3 metric id=0 count=3 sum=5 max=3
        1.003 INFO trace/v3 metric_define id=1 target=17:reader.pagination name=12:measure_text kind=distribution unit=6:cycles
        1.004 INFO trace/v3 metric id=1 count=4 sum=120 max=50
        1.005 INFO trace/v3 end id=1
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

    #[test]
    fn perfetto_exports_flight_recorder_loss_diagnostics() {
        let log = indoc! {r#"
        1.000 INFO trace/v3 capture id=1 hz=240000000 at=100 spans=0 overwritten=12 metrics=0 metric_dropped=3
        1.001 INFO trace/v3 end id=1
    "#};

        let captures = parse_captures(log).unwrap();

        let trace = build_perfetto(&captures).unwrap();

        assert!(contains_bytes(&trace, b"trace.overwritten"));

        assert!(contains_bytes(&trace, b"trace.metric_dropped"));

        assert!(contains_bytes(&trace, b"count"));
    }

    #[test]
    fn overlapping_async_spans_get_separate_lanes() {
        let log = indoc! {r#"
        1.000 INFO trace/v3 capture id=1 hz=240000000 at=400 spans=3 overwritten=0 metrics=0 metric_dropped=0
        1.001 INFO trace/v3 define id=0 target=13:reader.loader name=5:first fields=0
        1.002 INFO trace/v3 span id=0 kind=async depth=0 start=100 cycles=100 values=0
        1.003 INFO trace/v3 define id=1 target=13:reader.loader name=6:second fields=0
        1.004 INFO trace/v3 span id=1 kind=async depth=0 start=150 cycles=50 values=0
        1.005 INFO trace/v3 define id=2 target=13:reader.loader name=5:third fields=0
        1.006 INFO trace/v3 span id=2 kind=async depth=0 start=200 cycles=20 values=0
        1.007 INFO trace/v3 end id=1
    "#};

        let captures = parse_captures(log).unwrap();

        let tracks = build_tracks(&captures).unwrap();

        assert_eq!(tracks.async_tracks.len(), 2);

        let first = tracks.async_span_tracks[&(1, 0)];

        let second = tracks.async_span_tracks[&(1, 1)];

        let third = tracks.async_span_tracks[&(1, 2)];

        assert_ne!(first, second);

        assert_eq!(first, third);

        let trace = build_perfetto(&captures).unwrap();

        assert!(contains_bytes(&trace, b"async 0"));

        assert!(contains_bytes(&trace, b"async 1"));
    }
}
