use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use inkpaper_trace::TraceEvent;

use crate::trace::{
    Capture, EventKind, parse_captures, trace_event_name, trace_frame_key, trace_frame_name,
};

const PERFETTO_ROOT_TRACK: u64 = 1;

const PERFETTO_FRAMES_TRACK: u64 = 2;

const PERFETTO_PACKET_SEQUENCE_ID: u64 = 1;

const PERFETTO_CPU_GROUP: u64 = 3;
const PERFETTO_CPU_RENDER_TRACK: u64 = 4;
const PERFETTO_CPU_TEXT_TRACK: u64 = 5;

const PERFETTO_DISPLAY_GROUP: u64 = 6;
const PERFETTO_DISPLAY_PRESENT_TRACK: u64 = 7;
const PERFETTO_DISPLAY_BUSY_TRACK: u64 = 8;

const PROTO_WIRE_VARINT: u8 = 0;
const PROTO_WIRE_LENGTH_DELIMITED: u8 = 2;

const PERFETTO_DISPLAY_PHASE_TRACK: u64 = 9;
const PERFETTO_DISPLAY_STATE_TRACK: u64 = 10;

#[derive(Debug)]
struct PerfettoTimedEvent {
    kind: EventKind,
    timestamp_ns: u64,
    track_uuid: u64,
    name: Option<String>,
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
    let Some(first) = captures.first() else {
        bail!("cannot build an empty Perfetto trace");
    };

    let hz = first.hz;

    if captures.iter().any(|capture| capture.hz != hz) {
        bail!("Perfetto export requires a stable trace clock rate across all sessions");
    }

    let origins = unwrap_capture_origins(captures)?;

    let mut timeline_base = origins[0];

    // async records use absolute raw cycle-counter values. Find their unwrapped starts
    // first because a deferred operation may have begun before the session which logs
    // its completion.
    for (capture, origin) in captures.iter().zip(origins.iter().copied()) {
        let raw_origin = capture
            .origin_cycles
            .with_context(|| format!("trace session {} has no origin", capture.id,))?;

        for span in &capture.async_spans {
            let start = unwrap_cycle_near(span.start_cycles, raw_origin, origin)?;

            timeline_base = timeline_base.min(start);
        }
    }

    let mut events = Vec::new();
    let mut frame_bounds = BTreeMap::<u32, (u64, u64)>::new();

    for (capture, origin) in captures.iter().zip(origins.iter().copied()) {
        let relative_origin = origin
            .checked_sub(timeline_base)
            .context("trace origin moved before timeline base")?;

        for span in &capture.spans {
            let start_cycles = relative_origin + u64::from(span.start_cycles);
            let end_cycles = start_cycles + u64::from(span.duration_cycles);

            let start_ns = cycles_to_ns_u64(start_cycles, hz);
            let end_ns = cycles_to_ns_u64(end_cycles, hz);

            let key = trace_frame_key(span);
            let name = trace_frame_name(key);
            let track_uuid = perfetto_track_for_event(span.event);

            push_perfetto_slice(&mut events, track_uuid, name, start_ns, end_ns, span.depth);

            let bounds = frame_bounds
                .entry(capture.frame_id)
                .or_insert((start_ns, end_ns));

            bounds.0 = bounds.0.min(start_ns);
            bounds.1 = bounds.1.max(end_ns);
        }
    }

    // emit completed asynchronous controller operations.
    for (capture, origin) in captures.iter().zip(origins.iter().copied()) {
        let raw_origin = capture
            .origin_cycles
            .with_context(|| format!("trace session {} has no origin", capture.id,))?;

        for span in &capture.async_spans {
            let absolute_start = unwrap_cycle_near(span.start_cycles, raw_origin, origin)?;

            let absolute_end = absolute_start
                .checked_add(u64::from(span.duration_cycles))
                .context("async trace interval overflow")?;

            let start_cycles = absolute_start
                .checked_sub(timeline_base)
                .context("async span moved before timeline base")?;

            let end_cycles = absolute_end
                .checked_sub(timeline_base)
                .context("async span moved before timeline base")?;

            let name = trace_event_name(span.event, Some(span.arg));

            push_perfetto_slice(
                &mut events,
                PERFETTO_DISPLAY_STATE_TRACK,
                name,
                cycles_to_ns_u64(start_cycles, hz),
                cycles_to_ns_u64(end_cycles, hz),
                0,
            );
        }
    }

