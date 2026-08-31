use crate::{FontRegistry, GlyphMetrics, Offset, Pixels, px};

use super::{ShapedGlyph, arabic::MarkPlacement, bidi::DirectionalRun};

const MARK_GAP: Pixels = px(1);

pub(super) fn apply_visual_positioning<'font, const FONTS: usize>(
    registry: &FontRegistry<'font, FONTS>,
    size_px: u16,
    glyphs: &mut [ShapedGlyph],
    runs: &[DirectionalRun],
) -> Pixels {
    let mut advance = px(0);

    for run in runs.iter().copied() {
        debug_assert!(run.start <= run.end);
        debug_assert!(run.end <= glyphs.len());

        if run.start >= run.end || run.end > glyphs.len() {
            continue;
        }

        advance += apply_visual_run_positioning(
            registry,
            size_px,
            &mut glyphs[run.start..run.end],
            run.level % 2 == 1,
        );
    }

    advance
}

fn apply_visual_run_positioning<'font, const FONTS: usize>(
    registry: &FontRegistry<'font, FONTS>,
    size_px: u16,
    glyphs: &mut [ShapedGlyph],
    right_to_left: bool,
) -> Pixels {
    let mut previous_base: Option<ShapedGlyph> = None;
    let mut visual_advance = px(0);
    let mut index = 0usize;

    while index < glyphs.len() {
        let shaped = glyphs[index];

        // marks are positioned after their base below.
        if let Some(placement) = shaped.mark_placement() {
            glyphs[index] = ShapedGlyph::new_mark(
                shaped.font(),
                shaped.glyph(),
                shaped.cluster(),
                shaped.base_advance(),
                placement,
                Offset::ZERO,
            );

            index = index.saturating_add(1);
            continue;
        }

        let cursive = match previous_base {
            Some(previous) if previous.font() == shaped.font() => {
                registry.get(shaped.font()).and_then(|face| {
                    face.cursive_attachment(
                        previous.glyph(),
                        shaped.glyph(),
                        size_px,
                        right_to_left,
                    )
                })
            }

            _ => None,
        };

        let (x_adjustment, y_offset) = match (previous_base, cursive) {
            (Some(previous), Some(attachment)) => {
                let delta = attachment.origin_delta();

                // before pair positioning, the visual-right origin would be one intrinsic
                // advance after the previous glyph's origin.
                // convert the desired absolute origin distance into our existing
                // per-glyph offset/advance refinement.
                (
                    delta.x - previous.base_advance(),
                    previous.offset().y + delta.y,
                )
            }
            _ => {
                let kerning = match previous_base {
                    Some(previous) if previous.font() == shaped.font() => registry
                        .get(shaped.font())
                        .map(|face| face.kerning(previous.glyph(), shaped.glyph(), size_px))
                        .unwrap_or(px(0)),

                    _ => px(0),
                };

                (kerning, px(0))
            }
        };

        // cursive attachment takes precedence over ordinary pair kerning for a pair
        // because the anchors define an exact connection geometry.
        let positioned_base = shaped.with_positioning(
            Offset::new(x_adjustment, y_offset),
            shaped.base_advance() + x_adjustment,
        );

        glyphs[index] = positioned_base;
        visual_advance += positioned_base.advance();

        let mut mark_end = index.saturating_add(1);

        while mark_end < glyphs.len() {
            let candidate = glyphs[mark_end];
            if candidate.cluster() != positioned_base.cluster()
                || candidate.mark_placement().is_none()
            {
                break;
            }

            mark_end = mark_end.saturating_add(1);
        }

        position_cluster_marks(
            registry,
            size_px,
            positioned_base,
            &mut glyphs[index.saturating_add(1)..mark_end],
        );

        previous_base = Some(positioned_base);
        index = mark_end;
    }

    visual_advance
}

fn position_cluster_marks<'font, const FONTS: usize>(
    registry: &FontRegistry<'font, FONTS>,
    size_px: u16,
    base: ShapedGlyph,
    marks: &mut [ShapedGlyph],
) {
    if marks.is_empty() {
        return;
    }
    if position_cluster_marks_with_font_anchors(registry, size_px, base, marks) {
        return;
    }

    position_cluster_marks_with_metrics(registry, size_px, base, marks);
}

