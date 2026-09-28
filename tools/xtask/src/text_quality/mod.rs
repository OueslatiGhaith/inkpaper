//! measures reader text as the device draws it, so rendering changes can be judged
//! by numbers instead of by eye. The metrics are described on [`metrics::SizeMetrics`].

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, anyhow};

mod hinted;
mod metrics;
mod render;

pub use hinted::Hinting;
use metrics::SizeMetrics;
use render::Rendered;

/// the reader's font size range, see `crates/app/src/reader/mod.rs`
const SIZES: [u16; 4] = [14, 20, 26, 32];

/// the corpus is rendered through the whole runtime, whose glyph caches live inline
const STACK_BYTES: usize = 256 << 20;

const ZOOM: usize = 4;

// public domain prose (Austen, Melville) followed by every glyph class once
const CORPUS: &str = "\
It is a truth universally acknowledged, that a single man in possession of a good fortune, must be in want of a wife. However little known the feelings or views of such a man may be on his first entering a neighbourhood, this truth is so well fixed in the minds of the surrounding families, that he is considered the rightful property of some one or other of their daughters.
\u{201c}My dear Mr. Bennet,\u{201d} said his lady to him one day, \u{201c}have you heard that Netherfield Park is let at last?\u{201d} Mr. Bennet replied that he had not. \u{201c}But it is,\u{201d} returned she; \u{201c}for Mrs. Long has just been here, and she told me all about it.\u{201d} Mr. Bennet made no answer.
Call me Ishmael. Some years ago\u{2014}never mind how long precisely\u{2014}having little or no money in my purse, and nothing particular to interest me on shore, I thought I would sail about a little and see the watery part of the world. It is a way I have of driving off the spleen and regulating the circulation. Whenever I find myself growing grim about the mouth; whenever it is a damp, drizzly November in my soul; whenever I find myself involuntarily pausing before coffin warehouses, and bringing up the rear of every funeral I meet; then, I account it high time to get to sea as soon as I can.
abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789 .,;:!?'\"()-\u{2013}\u{2014}\u{2018}\u{2019}\u{201c}\u{201d}
";

pub fn run(
    label: &str,
    compare: Option<&str>,
    min_ink_coverage: Option<u8>,
    hinting: Option<Hinting>,
) -> Result<()> {
    let output = output_directory().join(label);

    std::fs::create_dir_all(&output)?;

    let measured = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn({
            let output = output.clone();

            move || -> Result<Vec<(u16, SizeMetrics)>> {
                SIZES
                    .iter()
                    .map(|size| {
                        let rendered = render::render(CORPUS, *size, min_ink_coverage, hinting)
                            .with_context(|| format!("rendering {size}px"))?;

                        write_images(&output, *size, &rendered)?;

                        let metrics = metrics::measure(&rendered)
                            .with_context(|| format!("measuring {size}px"))?;

                        Ok((*size, metrics))
                    })
                    .collect()
            }
        })?
        .join()
        .map_err(|_| anyhow!("text-quality thread panicked"))??;

    let table = metrics_table(&measured);

    std::fs::write(output.join("metrics.tsv"), to_tsv(&table))?;

    match compare {
        Some(baseline) => {
            let path = output_directory().join(baseline).join("metrics.tsv");
            let before = from_tsv(
                &std::fs::read_to_string(&path)
                    .with_context(|| format!("reading {}", path.display()))?,
            )?;

            print_comparison(baseline, label, &before, &table);
        }
        None => print_table(&table),
    }

    println!("\nwrote {}", output.display());

    Ok(())
}

fn output_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("xtask lives two levels below the workspace root")
        .join("target/text-quality")
}

/// metric name -> size -> value, in display order
type Table = Vec<(String, BTreeMap<u16, String>)>;

