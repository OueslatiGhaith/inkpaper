use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result, bail};

use super::{Callsite, Capture, MetricKind, Span, Value, parse_captures};

#[derive(Debug)]
struct PerformanceCapture {
    hz: u32,
    frames: Vec<Frame>,

    overwritten: u64,
    metric_dropped: u64,
    incomplete_frames: usize,
}

#[derive(Debug, Clone)]
struct Frame {
    id: u32,

    phases: Vec<String>,

    render: u64,
    rebuild: u64,
    layout: u64,
    clear: u64,
    paint: u64,
    damage: u64,

    present: u64,
    present_io: u64,
    present_busy: u64,
    present_other: u64,
    present_bytes: u64,
    present_busy_waits: u64,
    present_longest_busy: u64,

    busy_wait_cycles: Vec<u64>,

    framebuffer_pixels: u64,
    text_draws: u64,
    glyphs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FrameSignature {
    phases: Vec<String>,
    framebuffer_pixels: u64,
    text_draws: u64,
    glyphs: u64,
    present_bytes: u64,
}

impl Frame {
    fn render_cycles(&self) -> u64 {
        self.render
    }

    fn total_cycles(&self) -> u64 {
        self.render.saturating_add(self.present)
    }

    fn signature(&self) -> FrameSignature {
        FrameSignature {
            phases: self.phases.clone(),
            framebuffer_pixels: self.framebuffer_pixels,
            text_draws: self.text_draws,
            glyphs: self.glyphs,
            present_bytes: self.present_bytes,
        }
    }

    fn phase_label(&self) -> &str {
        if self
            .phases
            .iter()
            .any(|phase| phase == "binary_full_refresh")
        {
            return "binary/full";
        }

        if self
            .phases
            .iter()
            .any(|phase| phase == "binary_fast_refresh")
        {
            return "binary/fast";
        }

        if self
            .phases
            .iter()
            .any(|phase| phase == "grayscale_base_refresh")
        {
            return "gray/full";
        }

        if self
            .phases
            .iter()
            .any(|phase| phase == "grayscale_precondition")
        {
            return "gray/fast";
        }

        if self.phases.iter().any(|phase| phase == "grayscale_refresh") {
            return "gray";
        }

        self.phases
            .iter()
            .find(|phase| {
                phase.as_str() != "power_on"
                    && phase.as_str() != "power_off"
                    && phase.as_str() != "unknown"
            })
            .map(String::as_str)
            .unwrap_or("-")
    }
}

#[derive(Debug, Default)]
struct FrameBuilder {
    render: Option<RenderFrame>,
    present: Option<PresentFrame>,
}

#[derive(Debug)]
struct RenderFrame {
    render: u64,

    rebuild: u64,
    layout: u64,
    clear: u64,
    paint: u64,
    damage: u64,

    framebuffer_pixels: u64,
    text_draws: u64,
    glyphs: u64,
}

#[derive(Debug)]
struct PresentFrame {
    phases: Vec<String>,

    present: u64,
    present_io: u64,
    present_busy: u64,
    present_other: u64,
    present_bytes: u64,
    present_busy_waits: u64,
    present_longest_busy: u64,