fn position_cluster_marks_with_font_anchors<'font, const FONTS: usize>(
    registry: &FontRegistry<'font, FONTS>,
    size_px: u16,
    base: ShapedGlyph,
    marks: &mut [ShapedGlyph],
) -> bool {
    let mut previous_mark: Option<ShapedGlyph> = None;

    for mark in marks.iter_mut() {
        let shaped = *mark;
        let Some(placement) = shaped.mark_placement() else {
            return false;
        };

        let mut attachment = None;

        // prefer mark-to-mark when another mark in this cluster has already been positioned.
        if let Some(parent) = previous_mark
            && parent.font() == shaped.font()
        {
            attachment = registry
                .get(shaped.font())
                .and_then(|face| face.mark_to_mark_offset(parent.glyph(), shaped.glyph(), size_px))
                .map(|offset| {
                    Offset::new(parent.offset().x + offset.x, parent.offset().y + offset.y)
                });
        }

        // our current ligature producer is lam-alef.
        // a mark encountered after the consumed alef belongs to the final logical
        // component of that ligature.
        if attachment.is_none()
            && base.font() == shaped.font()
            && let Some(component) = base.final_ligature_component()
        {
            attachment = registry
                .get(shaped.font())
                .and_then(|face| {
                    face.mark_to_ligature_offset(base.glyph(), component, shaped.glyph(), size_px)
                })
                .map(|offset| {
                    Offset::new(offset.x - base.base_advance(), base.offset().y + offset.y)
                });
        }

        // ordinary single-component base.
        if attachment.is_none() && base.font() == shaped.font() {
            attachment = registry
                .get(shaped.font())
                .and_then(|face| face.mark_to_base_offset(base.glyph(), shaped.glyph(), size_px))
                .map(|offset| {
                    Offset::new(offset.x - base.base_advance(), base.offset().y + offset.y)
                });
        }

        let Some(offset) = attachment else {
            // keep the cluster internally consistent:
            // if any mark cannot use font anchors, the caller will replace the whole
            // cluster with metric positioning.
            return false;
        };

        let positioned = ShapedGlyph::new_mark(
            shaped.font(),
            shaped.glyph(),
            shaped.cluster(),
            shaped.base_advance(),
            placement,
            offset,
        );

        *mark = positioned;
        previous_mark = Some(positioned);
    }

    true
}

fn position_cluster_marks_with_metrics<'font, const FONTS: usize>(
    registry: &FontRegistry<'font, FONTS>,
    size_px: u16,
    base: ShapedGlyph,
    marks: &mut [ShapedGlyph],
) {
    let fallback_offset = Offset::new(Pixels::ZERO - base.base_advance(), base.offset().y);

    let Some(base_metrics) = registry
        .get(base.font())
        .and_then(|face| face.glyph_metrics(base.glyph(), size_px))
    else {
        for mark in marks {
            let shaped = *mark;

            let Some(placement) = shaped.mark_placement() else {
                continue;
            };

            *mark = ShapedGlyph::new_mark(
                shaped.font(),
                shaped.glyph(),
                shaped.cluster(),
                shaped.base_advance(),
                placement,
                fallback_offset,
            );
        }

        return;
    };

    let base_height = px(i32::from(base_metrics.height));
    let mut above_edge = base_metrics.bearing_y;
    let mut below_edge = base_metrics.bearing_y + base_height;

    // shadda stays nearest to the base even when canonical Unicode ordering places
    // another above-base mark first.
    for target in [
        MarkPlacement::Shadda,
        MarkPlacement::Above,
        MarkPlacement::Below,
    ] {
        for mark in marks.iter_mut() {
            let shaped = *mark;
            if shaped.mark_placement() != Some(target) {
                continue;
            }

            let mark_metrics = registry
                .get(shaped.font())
                .and_then(|face| face.glyph_metrics(shaped.glyph(), size_px));

            let offset = match mark_metrics {
                Some(mark_metrics) => {
                    let edge = match target {
                        MarkPlacement::Shadda | MarkPlacement::Above => &mut above_edge,
                        MarkPlacement::Below => &mut below_edge,
                    };

                    positioned_mark_offset(base, base_metrics, mark_metrics, target, edge)
                }

                None => fallback_offset,
            };

            *mark = ShapedGlyph::new_mark(
                shaped.font(),
                shaped.glyph(),
                shaped.cluster(),
                shaped.base_advance(),
                target,
                offset,
            );
        }
    }
}

fn positioned_mark_offset(
    base: ShapedGlyph,
    base_metrics: GlyphMetrics,
    mark_metrics: GlyphMetrics,
    placement: MarkPlacement,
    edge: &mut Pixels,
) -> Offset {
    let base_width = px(i32::from(base_metrics.width));
    let mark_width = px(i32::from(mark_metrics.width));
    let mark_height = px(i32::from(mark_metrics.height));

    // the renderer reaches the mark after it has already advanced past the base. Move back
    // by the base's intrinsic advance, then center the mark over the base's actual ink bounds.
    // pair kerning on the base cancels naturally because it shifts both the drawn base
    // and the post-base pen by the same amount.
    let x = base_metrics.bearing_x + (base_width - mark_width) / 2
        - mark_metrics.bearing_x
        - base.base_advance();

    let relative_y = match placement {
        MarkPlacement::Shadda | MarkPlacement::Above => {
            let bottom = *edge - MARK_GAP;
            let top = bottom - mark_height;

            *edge = top;

            top - mark_metrics.bearing_y
        }

        MarkPlacement::Below => {
            let top = *edge + MARK_GAP;

            *edge = top + mark_height;

            top - mark_metrics.bearing_y
        }
    };

    Offset::new(x, base.offset().y + relative_y)
}
