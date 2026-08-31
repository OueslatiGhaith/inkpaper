use core::{convert::Infallible, str::CharIndices};

use crate::{FontId, FontRegistry, Offset, OpenTypeFeature, Pixels, ResolvedGlyph, px};

use super::{
    ShapeError, ShapeState, ShapeSummary, ShapedGlyph, SimpleShaper,
    arabic::{MarkPlacement, contextual_form, joining_type, lam_alef_form, mark_placement},
};

pub(crate) const ARABIC_ISOL_FEATURE: OpenTypeFeature = OpenTypeFeature::new(*b"isol");
pub(crate) const ARABIC_INIT_FEATURE: OpenTypeFeature = OpenTypeFeature::new(*b"init");
pub(crate) const ARABIC_MEDI_FEATURE: OpenTypeFeature = OpenTypeFeature::new(*b"medi");
pub(crate) const ARABIC_FINA_FEATURE: OpenTypeFeature = OpenTypeFeature::new(*b"fina");
pub(crate) const ARABIC_RLIG_FEATURE: OpenTypeFeature = OpenTypeFeature::new(*b"rlig");

impl SimpleShaper {
    pub fn try_shape_piece_with<'font, const FONTS: usize, F, E>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        preferred_font: FontId,
        size_px: u16,
        text: &str,
        state: &mut ShapeState,
        mut visit: F,
    ) -> Result<ShapeSummary, E>
    where
        F: FnMut(ShapedGlyph) -> Result<(), E>,
    {
        let mut advance = px(0);
        let mut glyph_count = 0usize;
        let mut characters = text.char_indices();
        let mut previous_cluster = None;

        while let Some((cluster, character)) = characters.next() {
            let joining = joining_type(character);

            // transparent marks belong to the preceeding logical cluster
            // they remain separate glyphs for now because we don't mark positioning yet,
            // but line breaking, ellipsis and bidi must treat the bae + marks as
            // invisible unit
            if joining.is_transparent() {
                let cluster = previous_cluster.unwrap_or(cluster);

                if let Some(resolved) = registry.resolve_glyph(preferred_font, character) {
                    if let Some(placement) = mark_placement(character) {
                        emit_resolved_mark(
                            resolved,
                            cluster,
                            size_px,
                            placement,
                            &mut glyph_count,
                            &mut visit,
                        )?;
                    } else {
                        // Preserve the existing behavior for transparent
                        // characters outside our supported Arabic-mark
                        // subset. We can expand the table deliberately
                        // later rather than guessing their placement.
                        emit_resolved_glyph(
                            resolved,
                            cluster,
                            size_px,
                            &mut advance,
                            &mut glyph_count,
                            &mut visit,
                        )?;
                    }
                }

                continue;
            }

            previous_cluster = Some(cluster);

            // lam-alef remains the only ligature the bounded shaper explicitly recognizes.
            // modern fonts get first priority through GSUB rlig.
            // Presentation Forms remain the compatibility fallback.
            if character == '\u{0644}'
                && let Some((_alef_cluster, alef)) = characters.clone().next()
            {
                let joins_previous = state.previous_joins_forward && joining.accepts_previous();

                if let Some(resolved) = resolve_lam_alef_ligature(
                    registry,
                    preferred_font,
                    alef,
                    joins_previous,
                    size_px,
                ) {
                    let _ = characters.next();

                    emit_resolved_ligature(
                        resolved,
                        cluster,
                        size_px,
                        2,
                        &mut advance,
                        &mut glyph_count,
                        &mut visit,
                    )?;

                    state.previous_joins_forward = false;
                    continue;
                }
            }

            let next_accepts = next_non_transparent_accepts_previous(&characters);
            let joins_previous = state.previous_joins_forward && joining.accepts_previous();
            let joins_next = joining.connects_forward() && next_accepts;
            let presentation = contextual_form(character, joins_previous, joins_next);

            // we only enable Arabic contextual GSUB for characters covered by our existing
            // Arabic forms data.
            // that keeps unrelated scripts out of Arabic isol/init/medi/fina features
            // while still making Presentation Forms merely a fallback.
            let feature =
                presentation.map(|_| arabic_contextual_feature(joins_previous, joins_next));

            let resolved = resolve_contextual_glyph(
                registry,
                preferred_font,
                character,
                feature,
                presentation,
            );

            if let Some(resolved) = resolved {
                emit_resolved_glyph(
                    resolved,
                    cluster,
                    size_px,
                    &mut advance,
                    &mut glyph_count,
                    &mut visit,
                )?;
            }

            state.previous_joins_forward = joining.connects_forward();
        }

        Ok(ShapeSummary::new(glyph_count, advance))
    }
    pub fn shape_piece_with<'font, const FONTS: usize, F>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        preferred_font: FontId,
        size_px: u16,
        text: &str,
        state: &mut ShapeState,
        mut visit: F,
    ) -> ShapeSummary
    where
        F: FnMut(ShapedGlyph),
    {
        let result =
            self.try_shape_piece_with(registry, preferred_font, size_px, text, state, |glyph| {
                visit(glyph);
                Ok::<(), Infallible>(())
            });

        match result {
            Ok(summary) => summary,
            Err(error) => match error {},
        }
    }
    pub fn shape_piece_into<'font, const FONTS: usize>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        preferred_font: FontId,
        size_px: u16,
        text: &str,
        state: &mut ShapeState,
        output: &mut [ShapedGlyph],
    ) -> Result<ShapeSummary, ShapeError> {
        let mut written = 0usize;

        self.try_shape_piece_with(registry, preferred_font, size_px, text, state, |glyph| {
            let Some(destination) = output.get_mut(written) else {
                return Err(ShapeError::BufferTooSmall);
            };

            *destination = glyph;
            written = written.saturating_add(1);

            Ok(())
        })
    }
    pub fn next_cluster_boundary<'font, const FONTS: usize>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        preferred_font: FontId,
        size_px: u16,
        text: &str,
        from: usize,
    ) -> Option<usize> {
        if from >= text.len() || !text.is_char_boundary(from) {
            return None;
        }

        let mut state = ShapeState::new();
        let mut next_boundary: Option<usize> = None;

        self.shape_piece_with(
            registry,
            preferred_font,
            size_px,
            text,
            &mut state,
            |glyph| {
                let cluster = glyph.cluster();
                if cluster <= from {
                    return;
                }

                next_boundary = Some(match next_boundary {
                    Some(current) => current.min(cluster),
                    None => cluster,
                });
            },
        );

        next_boundary.or(Some(text.len()))
    }
}

