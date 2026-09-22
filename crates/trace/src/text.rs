use core::{
    cell::RefCell,
    fmt::{self, Write},
    ptr,
};

use critical_section::Mutex;

use crate::{
    CapturedMetric, CapturedSpan, METRIC_CAPACITY, Metadata, MetricKind, TRACE_CAPACITY,
    TraceCapture, ValueKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncodeError {
    StaleCapture,
    DefinitionCapacity,
    Write,
}

impl From<fmt::Error> for TextEncodeError {
    fn from(_: fmt::Error) -> Self {
        Self::Write
    }
}

struct DefinitionTable<const N: usize> {
    entries: [Option<&'static Metadata>; N],
    len: usize,
}

impl<const N: usize> DefinitionTable<N> {
    const fn new() -> Self {
        Self {
            entries: [None; N],
            len: 0,
        }
    }

    fn id_for(&mut self, metadata: &'static Metadata) -> Result<(u16, bool), TextEncodeError> {
        for index in 0..self.len {
            let Some(existing) = self.entries[index] else {
                continue;
            };

            if ptr::eq(existing, metadata) {
                let id = u16::try_from(index).map_err(|_| TextEncodeError::DefinitionCapacity)?;

                return Ok((id, false));
            }
        }

        if self.len >= N {
            return Err(TextEncodeError::DefinitionCapacity);
        }

        let id = u16::try_from(self.len).map_err(|_| TextEncodeError::DefinitionCapacity)?;

        self.entries[self.len] = Some(metadata);
        self.len += 1;

        Ok((id, true))
    }
}

struct TextDefinitions {
    spans: DefinitionTable<TRACE_CAPACITY>,
    metrics: DefinitionTable<METRIC_CAPACITY>,
}

impl TextDefinitions {
    const fn new() -> Self {
        Self {
            spans: DefinitionTable::new(),
            metrics: DefinitionTable::new(),
        }
    }
}

static DEFINITIONS: Mutex<RefCell<TextDefinitions>> =
    Mutex::new(RefCell::new(TextDefinitions::new()));

pub(crate) fn reset_definitions() {
    critical_section::with(|cs| {
        *DEFINITIONS.borrow(cs).borrow_mut() = TextDefinitions::new();
    });
}

fn span_definition_id(metadata: &'static Metadata) -> Result<(u16, bool), TextEncodeError> {
    critical_section::with(|cs| DEFINITIONS.borrow(cs).borrow_mut().spans.id_for(metadata))
}

fn metric_definition_id(metadata: &'static Metadata) -> Result<(u16, bool), TextEncodeError> {
    critical_section::with(|cs| DEFINITIONS.borrow(cs).borrow_mut().metrics.id_for(metadata))
}

pub fn write_text_capture<W>(capture: &TraceCapture, writer: &mut W) -> Result<(), TextEncodeError>
where
    W: Write,
{
    writeln!(
        writer,
        "trace/v4 capture id={} hz={} at={} spans={} overwritten={} metrics={} metric_dropped={}",
        capture.id(),
        capture.clock_hz(),
        capture.captured_at_cycles(),
        capture.spans(),
        capture.overwritten(),
        capture.metrics(),
        capture.metric_dropped(),
    )?;

    for index in 0..capture.spans() {
        let span = capture.span(index).ok_or(TextEncodeError::StaleCapture)?;

        let metadata = span.metadata();

        let (id, newly_defined) = span_definition_id(metadata)?;

        if newly_defined {
            write_definition(writer, id, metadata)?;
        }

        write_span(writer, id, span)?;
    }

    for index in 0..capture.metrics() {
        let metric = capture.metric(index).ok_or(TextEncodeError::StaleCapture)?;

        let metadata = metric.metadata();

        let (id, newly_defined) = metric_definition_id(metadata)?;

        if newly_defined {
            write_metric_definition(writer, id, metric)?;
        }

        write_metric(writer, id, metric)?;
    }

    writeln!(writer, "trace/v4 end id={}", capture.id())?;

    Ok(())
}

fn write_definition<W>(writer: &mut W, id: u16, metadata: &'static Metadata) -> fmt::Result
where
    W: Write,
{
    write!(writer, "trace/v4 define id={} target=", id)?;

    write_component(writer, metadata.target())?;

    write!(writer, " name=")?;

    write_component(writer, metadata.name())?;

    write!(writer, " fields={}", metadata.fields().len())?;

    for field in metadata.fields() {
        write!(writer, " ")?;

        write_component(writer, field.name())?;
    }

    writeln!(writer)
}

fn write_span<W>(writer: &mut W, id: u16, span: CapturedSpan) -> fmt::Result
where
    W: Write,
{
    let field_count = span.metadata().fields().len();

    write!(
        writer,
        "trace/v4 span id={} kind={} depth={} start={} cycles={} values={}",
        id,
        span.kind().as_str(),
        span.depth(),
        span.start_cycles(),
        span.duration_cycles(),
        field_count,
    )?;

    for index in 0..field_count {
        let field = span
            .field(index)
            .expect("trace field metadata and values must agree");

        let value = field.value();

        match value.kind() {
            ValueKind::Unsigned => {
                write!(writer, " u:{}", value.raw())?;
            }

            ValueKind::Signed => {
                write!(writer, " i:{}", value.raw() as i32)?;
            }

            ValueKind::Bool => {
                write!(writer, " b:{}", u8::from(value.raw() != 0))?;
            }
        }
    }

    writeln!(writer)
}

fn write_metric_definition<W>(writer: &mut W, id: u16, metric: CapturedMetric) -> fmt::Result
where
    W: Write,
{
    let metadata = metric.metadata();

    write!(writer, "trace/v4 metric_define id={} target=", id)?;

    write_component(writer, metadata.target())?;

    write!(writer, " name=")?;

    write_component(writer, metadata.name())?;

    write!(writer, " kind={} unit=", metric.kind().as_str())?;

    write_component(writer, metric.unit())?;

    writeln!(writer)
}

fn write_metric<W>(writer: &mut W, id: u16, metric: CapturedMetric) -> fmt::Result
where
    W: Write,
{
    match metric.kind() {
        MetricKind::Gauge => {
            writeln!(writer, "trace/v4 metric id={} value={}", id, metric.sum())
        }

        MetricKind::Counter | MetricKind::Distribution => {
            writeln!(
                writer,
                "trace/v4 metric id={} count={} sum={} max={}",
                id,
                metric.count(),
                metric.sum(),
                metric.max(),
            )
        }
    }
}

fn write_component<W>(writer: &mut W, value: &str) -> fmt::Result
where
    W: Write,
{
    write!(writer, "{}:{}", value.len(), value)
}
