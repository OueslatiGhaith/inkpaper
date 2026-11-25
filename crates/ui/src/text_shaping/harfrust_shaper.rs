//! OpenType text shaping via `harfrust` for TTF/OTF faces.
//!
//! BiDi visual runs come from `bidi::resolve_visual_runs`. Within each visual run,
//! characters are itemized into contiguous `(FontInstance, Script)` sub-runs (with
//! fallback to `\u{FFFD}` / `?` for missing characters, keeping default-ignorables and
//! combining marks attached to their base's font), then shaped with `harfrust`.

use alloc::{string::String, vec::Vec};

use harfrust::{Buffer, BufferFlags, Direction, Script, ShapeOptions, ShaperFont};

use crate::{FontInstance, FontRegistry, GlyphId, Offset, PreparedFont, ShapeSummary, px};

use super::{
    PreparedSimpleShaper, ShapeError, ShapedGlyph,
    arabic::joining_type,
    bidi::{VisualRun, piece_cluster},
    logical::DEFAULT_IGNORABLES,
    marks::extends_cluster,
};

struct SubRun {
    font: FontInstance,
    script: Option<Script>,
    /// Range into the itemizer's flat `(char, line_byte_offset)` item list.
    items_start: usize,
    items_end: usize,
}

impl<'font> PreparedSimpleShaper<'font> {
    /// Shapes `text` followed by `suffix` using `harfrust` when the registry contains
    /// TTF/OTF faces. Returns `None` for non-TTF registries (bitmap or synthetic faces)
    /// so the caller falls through to the zero-alloc `SimpleShaper` pipeline.
    pub(super) fn try_shape_line_with_harfrust<const FONTS: usize>(
        &self,
        registry: &'font FontRegistry<'_, FONTS>,
        size_px: u16,
        text: &str,
        suffix: &str,
        runs: &[VisualRun],
        mut visit: impl FnMut(ShapedGlyph),
    ) -> Option<Result<ShapeSummary, ShapeError>> {
        let preferred_has_harf = self
            .prepared_font
            .as_ref()
            .and_then(PreparedFont::harfrust_font)
            .is_some();

        if !preferred_has_harf && !registry.has_ttf_face() {
            return None;
        }

        let total_len = text.len() + suffix.len();
        if total_len == 0 || size_px == 0 {
            return Some(Ok(ShapeSummary::new(0, px(0))));
        }

        let mut buffer = Buffer::new();
        buffer.set_flags(BufferFlags::REMOVE_DEFAULT_IGNORABLES);

        let mut items: Vec<(char, usize)> = Vec::new();
        let mut sub_runs: Vec<SubRun> = Vec::new();
        let mut combined_scratch = String::new();
        let mut glyph_count = 0usize;
        let mut total_advance = px(0);

        for run in runs {
            let start = run.range.start.min(total_len);
            let end = run.range.end.min(total_len);
            if start >= end {
                continue;
            }

            let slice = slice_for_run(text, suffix, start, end, &mut combined_scratch);

            items.clear();
            sub_runs.clear();
            itemize_run(
                registry,
                self.preferred_font,
                self.prepared_font.as_ref(),
                slice,
                start,
                &mut items,
                &mut sub_runs,
            );

            let sub_run_indices = 0..sub_runs.len();
            if run.right_to_left {
                for index in sub_run_indices.rev() {
                    shape_sub_run(
                        registry,
                        self.prepared_font.as_ref(),
                        size_px,
                        text,
                        &sub_runs[index],
                        &items,
                        true,
                        &mut buffer,
                        &mut glyph_count,
                        &mut total_advance,
                        &mut visit,
                    );
                }
            } else {
                for index in sub_run_indices {
                    shape_sub_run(
                        registry,
                        self.prepared_font.as_ref(),
                        size_px,
                        text,
                        &sub_runs[index],
                        &items,
                        false,
                        &mut buffer,
                        &mut glyph_count,
                        &mut total_advance,
                        &mut visit,
                    );
                }
            }
        }

        Some(Ok(ShapeSummary::new(glyph_count, total_advance)))
    }
}