    busy_wait_cycles: Vec<u64>,
}

pub fn summarize_performance(input: &Path) -> Result<()> {
    let capture = read_performance_capture(input)?;

    println!("InkPaper performance summary - {} Hz", capture.hz);

    println!();

    println!(
        "{:>5}  {:<13}  {:>9}  {:>8}  {:>9}  {:>9}  {:>9}  {:>9}",
        "frame", "phase", "pixels", "glyphs", "render", "paint", "present", "total",
    );

    println!(
        "{:-<5}  {:-<13}  {:-<9}  {:-<8}  {:-<9}  {:-<9}  {:-<9}  {:-<9}",
        "", "", "", "", "", "", "", "",
    );

    for frame in &capture.frames {
        println!(
            "{:>5}  {:<13}  {:>9}  {:>8}  {:>8.1}ms  {:>8.1}ms  {:>8.1}ms  {:>8.1}ms",
            frame.id,
            frame.phase_label(),
            frame.framebuffer_pixels,
            frame.glyphs,
            cycles_ms(frame.render_cycles(), capture.hz),
            cycles_ms(frame.paint, capture.hz),
            cycles_ms(frame.present, capture.hz),
            cycles_ms(frame.total_cycles(), capture.hz),
        );
    }

    println!();
    println!("Present breakdown");
    println!();

    println!(
        "{:>5}  {:>9}  {:>9}  {:>9}  {:>9}  {:>10}  {:>7}  {:>9}",
        "frame", "io", "busy", "other", "bytes", "KiB/s", "waits", "max busy",
    );

    println!(
        "{:-<5}  {:-<9}  {:-<9}  {:-<9}  {:-<9}  {:-<10}  {:-<7}  {:-<9}",
        "", "", "", "", "", "", "", "",
    );

    for frame in &capture.frames {
        println!(
            "{:>5}  {:>8.1}ms  {:>8.1}ms  {:>8.1}ms  {:>9}  {:>10.1}  {:>7}  {:>8.1}ms",
            frame.id,
            cycles_ms(frame.present_io, capture.hz),
            cycles_ms(frame.present_busy, capture.hz),
            cycles_ms(frame.present_other, capture.hz),
            frame.present_bytes,
            io_kib_per_second(frame.present_bytes, frame.present_io, capture.hz),
            frame.present_busy_waits,
            cycles_ms(frame.present_longest_busy, capture.hz),
        );
    }

    if capture
        .frames
        .iter()
        .any(|frame| !frame.busy_wait_cycles.is_empty())
    {
        println!();
        println!("Individual BUSY waits");
        println!();

        for frame in &capture.frames {
            if frame.busy_wait_cycles.is_empty() {
                continue;
            }

            print!("frame {:>5}:", frame.id);

            for (index, cycles) in frame.busy_wait_cycles.iter().copied().enumerate() {
                print!("  #{}={:.1}ms", index, cycles_ms(cycles, capture.hz));
            }

            println!();
        }
    }

    print_health_warnings(None, &capture);

    Ok(())
}

pub fn compare_performance(before: &Path, after: &Path) -> Result<()> {
    let before = read_performance_capture(before)?;

    let after = read_performance_capture(after)?;

    if before.hz != after.hz {
        bail!(
            "clock rates differ: before={} Hz after={} Hz",
            before.hz,
            after.hz,
        );
    }

    let before_by_id: BTreeMap<_, _> = before
        .frames
        .iter()
        .map(|frame| (frame.id, frame))
        .collect();

    let after_by_id: BTreeMap<_, _> = after.frames.iter().map(|frame| (frame.id, frame)).collect();

    let mut compared = 0usize;
    let mut mismatched = 0usize;

    println!("InkPaper performance comparison - {} Hz", before.hz);

    println!();

    println!(
        "{:>5}  {:>22}  {:>9}  {:>22}  {:>9}  {:>22}  {:>9}",
        "frame", "paint", "delta", "present", "delta", "total", "delta",
    );

    println!(
        "{:-<5}  {:-<22}  {:-<9}  {:-<22}  {:-<9}  {:-<22}  {:-<9}",
        "", "", "", "", "", "", "",
    );

    for (id, before_frame) in &before_by_id {
        let Some(after_frame) = after_by_id.get(id) else {
            continue;
        };

        compared += 1;

        let same_workload = before_frame.signature() == after_frame.signature();

        if !same_workload {
            mismatched += 1;
        }

        let marker = if same_workload { "" } else { "*" };

        println!(
            "{:>4}{}  {:>9.1} -> {:>8.1}  {:>+8.2}%  {:>9.1} -> {:>8.1}  {:>+8.2}%  {:>9.1} -> {:>8.1}  {:>+8.2}%",
            id,
            marker,
            cycles_ms(before_frame.paint, before.hz),
            cycles_ms(after_frame.paint, after.hz),
            delta_percent(before_frame.paint, after_frame.paint),
            cycles_ms(before_frame.present, before.hz),
            cycles_ms(after_frame.present, after.hz),
            delta_percent(before_frame.present, after_frame.present),
            cycles_ms(before_frame.total_cycles(), before.hz),
            cycles_ms(after_frame.total_cycles(), after.hz),
            delta_percent(before_frame.total_cycles(), after_frame.total_cycles()),
        );
    }

    if compared == 0 {
        bail!("no matching frame ids found between captures");
    }

    println!();

    if compared != before.frames.len() || compared != after.frames.len() {
        println!(
            "warning: matched {} frame(s), before has {}, after has {}",
            compared,
            before.frames.len(),
            after.frames.len(),
        );
    }

    if mismatched != 0 {
        println!(
            "warning: {mismatched} compared frame(s) have different workload signatures; marked with `*`",
        );

        println!(
            "signature includes display phase sequence, framebuffer pixels, text draws, glyph count, and presentation bytes",
        );
    }

    print_health_warnings(Some("before"), &before);

    print_health_warnings(Some("after"), &after);

    Ok(())
}

fn read_performance_capture(path: &Path) -> Result<PerformanceCapture> {
    let log =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    let captures = parse_captures(&log)?;

    build_performance_capture(&captures)
}

fn build_performance_capture(captures: &[Capture]) -> Result<PerformanceCapture> {
    let first = captures
        .first()
        .context("cannot summarize an empty trace")?;

    let hz = first.clock_hz;

    if captures.iter().any(|capture| capture.clock_hz != hz) {
        bail!("performance report requires one stable trace clock frequency");
    }

    let mut frames: BTreeMap<u32, FrameBuilder> = BTreeMap::new();

    let mut overwritten = 0u64;
    let mut metric_dropped = 0u64;

    for capture in captures {
        overwritten = overwritten.saturating_add(capture.overwritten);

        metric_dropped = metric_dropped.saturating_add(u64::from(capture.metric_dropped));

        if let Some((render_span, render_callsite)) =
            find_unique_span(capture, "ui.render", "render")?
        {
            let frame_id = span_u32_field(render_callsite, render_span, "frame")?;

            let render = RenderFrame {
                render: render_span.duration_cycles,

                rebuild: span_duration(capture, "ui.render", "rebuild")?.unwrap_or(0),

                layout: span_duration(capture, "ui.render", "layout")?.unwrap_or(0),

                clear: span_duration(capture, "ui.render", "clear")?
                    .context("render capture is missing ui.render/clear span")?,

                paint: span_duration(capture, "ui.render", "paint")?
                    .context("render capture is missing ui.render/paint span")?,

                damage: span_duration(capture, "ui.render", "damage")?
                    .context("render capture is missing ui.render/damage span")?,

                framebuffer_pixels: gauge_value(capture, "ui.coverage", "framebuffer_pixels")?,

                text_draws: gauge_value(capture, "ui.text", "draw_calls")?,

                glyphs: gauge_value(capture, "ui.text", "shaped_glyphs")?,
            };

            let builder = frames.entry(frame_id).or_default();

            if builder.render.replace(render).is_some() {
                bail!("frame {frame_id} contains more than one render capture");
            }
        }

        if let Some((present_span, present_callsite)) =
            find_unique_span(capture, "display.present", "present")?
        {
            let frame_id = span_u32_field(present_callsite, present_span, "frame")?;

            let busy_wait_cycles = busy_wait_cycles(capture)?;

            let present_busy = busy_wait_cycles
                .iter()
                .copied()
                .fold(0u64, u64::saturating_add);

            let present_busy_waits = u64::try_from(busy_wait_cycles.len()).unwrap_or(u64::MAX);

            let present_longest_busy = busy_wait_cycles.iter().copied().max().unwrap_or(0);

            let present_io = gauge_value(capture, "display.present", "io_cycles")?;

            let present = present_span.duration_cycles;

            let present_other = present
                .saturating_sub(present_io)
                .saturating_sub(present_busy);

            let present = PresentFrame {
                phases: display_phases(capture)?,

                present,
                present_io,
                present_busy,
                present_other,

                present_bytes: gauge_value(capture, "display.present", "bytes")?,

                present_busy_waits,
                present_longest_busy,

                busy_wait_cycles,
            };

            let builder = frames.entry(frame_id).or_default();

            if builder.present.replace(present).is_some() {
                bail!("frame {frame_id} contains more than one presentation capture");
            }
        }
    }

    let mut complete_frames = Vec::new();
    let mut incomplete_frames = 0usize;

    for (id, builder) in frames {
        let (Some(render), Some(present)) = (builder.render, builder.present) else {
            incomplete_frames += 1;
            continue;
        };

        complete_frames.push(Frame {
            id,

            phases: present.phases,

            render: render.render,
            rebuild: render.rebuild,
            layout: render.layout,
            clear: render.clear,
            paint: render.paint,
            damage: render.damage,

            present: present.present,
            present_io: present.present_io,
            present_busy: present.present_busy,
            present_other: present.present_other,
            present_bytes: present.present_bytes,
            present_busy_waits: present.present_busy_waits,
            present_longest_busy: present.present_longest_busy,

            busy_wait_cycles: present.busy_wait_cycles,

            framebuffer_pixels: render.framebuffer_pixels,
            text_draws: render.text_draws,
            glyphs: render.glyphs,
        });
    }

    if complete_frames.is_empty() {
        bail!("trace contains no complete render/present performance frames");
    }

    Ok(PerformanceCapture {
        hz,
        frames: complete_frames,
        overwritten,
        metric_dropped,
        incomplete_frames,
    })
}

fn gauge_value(capture: &Capture, target: &str, name: &str) -> Result<u64> {
    let mut found = None;

    for definition in capture.metric_definitions.values() {
        if definition.target != target || definition.name != name {
            continue;
        }

        if definition.kind != MetricKind::Gauge {
            bail!("performance metric {target}/{name} is not a gauge");
        }

        if found.is_some() {
            bail!(
                "capture {} contains multiple definitions for metric {target}/{name}",
                capture.capture_id,
            );
        }

        let metric = capture.metrics.get(&definition.id).with_context(|| {
            format!(
                "capture {} is missing metric value for {target}/{name}",
                capture.capture_id,
            )
        })?;

        found = Some(metric.sum);
    }

    found.with_context(|| {
        format!(
            "capture {} is missing required performance metric {target}/{name}",
            capture.capture_id,
        )
    })
}

fn span_duration(capture: &Capture, target: &str, name: &str) -> Result<Option<u64>> {
    Ok(find_unique_span(capture, target, name)?.map(|(span, _)| span.duration_cycles))
}

fn frame_id_from_span(capture: &Capture, target: &str, name: &str) -> Result<Option<u32>> {
    let Some((span, callsite)) = find_unique_span(capture, target, name)? else {
        return Ok(None);
    };

    span_u32_field(callsite, span, "frame").map(Some)
}

fn find_unique_span<'a>(
    capture: &'a Capture,
    target: &str,
    name: &str,
) -> Result<Option<(&'a Span, &'a Callsite)>> {
    let mut found = None;

    for span in &capture.spans {
        let callsite = capture.callsites.get(&span.callsite_id).with_context(|| {
            format!(
                "capture {} is missing callsite {}",
                capture.capture_id, span.callsite_id,
            )
        })?;

        if callsite.target != target || callsite.name != name {
            continue;
        }

        if found.is_some() {
            bail!(
                "capture {} contains multiple {target}/{name} spans",
                capture.capture_id,
            );
        }

        found = Some((span, callsite));
    }

    Ok(found)
}