    for (frame_id, (start_ns, end_ns)) in frame_bounds {
        push_perfetto_slice(
            &mut events,
            PERFETTO_FRAMES_TRACK,
            format!("frame {frame_id}"),
            start_ns,
            end_ns,
            0,
        );
    }

    events.sort_by(|a, b| {
        a.timestamp_ns
            .cmp(&b.timestamp_ns)
            .then_with(|| a.track_uuid.cmp(&b.track_uuid))
            .then_with(|| match (a.kind, b.kind) {
                (EventKind::Close, EventKind::Open) => core::cmp::Ordering::Less,
                (EventKind::Open, EventKind::Close) => core::cmp::Ordering::Greater,
                (EventKind::Open, EventKind::Open) => a.depth.cmp(&b.depth),
                (EventKind::Close, EventKind::Close) => b.depth.cmp(&a.depth),
            })
    });

    let mut trace = Vec::new();

    push_perfetto_track_descriptor(&mut trace, PERFETTO_ROOT_TRACK, None, "InkPaper");

    push_perfetto_track_descriptor(
        &mut trace,
        PERFETTO_FRAMES_TRACK,
        Some(PERFETTO_ROOT_TRACK),
        "Frames",
    );

    push_perfetto_track_descriptor(
        &mut trace,
        PERFETTO_CPU_GROUP,
        Some(PERFETTO_ROOT_TRACK),
        "CPU",
    );

    push_perfetto_track_descriptor(
        &mut trace,
        PERFETTO_CPU_RENDER_TRACK,
        Some(PERFETTO_CPU_GROUP),
        "Render",
    );

    push_perfetto_track_descriptor(
        &mut trace,
        PERFETTO_CPU_TEXT_TRACK,
        Some(PERFETTO_CPU_GROUP),
        "Text",
    );

    push_perfetto_track_descriptor(
        &mut trace,
        PERFETTO_DISPLAY_GROUP,
        Some(PERFETTO_ROOT_TRACK),
        "Display",
    );

    push_perfetto_track_descriptor(
        &mut trace,
        PERFETTO_DISPLAY_PRESENT_TRACK,
        Some(PERFETTO_DISPLAY_GROUP),
        "Present",
    );

    push_perfetto_track_descriptor(
        &mut trace,
        PERFETTO_DISPLAY_BUSY_TRACK,
        Some(PERFETTO_DISPLAY_GROUP),
        "BUSY",
    );

    push_perfetto_track_descriptor(
        &mut trace,
        PERFETTO_DISPLAY_PHASE_TRACK,
        Some(PERFETTO_DISPLAY_GROUP),
        "Phases",
    );

    push_perfetto_track_descriptor(
        &mut trace,
        PERFETTO_DISPLAY_STATE_TRACK,
        Some(PERFETTO_DISPLAY_GROUP),
        "Controller",
    );

    for event in events {
        push_perfetto_track_event(
            &mut trace,
            event.timestamp_ns,
            event.track_uuid,
            event.kind,
            event.name.as_deref(),
        );
    }

    Ok(trace)
}

