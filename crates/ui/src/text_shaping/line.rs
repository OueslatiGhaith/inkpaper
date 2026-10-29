//! shapes a line of any length through a fixed chunk of glyphs.
//!
//! a line's directional runs come from one pass over its glyphs, which keeps none of
//! them. Each run then shapes again, in visual order: a left-to-right run streams
//! forward through the chunk, and a right-to-left run fills the chunk from its end, one
//! pass per chunk. Chunks always hold whole clusters, so a run positions the same
//! whether it fits one chunk or many.

use core::convert::Infallible;

use crate::{FontRegistry, ShapeSummary, pair_cache::PairPositioningCache, px};

use super::{
    PreparedSimpleShaper, ShapeError, ShapeState, ShapedGlyph,
    bidi::{
        TextRunBuilder, mirror_clusters, paragraph_direction, piece_cluster, reverse_glyph_clusters,
    },
    positioning::RunPositioner,
};

/// the most glyphs a line holds at once. A line may have any number, but each cluster,
/// a base and its marks, must fit
const LINE_CHUNK_GLYPHS: usize = 64;

impl<'font> PreparedSimpleShaper<'font> {
    /// shapes `text` followed by `suffix`, such as an ellipsis, and visits the positioned
    /// glyphs in visual order. The glyphs are the ones [`Self::shape_into`] stores, but
    /// the line can be any length
    pub fn shape_line_with<const FONTS: usize>(
        &self,
        registry: &'font FontRegistry<'_, FONTS>,
        size_px: u16,
        text: &str,
        suffix: &str,
        visit: impl FnMut(ShapedGlyph),
    ) -> Result<ShapeSummary, ShapeError> {
        self.shape_line_in_chunks::<LINE_CHUNK_GLYPHS, FONTS, 1>(
            registry, size_px, text, suffix, None, visit,
        )
    }

    /// [`Self::shape_line_with`], looking pair positioning up in `pair_positioning_cache`
    pub(crate) fn shape_line_with_pair_positioning_cache<const FONTS: usize, const SLOTS: usize>(
        &self,
        registry: &'font FontRegistry<'_, FONTS>,
        size_px: u16,
        text: &str,
        suffix: &str,
        pair_positioning_cache: &mut PairPositioningCache<SLOTS>,
        visit: impl FnMut(ShapedGlyph),
    ) -> Result<ShapeSummary, ShapeError> {
        self.shape_line_in_chunks::<LINE_CHUNK_GLYPHS, FONTS, SLOTS>(
            registry,
            size_px,
            text,
            suffix,
            Some(pair_positioning_cache),
            visit,
        )
    }

    /// [`Self::shape_line_with`] through chunks of `CHUNK` glyphs
    pub(super) fn shape_line_in_chunks<
        const CHUNK: usize,
        const FONTS: usize,
        const SLOTS: usize,
    >(
        &self,
        registry: &'font FontRegistry<'_, FONTS>,
        size_px: u16,
        text: &str,
        suffix: &str,
        mut pair_positioning_cache: Option<&mut PairPositioningCache<SLOTS>>,
        mut visit: impl FnMut(ShapedGlyph),
    ) -> Result<ShapeSummary, ShapeError> {
        let line = Line {
            shaper: self,
            registry,
            size_px,
            text,
            suffix,
        };

        let mut builder = TextRunBuilder::new(text, paragraph_direction(text));
        if !builder.is_single_run() {
            let mut result = Ok(());
            line.for_each_glyph(|glyph| {
                if result.is_ok() {
                    result = builder.push(glyph.cluster());
                }
            });
            result?;
        }
        let runs = builder.finish();

        let (order, run_count) = runs.visual_order();
        let mut glyph_count = 0usize;
        let mut advance = px(0);

        for &index in &order[..run_count] {
            let index = usize::from(index);
            let range = runs.range(index);
            let right_to_left = runs.level(index) % 2 == 1;
            let mut positioner = RunPositioner::new();

            let mut emit = |chunk: &mut [ShapedGlyph]| {
                if right_to_left {
                    mirror_clusters(registry, size_px, text, chunk);
                    reverse_glyph_clusters(chunk);
                }

                advance += positioner.position(
                    registry,
                    size_px,
                    chunk,
                    right_to_left,
                    self.prepared_font.as_ref(),
                    pair_positioning_cache.as_deref_mut(),
                );

                for glyph in chunk.iter() {
                    visit(glyph.with_cluster(piece_cluster(text, glyph.cluster())));
                }
                glyph_count += chunk.len();
            };

            if right_to_left {
                line.chunks_from_end::<CHUNK>(range, &mut emit)?;
            } else {
                line.chunks_from_start::<CHUNK>(range, &mut emit)?;
            }
        }

        Ok(ShapeSummary::new(glyph_count, advance))
    }
}