/// a metric's name and how to format it
type MetricRow = (&'static str, fn(&SizeMetrics) -> String);

fn metrics_table(measured: &[(u16, SizeMetrics)]) -> Table {
    let rows: [MetricRow; 10] = [
        ("glyph_inconsistency_pct", |m| {
            format!("{:.1}", m.glyph_inconsistency_pct)
        }),
        ("edge_noise_per_1000_ink", |m| {
            format!("{:.1}", m.edge_noise_per_1000_ink)
        }),
        ("stem_consistency_pct", |m| {
            format!("{:.1}", m.stem_consistency_pct)
        }),
        ("spacing_error_mean_px", |m| {
            format!("{:.3}", m.spacing_error_mean_px)
        }),
        ("spacing_error_max_px", |m| {
            format!("{:.3}", m.spacing_error_max_px)
        }),
        ("weight_ratio", |m| format!("{:.3}", m.weight_ratio)),
        ("blurred_error_pct", |m| {
            format!("{:.2}", m.blurred_error_pct)
        }),
        ("stem_widths", |m| {
            m.stem_widths
                .iter()
                .map(|(width, count)| format!("{width}:{count}"))
                .collect::<Vec<_>>()
                .join(" ")
        }),
        ("glyphs_measured", |m| m.glyphs_measured.to_string()),
        ("glyphs_excluded", |m| m.glyphs_excluded.to_string()),
    ];

    rows.iter()
        .map(|(name, value)| {
            (
                (*name).to_owned(),
                measured
                    .iter()
                    .map(|(size, metrics)| (*size, value(metrics)))
                    .collect(),
            )
        })
        .collect()
}

fn to_tsv(table: &Table) -> String {
    let mut output = String::from("metric\tsize\tvalue\n");

    for (name, values) in table {
        for (size, value) in values {
            let _ = writeln!(output, "{name}\t{size}\t{value}");
        }
    }

    output
}

fn from_tsv(input: &str) -> Result<Table> {
    let mut table: Table = Vec::new();

    for line in input.lines().skip(1) {
        let mut fields = line.split('\t');
        let (Some(name), Some(size), Some(value)) = (fields.next(), fields.next(), fields.next())
        else {
            return Err(anyhow!("malformed metrics line {line:?}"));
        };

        let size = size.parse()?;

        match table.iter_mut().find(|(existing, _)| existing == name) {
            Some((_, values)) => {
                values.insert(size, value.to_owned());
            }
            None => table.push((name.to_owned(), BTreeMap::from([(size, value.to_owned())]))),
        }
    }

    Ok(table)
}

fn print_table(table: &Table) {
    print!("{:<26}", "metric");
    for size in SIZES {
        print!("{:>20}", format!("{size}px"));
    }
    println!();

    for (name, values) in table {
        print!("{name:<26}");
        for size in SIZES {
            print!(" {:>19}", values.get(&size).map_or("-", String::as_str));
        }
        println!();
    }
}

fn print_comparison(before_label: &str, after_label: &str, before: &Table, after: &Table) {
    println!("{before_label} -> {after_label}\n");

    print!("{:<26}", "metric");
    for size in SIZES {
        print!("{:>28}", format!("{size}px"));
    }
    println!();

    for (name, values) in after {
        let previous = before
            .iter()
            .find(|(existing, _)| existing == name)
            .map(|(_, values)| values);

        print!("{name:<26}");

        for size in SIZES {
            let now = values.get(&size).map_or("-", String::as_str);
            let then = previous
                .and_then(|values| values.get(&size))
                .map_or("-", String::as_str);

            let cell = match (then.parse::<f64>(), now.parse::<f64>()) {
                (Ok(then_value), Ok(now_value)) if then != now => {
                    format!("{then}->{now} ({:+.2})", now_value - then_value)
                }
                _ if then == now => now.to_owned(),
                _ => format!("{then}->{now}"),
            };

            print!(" {cell:>27}");
        }

        println!();
    }
}

fn write_images(output: &Path, size: u16, rendered: &Rendered) -> Result<()> {
    let page: Vec<u8> = rendered
        .ink
        .iter()
        .map(|ink| if *ink { 0 } else { 255 })
        .collect();
    let ideal: Vec<u8> = rendered
        .ideal
        .iter()
        .map(|coverage| 255 - (coverage * 255.0).round() as u8)
        .collect();

    write_gray_png(
        &output.join(format!("{size}px.png")),
        rendered.width,
        rendered.height,
        &page,
    )?;
    write_gray_png(
        &output.join(format!("{size}px-ideal.png")),
        rendered.width,
        rendered.height,
        &ideal,
    )?;

    let zoomed_width = rendered.width * ZOOM;
    let zoomed: Vec<u8> = (0..rendered.height * ZOOM)
        .flat_map(|y| (0..zoomed_width).map(move |x| (x / ZOOM, y / ZOOM)))
        .map(|(x, y)| page[y * rendered.width + x])
        .collect();

    write_gray_png(
        &output.join(format!("{size}px-x{ZOOM}.png")),
        zoomed_width,
        rendered.height * ZOOM,
        &zoomed,
    )
}

fn write_gray_png(path: &Path, width: usize, height: usize, pixels: &[u8]) -> Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, u32::try_from(width)?, u32::try_from(height)?);

    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder.write_header()?;

    writer.write_image_data(pixels)?;
    writer.finish()?;

    Ok(())
}
