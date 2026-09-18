use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use inkpaper_trace::TraceEvent;
use serde::Serialize;

#[derive(Debug)]
struct Capture {
    id: u32,
    hz: u32,
    expected_spans: usize,
    dropped: u32,
    open_spans: u8,
    spans: Vec<Span>,
}

#[derive(Debug, Clone, Copy)]
struct Span {
    event: TraceEvent,
    depth: u8,
    start_cycles: u32,
    duration_cycles: u32,
    arg: u32,
}

#[derive(Serialize)]
struct SpeedscopeFile {
    #[serde(rename = "$schema")]
    schema: &'static str,
    shared: Shared,
    profiles: Vec<Profile>,
    #[serde(rename = "activeProfileIndex")]
    active_profile_index: usize,
    exporter: &'static str,
}

#[derive(Serialize)]
struct Shared {
    frames: Vec<Frame>,
}

#[derive(Serialize)]
struct Frame {
    name: &'static str,
}

#[derive(Serialize)]
struct Profile {
    #[serde(rename = "type")]
    profile_type: &'static str,
    name: String,
    unit: &'static str,
    #[serde(rename = "startValue")]
    start_value: f64,
    #[serde(rename = "endValue")]
    end_value: f64,
    events: Vec<SpeedEvent>,
}

#[derive(Serialize)]
struct SpeedEvent {
    #[serde(rename = "type")]
    kind: &'static str,
    at: f64,
    frame: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventKind {
    Close,
    Open,
}

#[derive(Debug, Clone, Copy)]
struct TimedEvent {
    kind: EventKind,
    at: f64,
    frame: usize,
    depth: u8,
}

pub fn convert(input: &Path, output: Option<&Path>) -> Result<PathBuf> {
    let log =
        fs::read_to_string(input).with_context(|| format!("failed to read {}", input.display()))?;

    let captures = parse_captures(&log)?;

    let speedscope = build_speedscope(&captures)?;

    let output = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_output_path(input));

    let json = serde_json::to_string_pretty(&speedscope)
        .context("failed to serialize Speedscope profile")?;

    fs::write(&output, json).with_context(|| format!("failed to write {}", output.display()))?;

    Ok(output)
}

fn parse_captures(log: &str) -> Result<Vec<Capture>> {
    let mut captures = Vec::new();
    let mut current = None;

    for line in log.lines() {
        if let Some((_, payload)) = line.split_once("trace/session ") {
            if let Some(capture) = current.take() {
                captures.push(capture);
            }

            current = Some(Capture {
                id: u32_field(payload, "id")?,
                hz: u32_field(payload, "hz")?,
                expected_spans: usize_field(payload, "spans")?,
                dropped: u32_field(payload, "dropped")?,
                open_spans: u8_field(payload, "open")?,
                spans: Vec::new(),
            });

            continue;
        }

        let Some((_, payload)) = line.split_once("trace/span ") else {
            continue;
        };

        let Some(capture) = current.as_mut() else {
            continue;
        };

        let event_id = u8_field(payload, "event")?;

        let event = TraceEvent::from_id(event_id)
            .with_context(|| format!("unknown trace event id {event_id}"))?;

        capture.spans.push(Span {
            event,
            depth: u8_field(payload, "depth")?,
            start_cycles: u32_field(payload, "start")?,
            duration_cycles: u32_field(payload, "cycles")?,
            arg: u32_field(payload, "arg")?,
        });
    }

    if let Some(capture) = current {
        captures.push(capture);
    }

    if captures.is_empty() {
        bail!("no InkPaper trace sessions found in log");
    }

    for capture in &captures {
        if capture.hz == 0 {
            bail!("trace session {} has a zero clock rate", capture.id);
        }

        if capture.expected_spans != capture.spans.len() {
            bail!(
                "trace session {} is incomplete: header says {} spans but log contains {}",
                capture.id,
                capture.expected_spans,
                capture.spans.len(),
            );
        }

        if capture.dropped != 0 {
            bail!(
                "trace session {} overflowed: {} spans were dropped",
                capture.id,
                capture.dropped,
            );
        }

        if capture.open_spans != 0 {
            bail!(
                "trace session {} ended with {} open spans",
                capture.id,
                capture.open_spans,
            );
        }
    }

    Ok(captures)
}

