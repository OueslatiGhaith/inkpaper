use std::collections::{BTreeMap, HashMap};

use anyhow::{Result, ensure};
use inkpaper_ui::GlyphId;

use super::render::{Placement, Rendered};

pub struct SizeMetrics {
    /// glyph occurrences whose pixels could be read off the page
    pub glyphs_measured: usize,
    /// occurrences left out because a neighbour overlaps them or the line clip cuts them
    pub glyphs_excluded: usize,
    /// % of occurrences that differ from their letter's most common rendering.
    /// Lower is better
    pub glyph_inconsistency_pct: f64,
    /// isolated ink specks plus pinholes, per 1000 inked pixels. Lower is better
    pub edge_noise_per_1000_ink: f64,
    /// % of the stems of n m h u l i r that have the most common stem width. Higher is
    /// better
    pub stem_consistency_pct: f64,
    pub stem_widths: BTreeMap<usize, usize>,
    /// distance between each glyph's pen position and its unrounded position. Lower is
    /// better
    pub spacing_error_mean_px: f64,
    pub spacing_error_max_px: f64,
    /// inked pixels / ideal coverage. Not better or worse by itself: it shows how much
    /// darker or lighter than the outlines the text comes out
    pub weight_ratio: f64,
    /// mean difference from the ideal image after a 1px gaussian blur. Informational:
    /// it also grows with spacing drift, and dithering is designed to do well here even
    /// when it looks noisy up close
    pub blurred_error_pct: f64,
}

pub fn measure(rendered: &Rendered) -> Result<SizeMetrics> {
    let owners = coverage_owners(rendered);

    // every inked pixel must come from a glyph we placed, otherwise the harness
    // positions glyphs differently from the painter and nothing below is trustworthy
    let stray = rendered
        .ink
        .iter()
        .zip(&owners)
        .filter(|(ink, owners)| **ink && **owners == 0)
        .count();
    ensure!(
        stray == 0,
        "{stray} inked pixels are outside every placed glyph; the harness disagrees with the painter"
    );

    // a glyph's own pixels can be read off the page when no other glyph covers them
    // and the line clip did not cut it
    let isolated: Vec<(&Placement, Vec<bool>)> = rendered
        .glyphs
        .iter()
        .filter(|glyph| glyph.width > 0 && glyph.height > 0)
        .filter_map(|glyph| isolated_pattern(rendered, &owners, glyph).map(|ink| (glyph, ink)))
        .collect();

    let drawn = rendered
        .glyphs
        .iter()
        .filter(|glyph| glyph.coverage.iter().any(|coverage| *coverage > 0))
        .count();

    let (inconsistent, grouped) = inconsistent_occurrences(
        isolated
            .iter()
            .map(|(glyph, ink)| (glyph.glyph, ink.as_slice())),
    );

    let noise = count_edge_noise(&rendered.ink, rendered.width, rendered.height);

    let (stem_consistency_pct, stem_widths) = stems(rendered, &isolated);

    let spacing_errors: Vec<f64> = rendered
        .glyphs
        .iter()
        .filter(|glyph| !glyph.first_in_line)
        .map(|glyph| (f64::from(glyph.pen_x) - glyph.ideal_pen_x).abs())
        .collect();

    let ink_pixels = rendered.ink.iter().filter(|ink| **ink).count();
    let ideal_ink: f64 = rendered.ideal.iter().map(|value| f64::from(*value)).sum();

    Ok(SizeMetrics {
        glyphs_measured: isolated.len(),
        glyphs_excluded: drawn - isolated.len(),
        glyph_inconsistency_pct: percent(inconsistent, grouped),
        edge_noise_per_1000_ink: (noise.isolated + noise.pinholes) as f64 * 1000.0
            / ink_pixels.max(1) as f64,
        stem_consistency_pct,
        stem_widths,
        spacing_error_mean_px: spacing_errors.iter().sum::<f64>()
            / spacing_errors.len().max(1) as f64,
        spacing_error_max_px: spacing_errors.iter().copied().fold(0.0, f64::max),
        weight_ratio: ink_pixels as f64 / ideal_ink.max(f64::EPSILON),
        blurred_error_pct: blurred_error(rendered) * 100.0,
    })
}

fn percent(part: usize, whole: usize) -> f64 {
    part as f64 * 100.0 / whole.max(1) as f64
}