struct Line<'shaper, 'font, 'registry, 'text, const FONTS: usize> {
    shaper: &'shaper PreparedSimpleShaper<'font>,
    registry: &'font FontRegistry<'registry, FONTS>,
    size_px: u16,
    text: &'text str,
    suffix: &'text str,
}

impl<const FONTS: usize> Line<'_, '_, '_, '_, FONTS> {
    /// shapes the line's glyphs in logical order, with their line offsets as clusters
    fn for_each_glyph(&self, mut visit: impl FnMut(ShapedGlyph)) {
        let mut state = ShapeState::new();
        let suffix_start = self.text.len();

        for (piece, offset) in [(self.text, 0), (self.suffix, suffix_start)] {
            let result = self.shaper.shaper.try_shape_piece_with_prepared_font(
                self.registry,
                self.shaper.preferred_font,
                self.shaper.prepared_font.as_ref(),
                self.size_px,
                piece,
                &mut state,
                |glyph| {
                    visit(glyph.with_cluster(offset + glyph.cluster()));
                    Ok::<(), Infallible>(())
                },
            );

            match result {
                Ok(_) => {}
                Err(error) => match error {},
            }
        }
    }

    /// visits the glyphs of the run at `range` in logical order, a chunk of whole
    /// clusters at a time
    fn chunks_from_start<const CHUNK: usize>(
        &self,
        range: core::ops::Range<usize>,
        emit: &mut impl FnMut(&mut [ShapedGlyph]),
    ) -> Result<(), ShapeError> {
        let mut chunk = [ShapedGlyph::EMPTY; CHUNK];
        let mut len = 0usize;
        let mut result = Ok(());

        self.for_each_glyph(|glyph| {
            if result.is_err() || !range.contains(&glyph.cluster()) {
                return;
            }

            if len == CHUNK {
                // keep the cluster `glyph` continues for the next chunk
                let keep = if chunk[len - 1].cluster() == glyph.cluster() {
                    cluster_start(&chunk[..len])
                } else {
                    len
                };

                if keep == 0 {
                    result = Err(ShapeError::BufferTooSmall);
                    return;
                }

                emit(&mut chunk[..keep]);
                chunk.copy_within(keep..len, 0);
                len -= keep;
            }

            chunk[len] = glyph;
            len += 1;
        });

        result?;

        if len > 0 {
            emit(&mut chunk[..len]);
        }

        Ok(())
    }

    /// visits the glyphs of the run at `range` from its end, each chunk of whole
    /// clusters still in logical order. Each chunk shapes the run again up to where the
    /// previous chunk began, keeping the last glyphs before it
    fn chunks_from_end<const CHUNK: usize>(
        &self,
        range: core::ops::Range<usize>,
        emit: &mut impl FnMut(&mut [ShapedGlyph]),
    ) -> Result<(), ShapeError> {
        // how many of the run's glyphs come before the chunks visited so far
        let mut remaining = None::<usize>;

        loop {
            let mut ring = [ShapedGlyph::EMPTY; CHUNK];
            let mut len = 0usize;
            let mut head = 0usize;
            let mut before_ring = None::<usize>;
            let mut seen = 0usize;

            self.for_each_glyph(|glyph| {
                if !range.contains(&glyph.cluster()) || remaining.is_some_and(|end| seen >= end) {
                    return;
                }

                seen += 1;

                if len < CHUNK {
                    ring[len] = glyph;
                    len += 1;
                } else {
                    before_ring = Some(ring[head].cluster());
                    ring[head] = glyph;
                    head = (head + 1) % CHUNK;
                }
            });

            if len == 0 {
                return Ok(());
            }

            let ring = &mut ring[..len];
            ring.rotate_left(head);

            // the cluster that began before the ring goes with the next chunk
            let start = match before_ring {
                Some(cluster) => ring
                    .iter()
                    .position(|glyph| glyph.cluster() != cluster)
                    .ok_or(ShapeError::BufferTooSmall)?,
                None => 0,
            };

            emit(&mut ring[start..]);

            let next = seen - (len - start);
            if next == 0 {
                return Ok(());
            }
            remaining = Some(next);
        }
    }
}

/// where the last cluster of `glyphs` starts
fn cluster_start(glyphs: &[ShapedGlyph]) -> usize {
    let Some(last) = glyphs.last() else {
        return 0;
    };

    glyphs
        .iter()
        .rposition(|glyph| glyph.cluster() != last.cluster())
        .map_or(0, |index| index + 1)
}