fn next_non_transparent_accepts_previous(characters: &CharIndices<'_>) -> bool {
    let lookahead = characters.clone();

    for (_, character) in lookahead {
        let joining = joining_type(character);

        if joining.is_transparent() {
            continue;
        }

        return joining.accepts_previous();
    }

    false
}

fn resolve_contextual_glyph<'font, const FONTS: usize>(
    registry: &FontRegistry<'font, FONTS>,
    preferred_font: FontId,
    base_character: char,
    feature: Option<OpenTypeFeature>,
    presentation: Option<char>,
) -> Option<ResolvedGlyph<'font>> {
    let base = registry.resolve_character_exact(preferred_font, base_character);

    // modern OpenType path.
    // crucially, substitution happens on the glyph from the face that actually resolved
    //  the base character.
    if let Some(base) = base
        && let Some(feature) = feature
        && let Some(glyph) = base.face().single_substitution(feature, base.glyph())
    {
        return Some(ResolvedGlyph::new(base.font(), base.face(), glyph));
    }

    // compatibility path for bitmap fonts and fonts which expose Arabic Presentation
    // Forms directly through cmap.
    if let Some(presentation) = presentation
        && let Some(resolved) = registry.resolve_character_exact(preferred_font, presentation)
    {
        return Some(resolved);
    }

    // if we had a valid base glyph but neither shaping path changed it, keep it readable
    // rather than replacing it.
    if let Some(base) = base {
        return Some(base);
    }

    registry.resolve_glyph(preferred_font, base_character)
}

