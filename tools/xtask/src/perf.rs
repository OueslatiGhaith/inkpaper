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

fn u64_field(payload: &str, name: &str) -> Result<u64> {
    field(payload, name)?
        .parse()
        .with_context(|| format!("invalid `{name}` in perf line: {payload}"))
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::{cycles_ms, parse_capture};

    #[test]
    fn parses_perf_capture() {
        let log = indoc! {r#"
            0.000 INFO perf/config hz=240000000
            1.000 INFO perf/frame id=1 refresh=Full presentation=Gray4 x=0 y=0 width=800 height=480 rebuild=500 layout=1000 clear=200 paint=2000 damage=10 present=4000 framebuffer_pixels=156480 framebuffer_valid=1 text_draws=3 glyphs=34
            2.000 INFO perf/frame id=2 refresh=Full presentation=Binary x=0 y=0 width=800 height=480 rebuild=0 layout=1000 clear=200 paint=1000 damage=10 present=2000 framebuffer_pixels=0 framebuffer_valid=1 text_draws=31 glyphs=1021
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
    }

    #[test]
    fn converts_cycles_to_milliseconds() {
        assert_eq!(cycles_ms(240_000, 240_000_000), 1.0);
    }

    #[test]
    fn workload_signature_detects_changed_content() {
        let log = indoc! {r#"
            0.000 INFO perf/config hz=240000000
            1.000 INFO perf/frame id=1 refresh=Full presentation=Gray4 x=0 y=0 width=800 height=480 rebuild=0 layout=0 clear=0 paint=100 damage=0 present=200 framebuffer_pixels=156480 framebuffer_valid=1 text_draws=3 glyphs=34
            2.000 INFO perf/frame id=2 refresh=Full presentation=Gray4 x=0 y=0 width=800 height=480 rebuild=0 layout=0 clear=0 paint=100 damage=0 present=200 framebuffer_pixels=155520 framebuffer_valid=1 text_draws=3 glyphs=34
        "#};

        let capture = parse_capture(log).unwrap();

        assert_ne!(capture.frames[0].signature(), capture.frames[1].signature());
    }
}
