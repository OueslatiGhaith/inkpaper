use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result, bail};

#[derive(Debug)]
struct Capture {
    hz: u32,
    frames: Vec<Frame>,
}

#[derive(Debug, Clone)]
struct Frame {
    id: u32,

    refresh: String,
    presentation: String,

    x: u16,
    y: u16,
    width: u16,
    height: u16,

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
    present_io_calls: u32,
    present_busy_waits: u32,
    present_longest_busy: u32,
    present_busy_dropped: u32,
    present_busy_cycles: Vec<u64>,

    framebuffer_pixels: u64,
    framebuffer_valid: bool,

    text_draws: u64,
    glyphs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FrameSignature {
    refresh: String,
    presentation: String,

    x: u16,
    y: u16,
    width: u16,
    height: u16,

    framebuffer_pixels: Option<u64>,

    text_draws: u64,
    glyphs: u64,
}

impl Frame {
    fn render_cycles(&self) -> u64 {
        self.rebuild
            .saturating_add(self.layout)
            .saturating_add(self.clear)
            .saturating_add(self.paint)
            .saturating_add(self.damage)
    }

    fn total_cycles(&self) -> u64 {
        self.render_cycles().saturating_add(self.present)
    }

    fn signature(&self) -> FrameSignature {
        FrameSignature {
            refresh: self.refresh.clone(),
            presentation: self.presentation.clone(),

            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,

            framebuffer_pixels: self.framebuffer_valid.then_some(self.framebuffer_pixels),

            text_draws: self.text_draws,
            glyphs: self.glyphs,
        }
    }
}

pub fn summary(input: &Path) -> Result<()> {
    let capture = read_capture(input)?;

    println!("InkPaper performance summary — {} Hz", capture.hz);

    println!();

    println!(
        "{:>5}  {:<5}  {:<10}  {:>9}  {:>8}  {:>8}  {:>9}  {:>9}  {:>9}",
        "frame", "refresh", "mode", "pixels", "glyphs", "render", "paint", "present", "total",
    );

    println!(
        "{:-<5}  {:-<5}  {:-<10}  {:-<9}  {:-<8}  {:-<8}  {:-<9}  {:-<9}  {:-<9}",
        "", "", "", "", "", "", "", "", "",
    );

    for frame in &capture.frames {
        let pixels = if frame.framebuffer_valid {
            frame.framebuffer_pixels.to_string()
        } else {
            "-".to_owned()
        };

        println!(
            "{:>5}  {:<5}  {:<10}  {:>9}  {:>8}  {:>7.1}ms  {:>8.1}ms  {:>8.1}ms  {:>8.1}ms",
            frame.id,
            frame.refresh,
            frame.presentation,
            pixels,
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
            cycles_ms(u64::from(frame.present_longest_busy), capture.hz),
        );
    }

    if capture
        .frames
        .iter()
        .any(|frame| !frame.present_busy_cycles.is_empty())
    {
        println!();
        println!("Individual BUSY waits");
        println!();

        for frame in &capture.frames {
            if frame.present_busy_cycles.is_empty() {
                continue;
            }

            print!("frame {:>5}:", frame.id);

            for (index, cycles) in frame.present_busy_cycles.iter().copied().enumerate() {
                print!("  #{}={:.1}ms", index, cycles_ms(cycles, capture.hz));
            }

            if frame.present_busy_dropped != 0 {
                print!("  dropped={}", frame.present_busy_dropped);
            }

            println!();
        }
    }

    Ok(())
}

pub fn compare(before: &Path, after: &Path) -> Result<()> {
    let before = read_capture(before)?;
    let after = read_capture(after)?;

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

    println!("InkPaper performance comparison — {} Hz", before.hz);

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

    if before.frames.len() != after.frames.len() {
        println!(
            "warning: frame counts differ: before={} after={}",
            before.frames.len(),
            after.frames.len(),
        );
    }

    if mismatched != 0 {
        println!(
            "warning: {mismatched} compared frame(s) have different workload signatures; marked with `*`",
        );

        println!(
            "signature includes refresh mode, presentation mode, damage bounds, framebuffer pixels, text draws, and glyph count",
        );
    }

    Ok(())
}

fn read_capture(path: &Path) -> Result<Capture> {
    let log =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    parse_capture(&log)
}

fn parse_capture(log: &str) -> Result<Capture> {
    let mut hz = None;
    let mut frames = Vec::new();
    let mut busy_waits: BTreeMap<u32, Vec<(u32, u64)>> = BTreeMap::new();

    for line in log.lines() {
        if let Some((_, payload)) = line.split_once("perf/config ") {
            let found = u32_field(payload, "hz")?;

            if let Some(existing) = hz
                && existing != found
            {
                bail!("capture contains multiple clock rates: {existing} and {found}");
            }

            hz = Some(found);

            continue;
        }

        if let Some((_, payload)) = line.split_once("perf/present_busy ") {
            let frame_id = u32_field(payload, "frame")?;
            let index = u32_field(payload, "index")?;
            let cycles = u64_field(payload, "cycles")?;

            busy_waits
                .entry(frame_id)
                .or_default()
                .push((index, cycles));

            continue;
        }

        let Some((_, payload)) = line.split_once("perf/frame ") else {
            continue;
        };

        frames.push(Frame {
            id: u32_field(payload, "id")?,
            refresh: string_field(payload, "refresh")?,
            presentation: string_field(payload, "presentation")?,
            x: u16_field(payload, "x")?,
            y: u16_field(payload, "y")?,
            width: u16_field(payload, "width")?,
            height: u16_field(payload, "height")?,
            rebuild: u64_field(payload, "rebuild")?,
            layout: u64_field(payload, "layout")?,
            clear: u64_field(payload, "clear")?,
            paint: u64_field(payload, "paint")?,
            damage: u64_field(payload, "damage")?,
            present: u64_field(payload, "present")?,
            present_io: u64_field(payload, "present_io")?,
            present_busy: u64_field(payload, "present_busy")?,
            present_other: u64_field(payload, "present_other")?,
            present_bytes: u64_field(payload, "present_bytes")?,
            present_io_calls: u32_field(payload, "present_io_calls")?,
            present_busy_waits: u32_field(payload, "present_busy_waits")?,
            present_longest_busy: u32_field(payload, "present_longest_busy")?,
            present_busy_dropped: optional_u32_field(payload, "present_busy_dropped")?.unwrap_or(0),
            present_busy_cycles: Vec::new(),
            framebuffer_pixels: u64_field(payload, "framebuffer_pixels")?,
            framebuffer_valid: u8_field(payload, "framebuffer_valid")? != 0,
            text_draws: u64_field(payload, "text_draws")?,
            glyphs: u64_field(payload, "glyphs")?,
        });
    }

    let hz = hz.ok_or_else(|| anyhow::anyhow!("no `perf/config` record found"))?;

    if frames.is_empty() {
        bail!("no `perf/frame` records found");
    }

    frames.sort_by_key(|frame| frame.id);

    for frame in &mut frames {
        let Some(mut waits) = busy_waits.remove(&frame.id) else {
            // older captures did not contain individual BUSY records.
            continue;
        };

        waits.sort_by_key(|(index, _)| *index);

        for (expected, (actual, _)) in waits.iter().enumerate() {
            let expected = u32::try_from(expected).unwrap_or(u32::MAX);

            if *actual != expected {
                bail!(
                    "frame {} has non-contiguous BUSY wait indices: expected {} but found {}",
                    frame.id,
                    expected,
                    actual,
                );
            }
        }

        let expected_samples = frame
            .present_busy_waits
            .saturating_sub(frame.present_busy_dropped);

        let actual_samples = u32::try_from(waits.len()).unwrap_or(u32::MAX);

        if actual_samples != expected_samples {
            bail!(
                "frame {} reports {} stored BUSY waits but log contains {}",
                frame.id,
                expected_samples,
                actual_samples,
            );
        }

        frame.present_busy_cycles = waits.into_iter().map(|(_, cycles)| cycles).collect();
    }

    for pair in frames.windows(2) {
        if pair[0].id == pair[1].id {
            bail!("duplicate perf frame id {}", pair[0].id);
        }
    }

    Ok(Capture { hz, frames })
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
        .with_context(|| format!("missing `{name}` in perf line: {payload}"))
}

fn string_field(payload: &str, name: &str) -> Result<String> {
    Ok(field(payload, name)?.to_owned())
}

fn u8_field(payload: &str, name: &str) -> Result<u8> {
    field(payload, name)?
        .parse()
        .with_context(|| format!("invalid `{name}` in perf line: {payload}"))
}

fn u16_field(payload: &str, name: &str) -> Result<u16> {
    field(payload, name)?
        .parse()
        .with_context(|| format!("invalid `{name}` in perf line: {payload}"))
}

fn u32_field(payload: &str, name: &str) -> Result<u32> {
    field(payload, name)?
        .parse()
        .with_context(|| format!("invalid `{name}` in perf line: {payload}"))
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
        format!("invalid `{name}` in perf line: {payload}")
    })?))
}

