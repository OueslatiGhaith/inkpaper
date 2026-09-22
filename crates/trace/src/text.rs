use core::fmt::{self, Write};

use crate::{CallsiteDefinition, CapturedMetric, CapturedSpan, TraceCapture, ValueKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncodeError {
    StaleCapture,
    Write,
}

impl From<fmt::Error> for TextEncodeError {
    fn from(_: fmt::Error) -> Self {
        Self::Write
    }
}

pub fn write_text_capture<W>(capture: &TraceCapture, writer: &mut W) -> Result<(), TextEncodeError>
where
    W: Write,
{
    writeln!(
        writer,
        "trace/v3 capture id={} hz={} at={} spans={} overwritten={} metrics={} metric_dropped={}",
        capture.id(),
        capture.clock_hz(),
        capture.captured_at_cycles(),
        capture.spans(),
        capture.overwritten(),
        capture.metrics(),
        capture.metric_dropped(),
    )?;

    for index in 0..capture.spans() {
        let entry = capture.entry(index).ok_or(TextEncodeError::StaleCapture)?;

        if let Some(definition) = entry.definition() {
            write_definition(writer, definition)?;
        }

        write_span(writer, entry.span())?;
    }

    for index in 0..capture.metrics() {
        let metric = capture.metric(index).ok_or(TextEncodeError::StaleCapture)?;

        write_metric_definition(writer, metric)?;

        write_metric(writer, metric)?;
    }

    writeln!(writer, "trace/v3 end id={}", capture.id())?;

    Ok(())
}

fn write_definition<W>(writer: &mut W, definition: CallsiteDefinition) -> fmt::Result
where
    W: Write,
{
    let metadata = definition.metadata();

    write!(
        writer,
        "trace/v3 define id={} target=",
        definition.id().get(),
    )?;

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

fn write_span<W>(writer: &mut W, span: CapturedSpan) -> fmt::Result
where
    W: Write,
{
    let field_count = span.metadata().fields().len();

    write!(
        writer,
        "trace/v3 span id={} depth={} start={} cycles={} values={}",
        span.callsite_id().get(),
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

fn write_metric_definition<W>(writer: &mut W, metric: CapturedMetric) -> fmt::Result
where
    W: Write,
{
    let metadata = metric.metadata();

    write!(
        writer,
        "trace/v3 metric_define id={} target=",
        metric.id().get(),
    )?;

    write_component(writer, metadata.target())?;

    write!(writer, " name=")?;

    write_component(writer, metadata.name())?;

    write!(writer, " kind={} unit=", metric.kind().as_str())?;

    write_component(writer, metric.unit())?;

    writeln!(writer)
}

fn write_metric<W>(writer: &mut W, metric: CapturedMetric) -> fmt::Result
where
    W: Write,
{
    writeln!(
        writer,
        "trace/v3 metric id={} count={} sum={} max={}",
        metric.id().get(),
        metric.count(),
        metric.sum(),
        metric.max(),
    )
}

fn write_component<W>(writer: &mut W, value: &str) -> fmt::Result
where
    W: Write,
{
    write!(writer, "{}:{}", value.len(), value)
}