fn slice_for_run<'a>(
    text: &'a str,
    suffix: &'a str,
    start: usize,
    end: usize,
    scratch: &'a mut String,
) -> &'a str {
    if end <= text.len() {
        &text[start..end]
    } else if start >= text.len() {
        &suffix[start - text.len()..end - text.len()]
    } else {
        scratch.clear();
        scratch.push_str(&text[start..]);
        scratch.push_str(&suffix[..end - text.len()]);
        scratch.as_str()
    }
}

fn itemize_run<const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    preferred_font: FontInstance,
    prepared_font: Option<&PreparedFont<'_>>,
    slice: &str,
    run_start: usize,
    items: &mut Vec<(char, usize)>,
    sub_runs: &mut Vec<SubRun>,
) {
    let preferred_resolved = prepared_font
        .map(PreparedFont::instance)
        .or_else(|| registry.resolve_instance(preferred_font).map(|r| r.instance()));

    for (rel_offset, character) in slice.char_indices() {
        let line_offset = run_start + rel_offset;

        let (font, output_char, char_script) = if DEFAULT_IGNORABLES.contains(character) {
            let Some(font) = sub_runs
                .last()
                .map(|run| run.font)
                .or(preferred_resolved)
            else {
                continue;
            };
            (font, character, Script::INHERITED)
        } else if (joining_type(character).is_transparent() || extends_cluster(character))
            && let Some(active_font) = sub_runs.last().map(|run| run.font)
            && face_supports(registry, prepared_font, active_font, character)
        {
            (active_font, character, Script::INHERITED)
        } else if let Some((font, ch)) =
            resolve_char_with_fallback(registry, preferred_font, prepared_font, character)
        {
            (font, ch, harfrust::unicode::script(u32::from(ch)))
        } else {
            continue;
        };

        let strong_script = (!is_neutral_script(char_script)).then_some(char_script);
        let item_index = items.len();
        items.push((output_char, line_offset));

        if let Some(last) = sub_runs.last_mut()
            && last.font == font
            && scripts_compatible(last.script, strong_script)
        {
            if last.script.is_none() {
                last.script = strong_script;
            }
            last.items_end = item_index + 1;
        } else {
            sub_runs.push(SubRun {
                font,
                script: strong_script,
                items_start: item_index,
                items_end: item_index + 1,
            });
        }
    }
}

fn is_neutral_script(script: Script) -> bool {
    matches!(script, Script::COMMON | Script::INHERITED | Script::UNKNOWN)
}

fn scripts_compatible(current: Option<Script>, next: Option<Script>) -> bool {
    match (current, next) {
        (_, None) | (None, Some(_)) => true,
        (Some(a), Some(b)) => a == b,
    }
}

fn face_supports<const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    prepared_font: Option<&PreparedFont<'_>>,
    instance: FontInstance,
    character: char,
) -> bool {
    if let Some(prepared) = prepared_font
        && prepared.instance() == instance
    {
        return prepared.glyph_id(character).is_some();
    }

    registry
        .resolve_instance(instance)
        .and_then(|resolved| resolved.glyph_id(character))
        .is_some()
}

fn resolve_char_exact<const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    preferred_font: FontInstance,
    prepared_font: Option<&PreparedFont<'_>>,
    character: char,
) -> Option<FontInstance> {
    if let Some(prepared) = prepared_font
        && prepared.glyph_id(character).is_some()
    {
        return Some(prepared.instance());
    }

    registry
        .resolve_character_exact(preferred_font, character)
        .map(|resolved| resolved.font_instance())
}

fn resolve_char_with_fallback<const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    preferred_font: FontInstance,
    prepared_font: Option<&PreparedFont<'_>>,
    character: char,
) -> Option<(FontInstance, char)> {
    if let Some(font) = resolve_char_exact(registry, preferred_font, prepared_font, character) {
        return Some((font, character));
    }

    if character != '\u{FFFD}'
        && let Some(font) = resolve_char_exact(registry, preferred_font, prepared_font, '\u{FFFD}')
    {
        return Some((font, '\u{FFFD}'));
    }

    if character != '?'
        && let Some(font) = resolve_char_exact(registry, preferred_font, prepared_font, '?')
    {
        return Some((font, '?'));
    }

    None
}