/// how many placed glyphs have non-zero coverage at each page pixel
fn coverage_owners(rendered: &Rendered) -> Vec<u8> {
    let mut owners = vec![0u8; rendered.width * rendered.height];

    for glyph in &rendered.glyphs {
        for dy in 0..glyph.height {
            for dx in 0..glyph.width {
                if !glyph.covers(dx, dy) {
                    continue;
                }

                if let Some(index) = page_index(rendered, glyph.x + dx as i32, glyph.y + dy as i32)
                {
                    owners[index] = owners[index].saturating_add(1);
                }
            }
        }
    }

    owners
}

fn page_index(rendered: &Rendered, x: i32, y: i32) -> Option<usize> {
    let x = usize::try_from(x).ok().filter(|x| *x < rendered.width)?;
    let y = usize::try_from(y).ok().filter(|y| *y < rendered.height)?;

    Some(y * rendered.width + x)
}

/// the glyph's ink in its own bitmap coordinates, or `None` when it cannot be
/// separated from its neighbours or was clipped
fn isolated_pattern(rendered: &Rendered, owners: &[u8], glyph: &Placement) -> Option<Vec<bool>> {
    let mut ink = vec![false; glyph.width * glyph.height];

    for dy in 0..glyph.height {
        for dx in 0..glyph.width {
            if !glyph.covers(dx, dy) {
                continue;
            }

            let x = glyph.x + dx as i32;
            let y = glyph.y + dy as i32;

            if !glyph.clip.contains(x, y) {
                return None;
            }

            let index = page_index(rendered, x, y)?;

            if owners[index] != 1 {
                return None;
            }

            ink[dy * glyph.width + dx] = rendered.ink[index];
        }
    }

    Some(ink)
}

/// occurrences that differ from their glyph's most common rendering, and the number
/// of occurrences of glyphs seen at least twice
fn inconsistent_occurrences<'a>(
    patterns: impl IntoIterator<Item = (GlyphId, &'a [bool])>,
) -> (usize, usize) {
    let mut by_glyph: HashMap<GlyphId, HashMap<&'a [bool], usize>> = HashMap::new();

    for (glyph, pattern) in patterns {
        *by_glyph
            .entry(glyph)
            .or_default()
            .entry(pattern)
            .or_default() += 1;
    }

    by_glyph
        .values()
        .map(|variants| {
            let total: usize = variants.values().sum();
            let modal = variants.values().copied().max().unwrap_or(0);

            (total, modal)
        })
        .filter(|(total, _)| *total >= 2)
        .fold((0, 0), |(inconsistent, grouped), (total, modal)| {
            (inconsistent + total - modal, grouped + total)
        })
}

/// share of vertical stems at the most common stem width, plus the width histogram.
///
/// stems are the ink runs on the row halfway up the x-height. An occurrence whose run
/// count does not match its letter (a stem broken up or merged) counts all its stems
/// as off-width.
fn stems(
    rendered: &Rendered,
    isolated: &[(&Placement, Vec<bool>)],
) -> (f64, BTreeMap<usize, usize>) {
    let mut widths = BTreeMap::new();
    let mut expected_total = 0usize;
    let mut measured = Vec::new();

    for (glyph, ink) in isolated {
        let Some((_, expected)) = rendered
            .stem_glyphs
            .iter()
            .find(|(stem_glyph, _)| *stem_glyph == glyph.glyph)
        else {
            continue;
        };

        expected_total += expected;

        let row = glyph.baseline - rendered.x_height / 2 - glyph.y;
        let Some(row) = usize::try_from(row).ok().filter(|row| *row < glyph.height) else {
            continue;
        };

        let runs = ink_runs(&ink[row * glyph.width..(row + 1) * glyph.width]);

        for width in &runs {
            *widths.entry(*width).or_insert(0) += 1;
        }

        if runs.len() == *expected {
            measured.extend(runs);
        }
    }

    let modal = widths
        .iter()
        .max_by_key(|(_, count)| **count)
        .map(|(width, _)| *width);

    let consistent = measured
        .iter()
        .filter(|width| Some(**width) == modal)
        .count();

    (percent(consistent, expected_total), widths)
}

