use core::fmt::{self, Write};

use crate::{CallsiteDefinition, CapturedSpan, TraceCapture, ValueKind};

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

pub fn write_text_capture<W>(capture: TraceCapture, writer: &mut W) -> Result<(), TextEncodeError>
where
    W: Write,
{
    writeln!(
        writer,
        "trace/v2 capture session={} hz={} origin={} spans={} dropped={} open={}",
        capture.session_id(),
        capture.clock_hz(),
        capture.origin_cycles(),
        capture.spans(),
        capture.dropped(),
        capture.open_spans(),
    )?;

    for index in 0..capture.spans() {
        let entry = capture.entry(index).ok_or(TextEncodeError::StaleCapture)?;

        if let Some(definition) = entry.definition() {
            write_definition(writer, definition)?;
        }

        write_span(writer, entry.span())?;
    }

    writeln!(writer, "trace/v2 end session={}", capture.session_id())?;

    Ok(())
}

fn write_definition<W>(writer: &mut W, definition: CallsiteDefinition) -> fmt::Result
where
    W: Write,
{
    let metadata = definition.metadata();

    write!(
        writer,
        "trace/v2 define id={} target=",
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
        "trace/v2 span id={} depth={} start={} cycles={} values={}",
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
            ValueKind::Unsigned => write!(writer, " u:{}", value.raw())?,
            ValueKind::Signed => write!(writer, " i:{}", value.raw() as i32)?,
            ValueKind::Bool => write!(writer, " b:{}", u8::from(value.raw() != 0))?,
        }
    }

    writeln!(writer)
}

fn write_component<W>(writer: &mut W, value: &str) -> fmt::Result
where
    W: Write,
{
    write!(writer, "{}:{}", value.len(), value)
}