fn span_u32_field(callsite: &Callsite, span: &Span, field_name: &str) -> Result<u32> {
    let index = callsite
        .fields
        .iter()
        .position(|field| field == field_name)
        .with_context(|| {
            format!(
                "trace span {}/{} has no `{field_name}` field",
                callsite.target, callsite.name,
            )
        })?;

    let value = span.values.get(index).with_context(|| {
        format!(
            "trace span {}/{} is missing value for `{field_name}`",
            callsite.target, callsite.name,
        )
    })?;

    match *value {
        Value::Unsigned(value) => Ok(value),

        Value::Signed(_) | Value::Bool(_) => {
            bail!(
                "trace span {}/{} field `{field_name}` is not unsigned",
                callsite.target,
                callsite.name,
            )
        }
    }
}

fn display_phases(capture: &Capture) -> Result<Vec<String>> {
    let mut phases: Vec<(u64, String)> = Vec::new();

    for span in &capture.spans {
        let callsite = capture.callsites.get(&span.callsite_id).with_context(|| {
            format!(
                "capture {} is missing callsite {}",
                capture.capture_id, span.callsite_id,
            )
        })?;

        if callsite.target != "display.phase" {
            continue;
        }

        phases.push((span.start_cycles, callsite.name.clone()));
    }

    phases.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    Ok(phases.into_iter().map(|(_, name)| name).collect())
}