#[allow(clippy::too_many_arguments)]
fn shape_sub_run<const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    prepared_font: Option<&PreparedFont<'_>>,
    size_px: u16,
    text: &str,
    sub_run: &SubRun,
    items: &[(char, usize)],
    right_to_left: bool,
    buffer: &mut Buffer,
    glyph_count: &mut usize,
    total_advance: &mut crate::Pixels,
    visit: &mut impl FnMut(ShapedGlyph),
) {
    let sub_items = &items[sub_run.items_start..sub_run.items_end];
    if sub_items.is_empty() {
        return;
    }

    let owned_font;
    let harf_font = if let Some(prepared) = prepared_font
        && prepared.instance() == sub_run.font
        && let Some(font) = prepared.harfrust_font()
    {
        Some(font)
    } else {
        owned_font = registry
            .resolve_instance(sub_run.font)
            .and_then(|resolved| resolved.harfrust_font());
        owned_font.as_ref()
    };

    let Some(harf_font) = harf_font else {
        shape_non_ttf_sub_run(
            registry,
            size_px,
            text,
            sub_run.font,
            sub_items,
            right_to_left,
            glyph_count,
            total_advance,
            visit,
        );
        return;
    };

    buffer.clear();
    buffer.set_direction(if right_to_left {
        Direction::RightToLeft
    } else {
        Direction::LeftToRight
    });
    if let Some(script) = sub_run.script {
        buffer.set_script(Some(script));
    }

    for &(ch, line_offset) in sub_items {
        buffer.push(u32::from(ch), u32::try_from(line_offset).unwrap_or(u32::MAX));
    }
    buffer.guess_segment_properties();

    // Use 26.6 fixed-point scale (FreeType/HarfBuzz standard) so fractional
    // GPOS adjustments and anchor deltas combine before rounding to pixels.
    let scale_26_6 = i32::from(size_px) << 6;
    let shaper_font = ShaperFont::new(harf_font).with_scale(scale_26_6);

    if harfrust::shape(&shaper_font, buffer, ShapeOptions::default()).is_err() {
        return;
    }

    for (info, pos) in buffer
        .glyph_infos()
        .iter()
        .zip(buffer.glyph_positions().iter())
    {
        let Ok(glyph_id) = u16::try_from(info.glyph_id) else {
            continue;
        };

        let advance = px(round_26_6(pos.x_advance));
        let offset = Offset::new(
            px(round_26_6(pos.x_offset)),
            // HarfBuzz Y grows upward; framebuffer Y grows downward.
            px(-round_26_6(pos.y_offset)),
        );
        let cluster = piece_cluster(text, info.cluster as usize);

        visit(ShapedGlyph::new_with_instance(
            sub_run.font,
            GlyphId::new(glyph_id),
            cluster,
            advance,
            offset,
            advance,
        ));
        *glyph_count += 1;
        *total_advance += advance;
    }
}

/// Fallback for a non-TTF face inside a mixed TTF + bitmap registry.
#[allow(clippy::too_many_arguments)]
fn shape_non_ttf_sub_run<const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    size_px: u16,
    text: &str,
    font: FontInstance,
    sub_items: &[(char, usize)],
    right_to_left: bool,
    glyph_count: &mut usize,
    total_advance: &mut crate::Pixels,
    visit: &mut impl FnMut(ShapedGlyph),
) {
    let Some(resolved) = registry.resolve_instance(font) else {
        return;
    };

    let mut emit_item = |ch: char, line_offset: usize| {
        if DEFAULT_IGNORABLES.contains(ch) {
            return;
        }
        let Some((glyph, advance)) = resolved.glyph_id_and_advance(ch, size_px) else {
            return;
        };
        let cluster = piece_cluster(text, line_offset);
        visit(ShapedGlyph::new_with_instance(
            font,
            glyph,
            cluster,
            advance,
            Offset::ZERO,
            advance,
        ));
        *glyph_count += 1;
        *total_advance += advance;
    };

    if right_to_left {
        for &(ch, line_offset) in sub_items.iter().rev() {
            emit_item(ch, line_offset);
        }
    } else {
        for &(ch, line_offset) in sub_items {
            emit_item(ch, line_offset);
        }
    }
}

#[inline]
const fn round_26_6(value: i32) -> i32 {
    value.saturating_add(32) >> 6
}