fn unwrap_capture_origins(captures: &[Capture]) -> Result<Vec<u64>> {
    let first = captures
        .first()
        .context("cannot unwrap origins for an empty trace")?;

    let first_raw = first.origin_cycles.with_context(|| {
        format!(
            "trace session {} has no origin; capture a new trace with origin-enabled firmware",
            first.id,
        )
    })?;

    let mut origins = Vec::with_capacity(captures.len());

    let mut previous_raw = first_raw;
    let mut unwrapped = u64::from(first_raw);

    origins.push(unwrapped);

    for capture in &captures[1..] {
        let raw = capture.origin_cycles.with_context(|| {
            format!(
                "trace session {} has no origin; capture a new trace with origin-enabled firmware",
                capture.id,
            )
        })?;

        let delta = raw.wrapping_sub(previous_raw);

        unwrapped = unwrapped
            .checked_add(u64::from(delta))
            .context("trace timeline overflow while unwrapping cycle counter")?;

        origins.push(unwrapped);

        previous_raw = raw;
    }

    Ok(origins)
}

fn unwrap_cycle_near(raw: u32, reference_raw: u32, reference_unwrapped: u64) -> Result<u64> {
    let forward = raw.wrapping_sub(reference_raw);
    let backward = reference_raw.wrapping_sub(raw);

    if forward <= backward {
        reference_unwrapped
            .checked_add(u64::from(forward))
            .context("cycle counter overflow while unwrapping async timestamp")
    } else {
        reference_unwrapped
            .checked_sub(u64::from(backward))
            .context("cycle counter underflow while unwrapping async timestamp")
    }
}

fn push_perfetto_slice(
    events: &mut Vec<PerfettoTimedEvent>,
    track_uuid: u64,
    name: String,
    start_ns: u64,
    end_ns: u64,
    depth: u8,
) {
    events.push(PerfettoTimedEvent {
        kind: EventKind::Open,
        timestamp_ns: start_ns,
        track_uuid,
        name: Some(name),
        depth,
    });

    events.push(PerfettoTimedEvent {
        kind: EventKind::Close,
        timestamp_ns: end_ns,
        track_uuid,
        name: None,
        depth,
    });
}

fn perfetto_track_for_event(event: TraceEvent) -> u64 {
    match event {
        TraceEvent::Render
        | TraceEvent::Rebuild
        | TraceEvent::Layout
        | TraceEvent::Clear
        | TraceEvent::Paint
        | TraceEvent::Damage => PERFETTO_CPU_RENDER_TRACK,

        TraceEvent::TextRun
        | TraceEvent::Shape
        | TraceEvent::Glyphs
        | TraceEvent::LogicalShape
        | TraceEvent::VisualOrder
        | TraceEvent::Positioning => PERFETTO_CPU_TEXT_TRACK,

        TraceEvent::Present => PERFETTO_DISPLAY_PRESENT_TRACK,

        TraceEvent::PresentBusy => PERFETTO_DISPLAY_BUSY_TRACK,

        TraceEvent::DisplayPhase => PERFETTO_DISPLAY_PHASE_TRACK,
    }
}

fn cycles_to_ns_u64(cycles: u64, hz: u32) -> u64 {
    let nanoseconds = u128::from(cycles) * 1_000_000_000u128 / u128::from(hz);

    u64::try_from(nanoseconds).unwrap_or(u64::MAX)
}

fn default_perfetto_output_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("inkpaper-trace");

    input.with_file_name(format!("{stem}.perfetto-trace"))
}

fn push_perfetto_track_descriptor(
    trace: &mut Vec<u8>,
    uuid: u64,
    parent_uuid: Option<u64>,
    name: &str,
) {
    let mut descriptor = Vec::new();

    proto_varint_field(&mut descriptor, 1, uuid);
    proto_bytes_field(&mut descriptor, 2, name.as_bytes());

    if let Some(parent_uuid) = parent_uuid {
        proto_varint_field(&mut descriptor, 5, parent_uuid);
    }

    let mut packet = Vec::new();

    // TracePacket.track_descriptor = 60
    proto_bytes_field(&mut packet, 60, &descriptor);

    push_perfetto_packet(trace, &packet);
}