fn busy_wait_cycles(capture: &Capture) -> Result<Vec<u64>> {
    let mut waits: Vec<(u32, u64)> = Vec::new();

    for span in &capture.spans {
        let callsite = capture.callsites.get(&span.callsite_id).with_context(|| {
            format!(
                "capture {} is missing callsite {}",
                capture.capture_id, span.callsite_id,
            )
        })?;

        if callsite.target != "display.present" || callsite.name != "busy_wait" {
            continue;
        }

        let index = span_u32_field(callsite, span, "index")?;

        waits.push((index, span.duration_cycles));
    }

    waits.sort_by_key(|(index, _)| *index);

    for pair in waits.windows(2) {
        if pair[0].0 == pair[1].0 {
            bail!(
                "capture {} contains duplicate BUSY wait index {}",
                capture.capture_id,
                pair[0].0,
            );
        }
    }

    Ok(waits.into_iter().map(|(_, cycles)| cycles).collect())
}

fn print_health_warnings(label: Option<&str>, capture: &PerformanceCapture) {
    let prefix = label.map(|label| format!("{label}: ")).unwrap_or_default();

    if capture.overwritten != 0 {
        println!(
            "warning: {prefix}trace overwrote {} span record(s)",
            capture.overwritten,
        );
    }

    if capture.metric_dropped != 0 {
        println!(
            "warning: {prefix}trace dropped {} metric observation(s)",
            capture.metric_dropped,
        );
    }

    if capture.incomplete_frames != 0 {
        println!(
            "warning: {prefix}ignored {} incomplete frame(s)",
            capture.incomplete_frames,
        );
    }
}