fn ink_runs(row: &[bool]) -> Vec<usize> {
    row.split(|ink| !ink)
        .map(<[bool]>::len)
        .filter(|len| *len > 0)
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
struct EdgeNoise {
    /// inked pixels with no inked 8-neighbour
    isolated: usize,
    /// white pixels whose 4 neighbours are all inked
    pinholes: usize,
}

fn count_edge_noise(ink: &[bool], width: usize, height: usize) -> EdgeNoise {
    let at = |x: isize, y: isize| {
        x >= 0
            && y >= 0
            && (x as usize) < width
            && (y as usize) < height
            && ink[y as usize * width + x as usize]
    };

    let mut noise = EdgeNoise {
        isolated: 0,
        pinholes: 0,
    };

    for y in 0..height as isize {
        for x in 0..width as isize {
            if at(x, y) {
                let neighbours = (-1..=1)
                    .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                    .filter(|offset| *offset != (0, 0))
                    .any(|(dx, dy)| at(x + dx, y + dy));

                if !neighbours {
                    noise.isolated += 1;
                }
            } else if at(x - 1, y) && at(x + 1, y) && at(x, y - 1) && at(x, y + 1) {
                noise.pinholes += 1;
            }
        }
    }

    noise
}

const BLUR_SIGMA: f64 = 1.0;
const BLUR_RADIUS: isize = 3;

/// mean absolute difference between the page and the ideal image after both are
/// blurred, over the text column
fn blurred_error(rendered: &Rendered) -> f64 {
    let ink: Vec<f64> = rendered
        .ink
        .iter()
        .map(|ink| if *ink { 1.0 } else { 0.0 })
        .collect();
    let ideal: Vec<f64> = rendered
        .ideal
        .iter()
        .map(|value| f64::from(*value))
        .collect();

    let ink = blur(&ink, rendered.width, rendered.height);
    let ideal = blur(&ideal, rendered.width, rendered.height);

    let area = rendered.text_area;
    let mut total = 0.0;
    let mut samples = 0usize;

    for y in area.top..area.bottom {
        for x in area.left..area.right {
            let index = y as usize * rendered.width + x as usize;

            total += (ink[index] - ideal[index]).abs();
            samples += 1;
        }
    }

    total / samples.max(1) as f64
}

fn blur(values: &[f64], width: usize, height: usize) -> Vec<f64> {
    let kernel: Vec<f64> = (-BLUR_RADIUS..=BLUR_RADIUS)
        .map(|offset| (-(offset * offset) as f64 / (2.0 * BLUR_SIGMA * BLUR_SIGMA)).exp())
        .collect();
    let sum: f64 = kernel.iter().sum();
    let kernel: Vec<f64> = kernel.iter().map(|weight| weight / sum).collect();

    let pass = |source: &[f64], horizontal: bool| {
        let mut output = vec![0.0; source.len()];

        for y in 0..height as isize {
            for x in 0..width as isize {
                let mut value = 0.0;

                for (tap, weight) in kernel.iter().enumerate() {
                    let offset = tap as isize - BLUR_RADIUS;
                    let (sx, sy) = if horizontal {
                        ((x + offset).clamp(0, width as isize - 1), y)
                    } else {
                        (x, (y + offset).clamp(0, height as isize - 1))
                    };

                    value += weight * source[sy as usize * width + sx as usize];
                }

                output[y as usize * width + x as usize] = value;
            }
        }

        output
    };

    pass(&pass(values, true), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bitmap(rows: &[&str]) -> (Vec<bool>, usize, usize) {
        let ink = rows
            .iter()
            .flat_map(|row| row.chars().map(|pixel| pixel == '#'))
            .collect();

        (ink, rows[0].len(), rows.len())
    }

    #[test]
    fn edge_noise_counts_specks_and_pinholes_but_not_strokes() {
        let (ink, width, height) = bitmap(&["#.....###", "......#.#", "..#...###", "...#....."]);

        // the top-left speck is isolated; the diagonal pair touch each other; the ring
        // encloses one pinhole
        assert_eq!(
            count_edge_noise(&ink, width, height),
            EdgeNoise {
                isolated: 1,
                pinholes: 1,
            }
        );
    }

    #[test]
    fn ink_runs_measure_each_stem() {
        let (row, _, _) = bitmap(&[".##..#...###."]);

        assert_eq!(ink_runs(&row), [2, 1, 3]);
    }

    #[test]
    fn inconsistency_counts_departures_from_the_modal_rendering() {
        let a = GlyphId::new(1);
        let b = GlyphId::new(2);
        let common = [true, false];
        let other = [false, true];

        let patterns = [
            (a, &common[..]),
            (a, &common[..]),
            (a, &other[..]),
            // glyphs seen once cannot be inconsistent and are left out
            (b, &other[..]),
        ];

        assert_eq!(inconsistent_occurrences(patterns), (1, 3));
    }
}
