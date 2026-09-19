use anyhow::{Context, Result, bail};
use inkpaper_trace::{DisplayPhase, TraceEvent};

mod perfetto;
mod speedscope;

pub use perfetto::convert_perfetto;
pub use speedscope::convert_speedscope;

#[derive(Debug)]
struct Capture {
    id: u32,
    frame_id: u32,
    hz: u32,
    origin_cycles: Option<u32>,
    expected_spans: usize,
    dropped: u32,
    open_spans: u8,
    spans: Vec<Span>,
    async_spans: Vec<AsyncSpan>,
}

#[derive(Debug, Clone, Copy)]
struct Span {
    event: TraceEvent,
    depth: u8,
    start_cycles: u32,
    duration_cycles: u32,
    arg: u32,
}

#[derive(Debug, Clone, Copy)]
struct AsyncSpan {
    event: TraceEvent,
    id: u32,
    start_cycles: u32,
    duration_cycles: u32,
    arg: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TraceFrameKey {
    event: TraceEvent,
    arg: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventKind {
    Close,
    Open,
}

fn parse_captures(log: &str) -> Result<Vec<Capture>> {
    let mut captures = Vec::new();
    let mut current = None;

    for line in log.lines() {
        if let Some((_, payload)) = line.split_once("trace/session ") {
            if let Some(capture) = current.take() {
                captures.push(capture);
            }

            let id = u32_field(payload, "id")?;
            let frame_id = optional_u32_field(payload, "frame")?.unwrap_or(id);

            current = Some(Capture {
                id,
                frame_id,
                hz: u32_field(payload, "hz")?,
                origin_cycles: optional_u32_field(payload, "origin")?,
                expected_spans: usize_field(payload, "spans")?,
                dropped: u32_field(payload, "dropped")?,
                open_spans: u8_field(payload, "open")?,
                spans: Vec::new(),
                async_spans: Vec::new(),
            });

            continue;
        }

        if let Some((_, payload)) = line.split_once("trace/async ") {
            let Some(capture) = current.as_mut() else {
                continue;
            };

            let event_id = u8_field(payload, "event")?;

            let event = TraceEvent::from_id(event_id)
                .with_context(|| format!("unknown async trace event id {event_id}"))?;

            capture.async_spans.push(AsyncSpan {
                event,
                id: u32_field(payload, "id")?,
                start_cycles: u32_field(payload, "start")?,
                duration_cycles: u32_field(payload, "cycles")?,
                arg: u32_field(payload, "arg")?,
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

fn trace_frame_key(span: &Span) -> TraceFrameKey {
    let arg = match span.event {
        TraceEvent::PresentBusy | TraceEvent::DisplayPhase => Some(span.arg),
        _ => None,
    };

    TraceFrameKey {
        event: span.event,
        arg,
    }
}

fn trace_frame_name(key: TraceFrameKey) -> String {
    trace_event_name(key.event, key.arg)
}

fn trace_event_name(event: TraceEvent, arg: Option<u32>) -> String {
    match (event, arg) {
        (TraceEvent::PresentBusy, Some(index)) => format!("{} #{}", event.name(), index),
        (TraceEvent::DisplayPhase, Some(id)) => {
            let phase = u8::try_from(id).ok().and_then(DisplayPhase::from_id);

            match phase {
                Some(phase) => phase.name().to_owned(),
                None => format!("display_phase #{}", id),
            }
        }
        _ => event.name().to_owned(),
    }
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

fn optional_field<'a>(payload: &'a str, name: &str) -> Option<&'a str> {
    payload.split_whitespace().find_map(|part| {
        let (key, value) = part.split_once('=')?;

        (key == name).then(|| value.trim_end_matches(','))
    })
}

fn optional_u32_field(payload: &str, name: &str) -> Result<Option<u32>> {
    let Some(value) = optional_field(payload, name) else {
        return Ok(None);
    };

    Ok(Some(value.parse().with_context(|| {
        format!("invalid `{name}` in trace line: {payload}")
    })?))
}

fn usize_field(payload: &str, name: &str) -> Result<usize> {
    field(payload, name)?
        .parse()
        .with_context(|| format!("invalid `{name}` in trace line: {payload}"))
}