fn cycles_ms(cycles: u64, hz: u32) -> f64 {
    cycles as f64 * 1_000.0 / f64::from(hz)
}

fn io_kib_per_second(bytes: u64, cycles: u64, hz: u32) -> f64 {
    if cycles == 0 {
        return 0.0;
    }

    let seconds = cycles as f64 / f64::from(hz);

    bytes as f64 / 1024.0 / seconds
}

fn delta_percent(before: u64, after: u64) -> f64 {
    if before == 0 {
        return if after == 0 { 0.0 } else { f64::INFINITY };
    }

    (after as f64 - before as f64) * 100.0 / before as f64
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::{build_performance_capture, cycles_ms, io_kib_per_second};

    use crate::trace::parse_captures;

    fn performance_log() -> &'static str {
        indoc! {r#"
            1.000 INFO trace/v4 capture id=1 hz=240000000 at=2600 spans=6 overwritten=0 metrics=3 metric_dropped=0
            1.001 INFO trace/v4 define id=0 target=9:ui.render name=6:render fields=1 5:frame
            1.002 INFO trace/v4 define id=1 target=9:ui.render name=7:rebuild fields=0
            1.003 INFO trace/v4 define id=2 target=9:ui.render name=6:layout fields=0
            1.004 INFO trace/v4 define id=3 target=9:ui.render name=5:clear fields=0
            1.005 INFO trace/v4 define id=4 target=9:ui.render name=5:paint fields=0
            1.006 INFO trace/v4 define id=5 target=9:ui.render name=6:damage fields=0
            1.007 INFO trace/v4 span id=1 kind=sync depth=1 start=1050 cycles=100 values=0
            1.008 INFO trace/v4 span id=2 kind=sync depth=1 start=1200 cycles=200 values=0
            1.009 INFO trace/v4 span id=3 kind=sync depth=1 start=1450 cycles=100 values=0
            1.010 INFO trace/v4 span id=4 kind=sync depth=1 start=1600 cycles=700 values=0
            1.011 INFO trace/v4 span id=5 kind=sync depth=1 start=2350 cycles=100 values=0
            1.012 INFO trace/v4 span id=0 kind=sync depth=0 start=1000 cycles=1500 values=1 u:1
            1.013 INFO trace/v4 metric_define id=0 target=11:ui.coverage name=18:framebuffer_pixels kind=gauge unit=6:pixels
            1.014 INFO trace/v4 metric id=0 value=156480
            1.015 INFO trace/v4 metric_define id=1 target=7:ui.text name=10:draw_calls kind=gauge unit=0:
            1.016 INFO trace/v4 metric id=1 value=31
            1.017 INFO trace/v4 metric_define id=2 target=7:ui.text name=13:shaped_glyphs kind=gauge unit=0:
            1.018 INFO trace/v4 metric id=2 value=1021
            1.019 INFO trace/v4 end id=1

            2.000 INFO trace/v4 capture id=2 hz=240000000 at=5000 spans=4 overwritten=0 metrics=6 metric_dropped=0
            2.001 INFO trace/v4 define id=6 target=15:display.present name=7:present fields=1 5:frame
            2.002 INFO trace/v4 define id=7 target=13:display.phase name=19:binary_fast_refresh fields=0
            2.003 INFO trace/v4 define id=8 target=15:display.present name=9:busy_wait fields=1 5:index
            2.004 INFO trace/v4 span id=7 kind=async depth=0 start=3000 cycles=800 values=0
            2.005 INFO trace/v4 span id=8 kind=async depth=0 start=3100 cycles=200 values=1 u:0
            2.006 INFO trace/v4 span id=8 kind=async depth=0 start=3400 cycles=300 values=1 u:1
            2.007 INFO trace/v4 span id=6 kind=async depth=0 start=2900 cycles=2000 values=1 u:1
            2.008 INFO trace/v4 metric_define id=3 target=15:display.present name=9:io_cycles kind=gauge unit=6:cycles
            2.009 INFO trace/v4 metric id=3 value=1000
            2.010 INFO trace/v4 metric_define id=4 target=15:display.present name=5:bytes kind=gauge unit=5:bytes
            2.011 INFO trace/v4 metric id=4 value=120000
            2.012 INFO trace/v4 metric_define id=5 target=15:display.present name=8:io_calls kind=gauge unit=0:
            2.013 INFO trace/v4 metric id=5 value=1203
            2.014 INFO trace/v4 metric_define id=6 target=14:display.stream name=6:cycles kind=gauge unit=6:cycles
            2.015 INFO trace/v4 metric id=6 value=900
            2.016 INFO trace/v4 metric_define id=7 target=14:display.stream name=5:bytes kind=gauge unit=5:bytes
            2.017 INFO trace/v4 metric id=7 value=120000
            2.018 INFO trace/v4 metric_define id=8 target=14:display.stream name=5:calls kind=gauge unit=0:
            2.019 INFO trace/v4 metric id=8 value=1200
            2.020 INFO trace/v4 end id=2
        "#}
    }

    #[test]
    fn builds_performance_frame_from_trace() {
        let captures = parse_captures(performance_log()).unwrap();

        let performance = build_performance_capture(&captures).unwrap();

        assert_eq!(performance.hz, 240_000_000);
        assert_eq!(performance.frames.len(), 1);

        let frame = &performance.frames[0];

        assert_eq!(frame.id, 1);

        // The complete render duration comes from the outer render span.
        assert_eq!(frame.render_cycles(), 1_500);

        assert_eq!(frame.rebuild, 100);
        assert_eq!(frame.layout, 200);
        assert_eq!(frame.clear, 100);
        assert_eq!(frame.paint, 700);
        assert_eq!(frame.damage, 100);

        // Presentation timing also comes from spans.
        assert_eq!(frame.present, 2_000);

        assert_eq!(frame.present_io, 1_000);
        assert_eq!(frame.present_busy, 500);
        assert_eq!(frame.present_other, 500);

        assert_eq!(frame.present_busy_waits, 2);
        assert_eq!(frame.present_longest_busy, 300);
        assert_eq!(frame.busy_wait_cycles, [200, 300]);

        assert_eq!(frame.total_cycles(), 3_500);

        assert_eq!(frame.framebuffer_pixels, 156_480);
        assert_eq!(frame.text_draws, 31);
        assert_eq!(frame.glyphs, 1_021);
        assert_eq!(frame.present_bytes, 120_000);

        assert_eq!(frame.phase_label(), "binary/fast");

        assert_eq!(performance.overwritten, 0);
        assert_eq!(performance.metric_dropped, 0);
        assert_eq!(performance.incomplete_frames, 0);
    }

    #[test]
    fn workload_signature_detects_changed_output() {
        let captures = parse_captures(performance_log()).unwrap();

        let performance = build_performance_capture(&captures).unwrap();

        let before = performance.frames[0].clone();

        let mut after = before.clone();

        assert_eq!(before.signature(), after.signature());

        after.present_bytes += 1;

        assert_ne!(before.signature(), after.signature());
    }

}
