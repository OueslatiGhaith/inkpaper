use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, anyhow, bail};

mod perfetto;

pub use perfetto::convert_perfetto;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Capture {
    session_id: u32,
    clock_hz: u32,
    origin_cycles: u32,

    expected_spans: usize,
    dropped: u32,
    open_spans: u8,

    callsites: BTreeMap<u16, Callsite>,
    spans: Vec<Span>,

    metrics_declared: bool,
    expected_metrics: usize,
    metric_dropped: u32,
    metric_definitions: BTreeMap<u16, MetricDefinition>,
    metrics: BTreeMap<u16, Metric>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Callsite {
    id: u16,
    target: String,
    name: String,
    fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Span {
    callsite_id: u16,
    depth: u8,
    start_cycles: u32,
    duration_cycles: u32,
    values: Vec<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetricKind {
    Counter,
    Distribution,
}

impl MetricKind {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "counter" => Ok(Self::Counter),
            "distribution" => Ok(Self::Distribution),
            _ => bail!("unknown metric kind `{value}`"),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Counter => "counter",
            Self::Distribution => "distribution",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MetricDefinition {
    id: u16,
    target: String,
    name: String,
    kind: MetricKind,
    unit: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Metric {
    id: u16,
    count: u32,
    sum: u64,
    max: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Value {
    Unsigned(u32),
    Signed(i32),
    Bool(bool),
}

fn parse_captures(log: &str) -> Result<Vec<Capture>> {
    let mut captures = Vec::new();

    let mut current: Option<Capture> = None;

    let mut session_ids = BTreeSet::new();

    for (line_index, line) in log.lines().enumerate() {
        let Some((_, record)) = line.split_once("trace/v2 ") else {
            continue;
        };

        let line_number = line_index + 1;

        if let Some(payload) = record.strip_prefix("capture ") {
            if let Some(open) = current.as_ref() {
                bail!(
                    "trace session {} did not end before a new capture started on log line {}",
                    open.session_id,
                    line_number,
                );
            }

            let capture = parse_capture_header(payload)
                .with_context(|| format!("invalid trace capture on log line {line_number}"))?;

            if !session_ids.insert(capture.session_id) {
                bail!(
                    "duplicate trace session {} on log line {}",
                    capture.session_id,
                    line_number,
                );
            }

            current = Some(capture);

            continue;
        }

        if let Some(payload) = record.strip_prefix("define ") {
            let capture = current.as_mut().with_context(|| {
                format!("trace definition outside a capture on log line {line_number}")
            })?;

            parse_definition(payload, capture)
                .with_context(|| format!("invalid trace definition on log line {line_number}"))?;

            continue;
        }

        if let Some(payload) = record.strip_prefix("span ") {
            let capture = current.as_mut().with_context(|| {
                format!("trace span outside a capture on log line {line_number}")
            })?;

            parse_span(payload, capture)
                .with_context(|| format!("invalid trace span on log line {line_number}"))?;

            continue;
        }

        if let Some(payload) = record.strip_prefix("metrics ") {
            let capture = current.as_mut().with_context(|| {
                format!("trace metric summary outside a capture on log line {line_number}")
            })?;

            parse_metrics_header(payload, capture).with_context(|| {
                format!("invalid trace metric summary on log line {line_number}")
            })?;

            continue;
        }

        if let Some(payload) = record.strip_prefix("metric_define ") {
            let capture = current.as_mut().with_context(|| {
                format!("trace metric definition outside a capture on log line {line_number}")
            })?;

            parse_metric_definition(payload, capture).with_context(|| {
                format!("invalid trace metric definition on log line {line_number}")
            })?;

            continue;
        }

        if let Some(payload) = record.strip_prefix("metric ") {
            let capture = current.as_mut().with_context(|| {
                format!("trace metric outside a capture on log line {line_number}")
            })?;

            parse_metric(payload, capture)
                .with_context(|| format!("invalid trace metric on log line {line_number}"))?;

            continue;
        }

        if let Some(payload) = record.strip_prefix("end ") {
            let capture = current.take().with_context(|| {
                format!("trace end outside a capture on log line {line_number}")
            })?;

            let end_session = parse_end(payload)
                .with_context(|| format!("invalid trace end on log line {line_number}"))?;

            if end_session != capture.session_id {
                bail!(
                    "trace session {} ended as session {} on log line {}",
                    capture.session_id,
                    end_session,
                    line_number,
                );
            }

            validate_capture(&capture)?;

            captures.push(capture);

            continue;
        }

        if let Some(payload) = record.strip_prefix("encode_error ") {
            bail!("firmware reported trace encoding failure on log line {line_number}: {payload}");
        }

        bail!("unknown trace/v2 record on log line {line_number}: {record}");
    }

    if let Some(capture) = current {
        bail!(
            "trace session {} is incomplete: missing matching trace/v2 end",
            capture.session_id,
        );
    }

    if captures.is_empty() {
        bail!("no trace/v2 captures found in log");
    }

    Ok(captures)
}

fn parse_capture_header(payload: &str) -> Result<Capture> {
    let mut parser = LineParser::new(payload);

    let session_id = parser.number("session=")?;
    let clock_hz = parser.number("hz=")?;
    let origin_cycles = parser.number("origin=")?;
    let expected_spans = parser.number("spans=")?;
    let dropped = parser.number("dropped=")?;
    let open_spans = parser.number("open=")?;

    parser.finish()?;

    if clock_hz == 0 {
        bail!("trace clock frequency must be non-zero");
    }

    Ok(Capture {
        session_id,
        clock_hz,
        origin_cycles,
        expected_spans,
        dropped,
        open_spans,
        callsites: BTreeMap::new(),
        spans: Vec::with_capacity(expected_spans),
        metrics_declared: false,
        expected_metrics: 0,
        metric_dropped: 0,
        metric_definitions: BTreeMap::new(),
        metrics: BTreeMap::new(),
    })
}

fn parse_metrics_header(payload: &str, capture: &mut Capture) -> Result<()> {
    if capture.metrics_declared {
        bail!(
            "trace session {} declares metrics more than once",
            capture.session_id,
        );
    }

    let mut parser = LineParser::new(payload);

    let expected_metrics = parser.number("count=")?;

    let metric_dropped = parser.number("dropped=")?;

    parser.finish()?;

    capture.metrics_declared = true;

    capture.expected_metrics = expected_metrics;

    capture.metric_dropped = metric_dropped;

    Ok(())
}

fn parse_metric_definition(payload: &str, capture: &mut Capture) -> Result<()> {
    if !capture.metrics_declared {
        bail!("metric definition appeared before trace/v2 metrics summary");
    }

    let mut parser = LineParser::new(payload);

    let id = parser.number("id=")?;

    let target = parser.component("target=")?;

    let name = parser.component("name=")?;

    let kind_text = parser.token("kind=")?;

    let kind = MetricKind::parse(kind_text)?;

    let unit = parser.component("unit=")?;

    parser.finish()?;

    if capture.metric_definitions.contains_key(&id) {
        bail!(
            "trace session {} defines metric {} more than once",
            capture.session_id,
            id,
        );
    }

    capture.metric_definitions.insert(
        id,
        MetricDefinition {
            id,
            target,
            name,
            kind,
            unit,
        },
    );

    Ok(())
}

fn parse_metric(payload: &str, capture: &mut Capture) -> Result<()> {
    if !capture.metrics_declared {
        bail!("metric appeared before trace/v2 metrics summary");
    }

    let mut parser = LineParser::new(payload);

    let id = parser.number("id=")?;
    let count = parser.number("count=")?;
    let sum = parser.number("sum=")?;
    let max = parser.number("max=")?;

    parser.finish()?;

    if !capture.metric_definitions.contains_key(&id) {
        bail!(
            "trace session {} uses undefined metric {}",
            capture.session_id,
            id,
        );
    }

    if capture.metrics.contains_key(&id) {
        bail!(
            "trace session {} records metric {} more than once",
            capture.session_id,
            id,
        );
    }

    if count == 0 {
        bail!("metric {} has zero observations", id);
    }

    if u64::from(max) > sum {
        bail!("metric {} has max {} greater than sum {}", id, max, sum);
    }

    capture.metrics.insert(
        id,
        Metric {
            id,
            count,
            sum,
            max,
        },
    );

    Ok(())
}

fn parse_definition(payload: &str, capture: &mut Capture) -> Result<()> {
    let mut parser = LineParser::new(payload);

    let id = parser.number("id=")?;
    let target = parser.component("target=")?;
    let name = parser.component("name=")?;
    let field_count: usize = parser.number("fields=")?;

    let mut fields = Vec::with_capacity(field_count);

    for _ in 0..field_count {
        fields.push(parser.component("")?);
    }

    parser.finish()?;

    if capture.callsites.contains_key(&id) {
        bail!(
            "trace session {} defines callsite {} more than once",
            capture.session_id,
            id,
        );
    }

    capture.callsites.insert(
        id,
        Callsite {
            id,
            target,
            name,
            fields,
        },
    );

    Ok(())
}

fn parse_span(payload: &str, capture: &mut Capture) -> Result<()> {
    let mut parser = LineParser::new(payload);

    let callsite_id = parser.number("id=")?;
    let depth = parser.number("depth=")?;
    let start_cycles = parser.number("start=")?;
    let duration_cycles = parser.number("cycles=")?;
    let value_count: usize = parser.number("values=")?;

    let callsite = capture.callsites.get(&callsite_id).with_context(|| {
        format!(
            "trace session {} uses undefined callsite {}",
            capture.session_id, callsite_id,
        )
    })?;

    if callsite.fields.len() != value_count {
        bail!(
            "callsite {} declares {} fields but span contains {} values",
            callsite_id,
            callsite.fields.len(),
            value_count,
        );
    }

    let mut values = Vec::with_capacity(value_count);

    for _ in 0..value_count {
        values.push(parser.value()?);
    }

    parser.finish()?;

    if capture.spans.len() >= capture.expected_spans {
        bail!(
            "trace session {} contains more spans than its header declared",
            capture.session_id,
        );
    }

    capture.spans.push(Span {
        callsite_id,
        depth,
        start_cycles,
        duration_cycles,
        values,
    });

    Ok(())
}

fn parse_end(payload: &str) -> Result<u32> {
    let mut parser = LineParser::new(payload);

    let session_id = parser.number("session=")?;

    parser.finish()?;

    Ok(session_id)
}

fn validate_capture(capture: &Capture) -> Result<()> {
    if capture.expected_spans != capture.spans.len() {
        bail!(
            "trace session {} is incomplete: header declares {} spans but {} were parsed",
            capture.session_id,
            capture.expected_spans,
            capture.spans.len(),
        );
    }

    if capture.dropped != 0 {
        bail!(
            "trace session {} overflowed: {} spans were dropped",
            capture.session_id,
            capture.dropped,
        );
    }

    if capture.open_spans != 0 {
        bail!(
            "trace session {} ended with {} open spans",
            capture.session_id,
            capture.open_spans,
        );
    }

    if !capture.metrics_declared {
        if !capture.metric_definitions.is_empty() || !capture.metrics.is_empty() {
            bail!(
                "trace session {} contains metrics without a metrics summary",
                capture.session_id,
            );
        }

        return Ok(());
    }

    if capture.metric_dropped != 0 {
        bail!(
            "trace session {} overflowed: {} metric observations were dropped",
            capture.session_id,
            capture.metric_dropped,
        );
    }

    if capture.expected_metrics != capture.metric_definitions.len() {
        bail!(
            "trace session {} declares {} metrics but contains {} definitions",
            capture.session_id,
            capture.expected_metrics,
            capture.metric_definitions.len(),
        );
    }

    if capture.expected_metrics != capture.metrics.len() {
        bail!(
            "trace session {} declares {} metrics but contains {} metric records",
            capture.session_id,
            capture.expected_metrics,
            capture.metrics.len(),
        );
    }

    Ok(())
}

struct LineParser<'a> {
    original: &'a str,
    rest: &'a str,
}

impl<'a> LineParser<'a> {
    fn new(line: &'a str) -> Self {
        Self {
            original: line,
            rest: line,
        }
    }

    fn number<T>(&mut self, prefix: &str) -> Result<T>
    where
        T: core::str::FromStr,
    {
        let token = self.token(prefix)?;

        token.parse().map_err(|_| {
            anyhow!(
                "invalid numeric value `{token}` after `{prefix}` in `{}`",
                self.original,
            )
        })
    }

    fn component(&mut self, prefix: &str) -> Result<String> {
        self.skip_whitespace();

        let rest = self
            .rest
            .strip_prefix(prefix)
            .with_context(|| format!("expected `{prefix}` in trace record `{}`", self.original))?;

        let colon = rest.find(':').with_context(|| {
            format!(
                "missing length separator after `{prefix}` in `{}`",
                self.original,
            )
        })?;

        let length_text = &rest[..colon];

        if length_text.is_empty() {
            bail!(
                "missing component length after `{prefix}` in `{}`",
                self.original,
            );
        }

        let length: usize = length_text.parse().with_context(|| {
            format!(
                "invalid component length `{length_text}` in `{}`",
                self.original,
            )
        })?;

        let contents = &rest[colon + 1..];

        let value = contents.get(..length).with_context(|| {
            format!(
                "component after `{prefix}` declares {length} bytes but the record is shorter or splits UTF-8 in `{}`",
                self.original,
            )
        })?;

        let tail = contents.get(length..).with_context(|| {
            format!(
                "component after `{prefix}` does not end on a UTF-8 boundary in `{}`",
                self.original,
            )
        })?;

        self.rest = tail;

        Ok(value.to_owned())
    }

    fn value(&mut self) -> Result<Value> {
        let token = self.token("")?;

        if let Some(value) = token.strip_prefix("u:") {
            let value = value.parse::<u32>().with_context(|| {
                format!(
                    "invalid unsigned trace value `{token}` in `{}`",
                    self.original,
                )
            })?;

            return Ok(Value::Unsigned(value));
        }

        if let Some(value) = token.strip_prefix("i:") {
            let value = value.parse::<i32>().with_context(|| {
                format!(
                    "invalid signed trace value `{token}` in `{}`",
                    self.original,
                )
            })?;

            return Ok(Value::Signed(value));
        }

        if let Some(value) = token.strip_prefix("b:") {
            return match value {
                "0" => Ok(Value::Bool(false)),
                "1" => Ok(Value::Bool(true)),

                _ => bail!(
                    "boolean trace value must be b:0 or b:1, got `{token}` in `{}`",
                    self.original,
                ),
            };
        }

        bail!(
            "unknown trace value encoding `{token}` in `{}`",
            self.original,
        )
    }

    fn token(&mut self, prefix: &str) -> Result<&'a str> {
        self.skip_whitespace();

        let rest = self
            .rest
            .strip_prefix(prefix)
            .with_context(|| format!("expected `{prefix}` in trace record `{}`", self.original))?;

        let end = rest
            .char_indices()
            .find_map(|(index, character)| character.is_ascii_whitespace().then_some(index))
            .unwrap_or(rest.len());

        let token = &rest[..end];

        if token.is_empty() {
            bail!("missing value after `{prefix}` in `{}`", self.original);
        }

        self.rest = &rest[end..];

        Ok(token)
    }

    fn finish(mut self) -> Result<()> {
        self.skip_whitespace();

        if !self.rest.is_empty() {
            bail!(
                "unexpected trailing data `{}` in `{}`",
                self.rest,
                self.original,
            );
        }

        Ok(())
    }

    fn skip_whitespace(&mut self) {
        self.rest = self.rest.trim_start_matches([' ', '\t']);
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use crate::trace::{Metric, MetricKind};

    use super::{Value, parse_captures};

    fn assert_error_contains(error: &anyhow::Error, expected: &str) {
        let message = format!("{error:#}");

        assert!(
            message.contains(expected),
            "expected error to contain `{expected}`, got:\n{message}",
        );
    }

    #[test]
    fn parses_self_describing_capture() {
        let log = indoc! {r#"
            1.000 INFO trace/v2 capture session=7 hz=240000000 origin=100 spans=2 dropped=0 open=0
            1.001 INFO trace/v2 define id=0 target=17:reader.pagination name=7:measure fields=2 5:bytes 6:cached
            1.002 INFO trace/v2 span id=0 depth=1 start=10 cycles=20 values=2 u:12 b:1
            1.003 INFO trace/v2 span id=0 depth=1 start=40 cycles=30 values=2 u:17 b:0
            1.004 INFO trace/v2 end session=7
        "#};

        let captures = parse_captures(log).unwrap();

        assert_eq!(captures.len(), 1);

        let capture = &captures[0];

        assert_eq!(capture.session_id, 7);
        assert_eq!(capture.clock_hz, 240_000_000);
        assert_eq!(capture.origin_cycles, 100);
        assert_eq!(capture.spans.len(), 2);

        let callsite = capture.callsites.get(&0).unwrap();

        assert_eq!(callsite.id, 0);
        assert_eq!(callsite.target, "reader.pagination");
        assert_eq!(callsite.name, "measure");
        assert_eq!(callsite.fields, ["bytes", "cached"]);

        assert_eq!(
            capture.spans[0].values,
            [Value::Unsigned(12), Value::Bool(true),],
        );

        assert_eq!(
            capture.spans[1].values,
            [Value::Unsigned(17), Value::Bool(false),],
        );
    }

    #[test]
    fn parses_length_prefixed_components_with_spaces() {
        let log = indoc! {r#"
            1.000 INFO trace/v2 capture session=1 hz=240000000 origin=10 spans=1 dropped=0 open=0
            1.001 INFO trace/v2 define id=5 target=11:reader text name=11:shape piece fields=1 10:byte count
            1.002 INFO trace/v2 span id=5 depth=0 start=1 cycles=2 values=1 u:12
            1.003 INFO trace/v2 end session=1
        "#};

        let captures = parse_captures(log).unwrap();
        let callsite = captures[0].callsites.get(&5).unwrap();

        assert_eq!(callsite.target, "reader text");
        assert_eq!(callsite.name, "shape piece");
        assert_eq!(callsite.fields, ["byte count"]);
    }

    #[test]
    fn parses_signed_and_boolean_values() {
        let log = indoc! {r#"
            1.000 INFO trace/v2 capture session=3 hz=240000000 origin=100 spans=1 dropped=0 open=0
            1.001 INFO trace/v2 define id=0 target=4:test name=6:values fields=2 5:delta 5:ready
            1.002 INFO trace/v2 span id=0 depth=0 start=5 cycles=10 values=2 i:-17 b:0
            1.003 INFO trace/v2 end session=3
        "#};

        let captures = parse_captures(log).unwrap();

        assert_eq!(
            captures[0].spans[0].values,
            [Value::Signed(-17), Value::Bool(false),],
        );
    }

    #[test]
    fn rejects_undefined_callsites() {
        let log = indoc! {r#"
            1.000 INFO trace/v2 capture session=1 hz=240000000 origin=0 spans=1 dropped=0 open=0
            1.001 INFO trace/v2 span id=4 depth=0 start=0 cycles=1 values=0
            1.002 INFO trace/v2 end session=1
        "#};

        let error = parse_captures(log).unwrap_err();

        assert_error_contains(&error, "undefined callsite 4");
    }

    #[test]
    fn rejects_field_value_count_mismatch() {
        let log = indoc! {r#"
            1.000 INFO trace/v2 capture session=1 hz=240000000 origin=0 spans=1 dropped=0 open=0
            1.001 INFO trace/v2 define id=0 target=4:test name=4:span fields=1 5:value
            1.002 INFO trace/v2 span id=0 depth=0 start=0 cycles=1 values=0
            1.003 INFO trace/v2 end session=1
        "#};

        let error = parse_captures(log).unwrap_err();

        assert_error_contains(&error, "declares 1 fields but span contains 0 values");
    }

    #[test]
    fn rejects_incomplete_capture() {
        let log = indoc! {r#"
            1.000 INFO trace/v2 capture session=1 hz=240000000 origin=0 spans=1 dropped=0 open=0
            1.001 INFO trace/v2 define id=0 target=4:test name=4:span fields=0
            1.002 INFO trace/v2 span id=0 depth=0 start=0 cycles=1 values=0
        "#};

        let error = parse_captures(log).unwrap_err();

        assert_error_contains(&error, "missing matching trace/v2 end");
    }

    #[test]
    fn rejects_bad_component_length() {
        let log = indoc! {r#"
            1.000 INFO trace/v2 capture session=1 hz=240000000 origin=0 spans=1 dropped=0 open=0
            1.001 INFO trace/v2 define id=0 target=99:test name=4:span fields=0
        "#};

        assert!(parse_captures(log).is_err());
    }

    #[test]
    fn rejects_dropped_spans() {
        let log = indoc! {r#"
            1.000 INFO trace/v2 capture session=1 hz=240000000 origin=0 spans=0 dropped=1 open=0
            1.001 INFO trace/v2 end session=1
        "#};

        let error = parse_captures(log).unwrap_err();

        assert_error_contains(&error, "1 spans were dropped");
    }

    #[test]
    fn parses_metric_summaries() {
        let log = indoc! {r#"
            1.000 INFO trace/v2 capture session=7 hz=240000000 origin=100 spans=0 dropped=0 open=0
            1.001 INFO trace/v2 metrics count=2 dropped=0
            1.002 INFO trace/v2 metric_define id=0 target=17:reader.pagination name=10:cache_hits kind=counter unit=0:
            1.003 INFO trace/v2 metric id=0 count=3 sum=5 max=3
            1.004 INFO trace/v2 metric_define id=1 target=17:reader.pagination name=12:measure_text kind=distribution unit=6:cycles
            1.005 INFO trace/v2 metric id=1 count=4 sum=120 max=50
            1.006 INFO trace/v2 end session=7
        "#};

        let captures = parse_captures(log).unwrap();

        let capture = &captures[0];

        assert_eq!(capture.expected_metrics, 2);

        assert_eq!(capture.metrics.len(), 2);

        let counter = capture.metric_definitions.get(&0).unwrap();

        assert_eq!(counter.target, "reader.pagination");

        assert_eq!(counter.name, "cache_hits");

        assert_eq!(counter.kind, MetricKind::Counter);

        assert_eq!(counter.unit, "");

        let timer = capture.metric_definitions.get(&1).unwrap();

        assert_eq!(timer.kind, MetricKind::Distribution);

        assert_eq!(timer.unit, "cycles");

        assert_eq!(
            capture.metrics.get(&1),
            Some(&Metric {
                id: 1,
                count: 4,
                sum: 120,
                max: 50,
            }),
        );
    }

    #[test]
    fn rejects_metric_without_definition() {
        let log = indoc! {r#"
            1.000 INFO trace/v2 capture session=1 hz=240000000 origin=0 spans=0 dropped=0 open=0
            1.001 INFO trace/v2 metrics count=1 dropped=0
            1.002 INFO trace/v2 metric id=0 count=1 sum=4 max=4
            1.003 INFO trace/v2 end session=1
        "#};

        let error = parse_captures(log).unwrap_err();

        assert_error_contains(&error, "undefined metric 0");
    }

    #[test]
    fn rejects_dropped_metrics() {
        let log = indoc! {r#"
            1.000 INFO trace/v2 capture session=1 hz=240000000 origin=0 spans=0 dropped=0 open=0
            1.001 INFO trace/v2 metrics count=0 dropped=1
            1.002 INFO trace/v2 end session=1
        "#};

        let error = parse_captures(log).unwrap_err();

        assert_error_contains(&error, "1 metric observations were dropped");
    }
}