fn build_speedscope(captures: &[Capture]) -> Result<SpeedscopeFile> {
    let frames = TraceEvent::ALL
        .into_iter()
        .map(|event| Frame { name: event.name() })
        .collect();

    let mut profiles = Vec::with_capacity(captures.len());

    for capture in captures {
        let mut timed = Vec::with_capacity(capture.spans.len() * 2);
        let mut end_value = 0.0f64;

        for span in &capture.spans {
            let start = cycles_to_us(span.start_cycles, capture.hz);

            let end_cycles = u64::from(span.start_cycles) + u64::from(span.duration_cycles);

            let end = cycles_to_us_u64(end_cycles, capture.hz);

            end_value = end_value.max(end);

            let frame = usize::from(span.event.id());

            timed.push(TimedEvent {
                kind: EventKind::Open,
                at: start,
                frame,
                depth: span.depth,
            });

            timed.push(TimedEvent {
                kind: EventKind::Close,
                at: end,
                frame,
                depth: span.depth,
            });

            let _ = span.arg;
        }

        timed.sort_by(|a, b| {
            a.at.total_cmp(&b.at).then_with(|| match (a.kind, b.kind) {
                (EventKind::Close, EventKind::Open) => core::cmp::Ordering::Less,
                (EventKind::Open, EventKind::Close) => core::cmp::Ordering::Greater,
                (EventKind::Open, EventKind::Open) => a.depth.cmp(&b.depth),
                (EventKind::Close, EventKind::Close) => b.depth.cmp(&a.depth),
            })
        });

        let events = timed
            .into_iter()
            .map(|event| SpeedEvent {
                kind: match event.kind {
                    EventKind::Open => "O",
                    EventKind::Close => "C",
                },
                at: event.at,
                frame: event.frame,
            })
            .collect();

        profiles.push(Profile {
            profile_type: "evented",
            name: format!("InkPaper render {}", capture.id),
            unit: "microseconds",
            start_value: 0.0,
            end_value,
            events,
        });
    }

    Ok(SpeedscopeFile {
        schema: "https://www.speedscope.app/file-format-schema.json",
        shared: Shared { frames },
        active_profile_index: profiles.len().saturating_sub(1),
        profiles,
        exporter: "InkPaper xtask",
    })
}

fn cycles_to_us(cycles: u32, hz: u32) -> f64 {
    cycles_to_us_u64(u64::from(cycles), hz)
}

fn cycles_to_us_u64(cycles: u64, hz: u32) -> f64 {
    cycles as f64 * 1_000_000.0 / f64::from(hz)
}

fn default_output_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("inkpaper-trace");

    input.with_file_name(format!("{stem}.speedscope.json"))
}

fn field<'a>(payload: &'a str, name: &str) -> Result<&'a str> {
    payload
        .split_whitespace()
        .find_map(|part| {
            let (key, value) = part.split_once('=')?;

            if key == name {
                Some(value.trim_end_matches(','))
            } else {
                None
            }
        })
        .with_context(|| format!("missing `{name}` in trace line: {payload}"))
}

fn u8_field(payload: &str, name: &str) -> Result<u8> {
    field(payload, name)?
        .parse()
        .with_context(|| format!("invalid `{name}` in trace line: {payload}"))
}

fn u32_field(payload: &str, name: &str) -> Result<u32> {
    field(payload, name)?
        .parse()
        .with_context(|| format!("invalid `{name}` in trace line: {payload}"))
}

fn usize_field(payload: &str, name: &str) -> Result<usize> {
    field(payload, name)?
        .parse()
        .with_context(|| format!("invalid `{name}` in trace line: {payload}"))
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use crate::trace::{build_speedscope, parse_captures};

    #[test]
    fn parses_completed_trace_and_builds_evented_profile() {
        let log = indoc! {r#"
            1.000 INFO  trace/session id=12 hz=240000000 spans=3 dropped=0 open=0
            1.001 INFO  trace/span event=7 depth=2 start=240 cycles=480 arg=12
            1.002 INFO  trace/span event=8 depth=2 start=720 cycles=240 arg=18
            1.003 INFO  trace/span event=6 depth=1 start=120 cycles=960 arg=12
        "#};

        let captures = parse_captures(log).unwrap();

        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].id, 12);
        assert_eq!(captures[0].spans.len(), 3);

        let file = build_speedscope(&captures).unwrap();

        assert_eq!(file.profiles.len(), 1);
        assert_eq!(file.profiles[0].events.len(), 6);

        let serialized = serde_json::to_string(&file).unwrap();

        assert!(serialized.contains("\"type\":\"evented\""));
        assert!(serialized.contains("\"name\":\"shape\""));
        assert!(serialized.contains("\"name\":\"glyphs\""));
    }
}