fn resolve_lam_alef_ligature<'font, const FONTS: usize>(
    registry: &FontRegistry<'font, FONTS>,
    preferred_font: FontId,
    alef: char,
    joins_previous: bool,
    size_px: u16,
) -> Option<ResolvedGlyph<'font>> {
    // besides identifying the supported alef variants, this gives us the legacy compatibility glyph.
    let presentation = lam_alef_form(alef, joins_previous)?;

    // resolve lam first, then require alef from that same face. A GSUB ligature cannot span fonts.
    if let Some(lam) = registry.resolve_character_exact(preferred_font, '\u{0644}') {
        let face = lam.face();

        if let Some(alef_glyph) = face.glyph_id(alef) {
            let lam_feature = if joins_previous {
                ARABIC_MEDI_FEATURE
            } else {
                ARABIC_INIT_FEATURE
            };

            // arabic form substitutions run before rlig in our bounded shaping pipeline.
            let contextual_lam = face
                .single_substitution(lam_feature, lam.glyph())
                .unwrap_or(lam.glyph());

            let contextual_alef = face
                .single_substitution(ARABIC_FINA_FEATURE, alef_glyph)
                .unwrap_or(alef_glyph);

            let mut ligature =
                face.ligature_substitution(ARABIC_RLIG_FEATURE, contextual_lam, contextual_alef);

            // some simpler fonts encode the required ligature directly against cmap
            // glyphs instead of contextual-form outputs.
            if ligature.is_none()
                && (contextual_lam != lam.glyph() || contextual_alef != alef_glyph)
            {
                ligature = face.ligature_substitution(ARABIC_RLIG_FEATURE, lam.glyph(), alef_glyph);
            }

            if let Some(glyph) = ligature
                && face.glyph_advance(glyph, size_px).is_some()
            {
                return Some(ResolvedGlyph::new(lam.font(), face, glyph));
            }
        }
    }

    // old bitmap / Presentation Forms path.
    registry.resolve_character_exact(preferred_font, presentation)
}

fn emit_resolved_mark<'font, F, E>(
    resolved: ResolvedGlyph<'font>,
    cluster: usize,
    size_px: u16,
    placement: MarkPlacement,
    glyph_count: &mut usize,
    visit: &mut F,
) -> Result<bool, E>
where
    F: FnMut(ShapedGlyph) -> Result<(), E>,
{
    let font = resolved.font();
    let face = resolved.face();
    let glyph = resolved.glyph();

    let Some(base_advance) = face.glyph_advance(glyph, size_px) else {
        return Ok(false);
    };

    let shaped = ShapedGlyph::new_mark(font, glyph, cluster, base_advance, placement, Offset::ZERO);

    visit(shaped)?;

    // a combining mark exists in the glyph stream and counts toward buffer capacity,
    // but does not move the pen.
    *glyph_count = glyph_count.saturating_add(1);

    Ok(true)
}

fn emit_resolved_ligature<'font, F, E>(
    resolved: ResolvedGlyph<'font>,
    cluster: usize,
    size_px: u16,
    ligature_components: u16,
    advance: &mut Pixels,
    glyph_count: &mut usize,
    visit: &mut F,
) -> Result<bool, E>
where
    F: FnMut(ShapedGlyph) -> Result<(), E>,
{
    debug_assert!(ligature_components > 1);

    let font = resolved.font();
    let face = resolved.face();
    let glyph = resolved.glyph();

    let Some(base_advance) = face.glyph_advance(glyph, size_px) else {
        return Ok(false);
    };

    let shaped = ShapedGlyph::new_ligature(
        font,
        glyph,
        cluster,
        base_advance,
        ligature_components,
        Offset::ZERO,
        base_advance,
    );

    visit(shaped)?;

    *advance += base_advance;
    *glyph_count = glyph_count.saturating_add(1);

    Ok(true)
}

fn emit_resolved_glyph<'font, F, E>(
    resolved: ResolvedGlyph<'font>,
    cluster: usize,
    size_px: u16,
    advance: &mut Pixels,
    glyph_count: &mut usize,
    visit: &mut F,
) -> Result<bool, E>
where
    F: FnMut(ShapedGlyph) -> Result<(), E>,
{
    let font = resolved.font();
    let face = resolved.face();
    let glyph = resolved.glyph();

    let Some(base_advance) = face.glyph_advance(glyph, size_px) else {
        return Ok(false);
    };

    // logical shaping produces unpositioned glyphs.
    // pair positioning belongs to the final visual glyph stream, after bidi ordering
    // and mirroring are complete.
    let shaped = ShapedGlyph::new(
        font,
        glyph,
        cluster,
        base_advance,
        Offset::ZERO,
        base_advance,
    );

    visit(shaped)?;

    *advance += base_advance;
    *glyph_count = glyph_count.saturating_add(1);

    Ok(true)
}

const fn arabic_contextual_feature(joins_previous: bool, joins_next: bool) -> OpenTypeFeature {
    match (joins_previous, joins_next) {
        (false, false) => ARABIC_ISOL_FEATURE,
        (false, true) => ARABIC_INIT_FEATURE,
        (true, true) => ARABIC_MEDI_FEATURE,
        (true, false) => ARABIC_FINA_FEATURE,
    }
}