fn push_perfetto_track_event(
    trace: &mut Vec<u8>,
    timestamp_ns: u64,
    track_uuid: u64,
    kind: EventKind,
    name: Option<&str>,
) {
    let mut event = Vec::new();

    let event_type = match kind {
        EventKind::Open => 1,
        EventKind::Close => 2,
    };

    // TrackEvent.type = 9
    proto_varint_field(&mut event, 9, event_type);

    // TrackEvent.track_uuid = 11
    proto_varint_field(&mut event, 11, track_uuid);

    // TrackEvent.name = 23. Slice-end packets intentionally omit the name.
    if let Some(name) = name {
        proto_bytes_field(&mut event, 23, name.as_bytes());
    }

    let mut packet = Vec::new();

    // TracePacket.timestamp = 8, in nanoseconds by default.
    proto_varint_field(&mut packet, 8, timestamp_ns);

    // TrackEvent packets must belong to a non-zero packet sequence.
    // we do not use incremental/interened data yet, so one sequence is enough
    // for the entire InkPaper trace.
    //
    // TracePacket.trusted_packet_sequence_id = 10
    proto_varint_field(&mut packet, 10, PERFETTO_PACKET_SEQUENCE_ID);

    // TracePacket.track_event = 11
    proto_bytes_field(&mut packet, 11, &event);

    push_perfetto_packet(trace, &packet);
}

fn push_perfetto_packet(trace: &mut Vec<u8>, packet: &[u8]) {
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

    use crate::trace::{
        parse_captures,
        perfetto::{build_perfetto, unwrap_capture_origins},
    };

    #[test]
    fn perfetto_export_contains_tracks_and_slices() {
        let log = indoc! {r#"
            1.000 INFO trace/session id=12 frame=4 hz=240000000 origin=1000 spans=2 dropped=0 open=0
            1.001 INFO trace/span event=4 depth=1 start=120 cycles=480 arg=0
            1.002 INFO trace/span event=0 depth=0 start=0 cycles=720 arg=0

            2.000 INFO trace/session id=13 frame=4 hz=240000000 origin=4000 spans=2 dropped=0 open=0
            2.001 INFO trace/span event=13 depth=1 start=240 cycles=480 arg=0
            2.002 INFO trace/span event=12 depth=0 start=0 cycles=960 arg=0
        "#};

        let captures = parse_captures(log).unwrap();
        let trace = build_perfetto(&captures).unwrap();

        assert!(!trace.is_empty());
        assert!(contains_bytes(&trace, b"InkPaper"));
        assert!(contains_bytes(&trace, b"Frames"));
        assert!(contains_bytes(&trace, b"Render"));
        assert!(contains_bytes(&trace, b"Present"));
        assert!(contains_bytes(&trace, b"BUSY"));
        assert!(contains_bytes(&trace, b"paint"));
        assert!(contains_bytes(&trace, b"present_busy #0"));
        assert!(contains_bytes(&trace, b"frame 4"));
    }

    #[test]
    fn perfetto_origin_unwraps_u32_cycle_counter() {
        let log = indoc! {r#"
            1.000 INFO trace/session id=1 frame=1 hz=240000000 origin=4294967000 spans=1 dropped=0 open=0
            1.001 INFO trace/span event=0 depth=0 start=0 cycles=100 arg=0

            2.000 INFO trace/session id=2 frame=1 hz=240000000 origin=1000 spans=1 dropped=0 open=0
            2.001 INFO trace/span event=12 depth=0 start=0 cycles=100 arg=0
        "#};

        let captures = parse_captures(log).unwrap();
        let origins = unwrap_capture_origins(&captures).unwrap();

        let expected_delta = 1000u32.wrapping_sub(4_294_967_000u32);

        assert_eq!(origins.len(), 2);
        assert_eq!(origins[1] - origins[0], u64::from(expected_delta),);
    }

    fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }
}