fn u64_field(payload: &str, name: &str) -> Result<u64> {
    field(payload, name)?
        .parse()
        .with_context(|| format!("invalid `{name}` in perf line: {payload}"))
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use crate::perf::{cycles_ms, io_kib_per_second, parse_capture};

    #[test]
    fn parses_perf_capture() {
        let log = indoc! {r#"
        0.000 INFO perf/config hz=240000000

        0.900 INFO perf/present_busy frame=1 index=0 cycles=400
        0.901 INFO perf/present_busy frame=1 index=1 cycles=900
        0.902 INFO perf/present_busy frame=1 index=2 cycles=1200
        1.000 INFO perf/frame id=1 refresh=Full presentation=Gray4 x=0 y=0 width=800 height=480 rebuild=500 layout=1000 clear=200 paint=2000 damage=10 present=4000 present_io=1000 present_busy=2500 present_other=500 present_bytes=96000 present_io_calls=800 present_busy_waits=3 present_longest_busy=1200 present_busy_dropped=0 framebuffer_pixels=156480 framebuffer_valid=1 text_draws=3 glyphs=34

        1.900 INFO perf/present_busy frame=2 index=0 cycles=1000
        2.000 INFO perf/frame id=2 refresh=Full presentation=Binary x=0 y=0 width=800 height=480 rebuild=0 layout=1000 clear=200 paint=1000 damage=10 present=2000 present_io=700 present_busy=1000 present_other=300 present_bytes=48000 present_io_calls=400 present_busy_waits=1 present_longest_busy=1000 present_busy_dropped=0 framebuffer_pixels=0 framebuffer_valid=1 text_draws=31 glyphs=1021
    "#};

        let capture = parse_capture(log).unwrap();

        assert_eq!(capture.hz, 240_000_000);

        assert_eq!(capture.frames.len(), 2);

        let first = &capture.frames[0];

        assert_eq!(first.id, 1);

        assert_eq!(first.presentation, "Gray4");

        assert_eq!(first.framebuffer_pixels, 156_480);

        assert!(first.framebuffer_valid);

        assert_eq!(first.glyphs, 34);

        assert_eq!(first.present_io, 1_000);

        assert_eq!(first.present_busy, 2_500);

        assert_eq!(first.present_other, 500);

        assert_eq!(first.present_bytes, 96_000);

        assert_eq!(first.present_busy_waits, 3);

        assert_eq!(first.present_busy_dropped, 0);

        assert_eq!(first.present_busy_cycles, vec![400, 900, 1_200]);
    }

    #[test]
    fn converts_cycles_to_milliseconds() {
        assert_eq!(cycles_ms(240_000, 240_000_000), 1.0);
    }

    #[test]
    fn workload_signature_detects_changed_content() {
        let log = indoc! {r#"
            0.000 INFO perf/config hz=240000000
            1.000 INFO perf/frame id=1 refresh=Full presentation=Gray4 x=0 y=0 width=800 height=480 rebuild=0 layout=0 clear=0 paint=100 damage=0 present=200 present_io=50 present_busy=100 present_other=50 present_bytes=96000 present_io_calls=800 present_busy_waits=4 present_longest_busy=50 framebuffer_pixels=156480 framebuffer_valid=1 text_draws=3 glyphs=34
            2.000 INFO perf/frame id=2 refresh=Full presentation=Gray4 x=0 y=0 width=800 height=480 rebuild=0 layout=0 clear=0 paint=100 damage=0 present=200 present_io=50 present_busy=100 present_other=50 present_bytes=96000 present_io_calls=800 present_busy_waits=4 present_longest_busy=50 framebuffer_pixels=155520 framebuffer_valid=1 text_draws=3 glyphs=34
        "#};

        let capture = parse_capture(log).unwrap();

        assert_ne!(capture.frames[0].signature(), capture.frames[1].signature());
    }

    #[test]
    fn calculates_io_throughput() {
        let rate = io_kib_per_second(1024, 240_000_000, 240_000_000);

        assert_eq!(rate, 1.0);
    }
}
