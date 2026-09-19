use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use inkpaper_trace::TraceEvent;
use serde::Serialize;

use crate::trace::{
    Capture, EventKind, TraceFrameKey, parse_captures, trace_frame_key, trace_frame_name,
};

#[derive(Serialize)]
struct Frame {
    name: String,
}

#[derive(Serialize)]
struct Shared {
    frames: Vec<Frame>,
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

#[derive(Debug, Clone, Copy)]
struct TimedEvent {
    kind: EventKind,
    at: f64,
    frame: usize,
    depth: u8,
}

fn default_speedscope_output_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("inkpaper-trace");

    input.with_file_name(format!("{stem}.speedscope.json"))
}

pub fn convert_speedscope(input: &Path, output: Option<&Path>) -> Result<PathBuf> {
    let log = std::fs::read_to_string(input)
        .with_context(|| format!("failed to read {}", input.display()))?;

    let captures = parse_captures(&log)?;

    let speedscope = build_speedscope(&captures)?;

    let output = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_speedscope_output_path(input));

    let json = serde_json::to_string_pretty(&speedscope)
        .context("failed to serialize Speedscope profile")?;

    std::fs::write(&output, json)
        .with_context(|| format!("failed to write {}", output.display()))?;

    Ok(output)
}

fn build_speedscope(captures: &[Capture]) -> Result<SpeedscopeFile> {
    let frame_keys = build_frame_keys(captures);

    let frames = frame_keys
        .iter()
        .copied()
        .map(|key| Frame {
            name: trace_frame_name(key),
        })
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

            let key = trace_frame_key(span);

            let frame = frame_keys
                .iter()
                .position(|candidate| *candidate == key)
                .with_context(|| format!("trace frame {:?} is missing from shared frames", key))?;

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
            name: format!(
                "InkPaper frame {} {}",
                capture.frame_id,
                capture_phase(capture),
            ),
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

fn build_frame_keys(captures: &[Capture]) -> Vec<TraceFrameKey> {
    let mut keys = Vec::new();

    for event in TraceEvent::ALL.iter().copied() {
        if event == TraceEvent::PresentBusy {
            continue;
        }

        keys.push(TraceFrameKey { event, arg: None });
    }

    for capture in captures {
        for span in &capture.spans {
            let key = trace_frame_key(span);

            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }

    keys
}

fn capture_phase(capture: &Capture) -> &'static str {
    if capture
        .spans
        .iter()
        .any(|span| span.depth == 0 && span.event == TraceEvent::Present)
    {
        return "present";
    }

    if capture
        .spans
        .iter()
        .any(|span| span.depth == 0 && span.event == TraceEvent::Render)
    {
        return "render";
    }

    "trace"
}

fn cycles_to_us(cycles: u32, hz: u32) -> f64 {
    cycles_to_us_u64(u64::from(cycles), hz)
}

fn cycles_to_us_u64(cycles: u64, hz: u32) -> f64 {
    cycles as f64 * 1_000_000.0 / f64::from(hz)
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use crate::trace::{parse_captures, speedscope::build_speedscope};

    #[test]
    fn parses_render_and_present_sessions() {
        let log = indoc! {r#"
            1.000 INFO trace/session id=12 frame=4 hz=240000000 spans=3 dropped=0 open=0
            1.001 INFO trace/span event=7 depth=2 start=240 cycles=480 arg=12
            1.002 INFO trace/span event=8 depth=2 start=720 cycles=240 arg=18
            1.003 INFO trace/span event=0 depth=0 start=120 cycles=960 arg=0

            2.000 INFO trace/session id=13 frame=4 hz=240000000 spans=3 dropped=0 open=0
            2.001 INFO trace/span event=13 depth=1 start=480 cycles=240 arg=0
            2.002 INFO trace/span event=13 depth=1 start=960 cycles=480 arg=1
            2.003 INFO trace/span event=12 depth=0 start=120 cycles=1680 arg=0
        "#};

        let captures = parse_captures(log).unwrap();

        assert_eq!(captures.len(), 2);
        assert_eq!(captures[0].frame_id, 4);
        assert_eq!(captures[1].frame_id, 4);

        let file = build_speedscope(&captures).unwrap();

        assert_eq!(file.profiles.len(), 2);
        assert_eq!(file.profiles[0].name, "InkPaper frame 4 render");
        assert_eq!(file.profiles[1].name, "InkPaper frame 4 present");

        let serialized = serde_json::to_string(&file).unwrap();

        assert!(serialized.contains("\"name\":\"present\""));
        assert!(serialized.contains("\"name\":\"present_busy #0\""));
        assert!(serialized.contains("\"name\":\"present_busy #1\""));
    }
}
