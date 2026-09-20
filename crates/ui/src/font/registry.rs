use inkpaper_trace::{TraceMetric, profile_metric_scope};

use crate::{
    FontFamilyId, FontInstance, FontProperties, FontWeight, FontWeightRange, Pixels, ResolvedFont,
};

use super::{FontFace, FontId, GlyphId};

#[derive(Clone, Copy)]
pub struct ResolvedGlyph<'font> {
    font: ResolvedFont<'font>,
    glyph: GlyphId,
}

impl<'font> ResolvedGlyph<'font> {
    pub const fn new(font: FontId, face: &'font dyn FontFace, glyph: GlyphId) -> Self {
        Self {
            font: ResolvedFont::new(FontInstance::normal(font), face),
            glyph,
        }
    }

    pub(crate) const fn from_resolved_font(font: ResolvedFont<'font>, glyph: GlyphId) -> Self {
        Self { font, glyph }
    }

    pub const fn font(self) -> FontId {
        self.font.id()
    }

    pub const fn font_instance(self) -> FontInstance {
        self.font.instance()
    }

    pub const fn resolved_font(self) -> ResolvedFont<'font> {
        self.font
    }

    pub const fn face(self) -> &'font dyn FontFace {
        self.font.face()
    }

    pub const fn glyph(self) -> GlyphId {
        self.glyph
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum FontRegistryError {
    Full,
    TooManyFamilies,
    InvalidFamily,
}

#[derive(Clone, Copy)]
struct RegisteredFont<'font> {
    family: FontFamilyId,
    weights: FontWeightRange,
    face: &'font dyn FontFace,
}

#[derive(Clone, Copy)]
pub struct FontRegistry<'font, const FONTS: usize> {
    fonts: [Option<RegisteredFont<'font>>; FONTS],
    len: usize,
    family_count: usize,
}

impl<const FONTS: usize> Default for FontRegistry<'_, FONTS> {
    fn default() -> Self {
        Self {
            fonts: [None; FONTS],
            len: 0,
            family_count: 0,
        }
    }
}

impl<'font, const FONTS: usize> FontRegistry<'font, FONTS> {
    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn capacity(&self) -> usize {
        FONTS
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub const fn family_count(&self) -> usize {
        self.family_count
    }

    pub fn register_family(&mut self) -> Result<FontFamilyId, FontRegistryError> {
        let index =
            u16::try_from(self.family_count).map_err(|_| FontRegistryError::TooManyFamilies)?;

        self.family_count += 1;

        Ok(FontFamilyId::new(index))
    }

    pub fn register(&mut self, font: &'font dyn FontFace) -> Result<FontId, FontRegistryError> {
        let family = if self.family_count == 0 {
            self.register_family()?
        } else {
            FontFamilyId::DEFAULT
        };

        self.register_face(family, font)
    }

    pub fn register_face(
        &mut self,
        family: FontFamilyId,
        font: &'font dyn FontFace,
    ) -> Result<FontId, FontRegistryError> {
        self.register_face_with_range(family, font.weight_range(), font)
    }

    pub fn register_face_with_weight(
        &mut self,
        family: FontFamilyId,
        weight: FontWeight,
        font: &'font dyn FontFace,
    ) -> Result<FontId, FontRegistryError> {
        self.register_face_with_range(family, FontWeightRange::exact(weight), font)
    }

    pub fn register_face_with_range(
        &mut self,
        family: FontFamilyId,
        weights: FontWeightRange,
        font: &'font dyn FontFace,
    ) -> Result<FontId, FontRegistryError> {
        if family.index() >= self.family_count {
            return Err(FontRegistryError::InvalidFamily);
        }

        if self.len >= FONTS {
            return Err(FontRegistryError::Full);
        }

        let index = u16::try_from(self.len).map_err(|_| FontRegistryError::Full)?;
        let id = FontId::new(index);

        self.fonts[self.len] = Some(RegisteredFont {
            family,
            weights,
            face: font,
        });
        self.len += 1;

        Ok(id)
    }

    fn entry(&self, id: FontId) -> Option<RegisteredFont<'font>> {
        self.fonts.get(id.index()).copied().flatten()
    }

    pub fn get(&self, id: FontId) -> Option<&'font dyn FontFace> {
        self.entry(id).map(|entry| entry.face)
    }

    pub fn default_font(&self) -> Option<&'font dyn FontFace> {
        self.get(FontId::DEFAULT)
    }

    pub fn resolve_id(&self, id: FontId) -> Option<FontId> {
        if self.get(id).is_some() {
            Some(id)
        } else if self.default_font().is_some() {
            Some(FontId::DEFAULT)
        } else {
            None
        }
    }

    pub fn resolve_with_id(&self, id: FontId) -> Option<(FontId, &'font dyn FontFace)> {
        let resolved = self.resolve_id(id)?;
        let font = self.get(resolved)?;

        Some((resolved, font))
    }

    pub fn resolve(&self, id: FontId) -> Option<&'font dyn FontFace> {
        self.resolve_with_id(id).map(|(_, font)| font)
    }

    pub fn resolve_instance(&self, instance: FontInstance) -> Option<ResolvedFont<'font>> {
        profile_metric_scope!(TraceMetric::FontResolve);

        let entry = self.entry(instance.font())?;
        let weight = entry.weights.resolve(instance.weight());

        Some(ResolvedFont::new(
            FontInstance::new(instance.font(), FontProperties::new(weight)),
            entry.face,
        ))
    }

    pub fn resolve_weight(&self, weight: FontWeight) -> Option<ResolvedFont<'font>> {
        self.resolve_family_weight(FontFamilyId::DEFAULT, weight)
    }

    pub fn resolve_family_weight(
        &self,
        family: FontFamilyId,
        weight: FontWeight,
    ) -> Option<ResolvedFont<'font>> {
        if let Some(resolved) = self.resolve_family_weight_exact(family, weight) {
            return Some(resolved);
        }

        if family != FontFamilyId::DEFAULT
            && let Some(resolved) = self.resolve_family_weight_exact(FontFamilyId::DEFAULT, weight)
        {
            return Some(resolved);
        }

        let entry = self.entry(FontId::DEFAULT)?;
        let weight = entry.weights.default_weight();

        Some(ResolvedFont::new(
            FontInstance::new(FontId::DEFAULT, FontProperties::new(weight)),
            entry.face,
        ))
    }

    fn resolve_family_weight_exact(
        &self,
        family: FontFamilyId,
        requested: FontWeight,
    ) -> Option<ResolvedFont<'font>> {
        let mut best: Option<(FontId, FontWeight, u8, u16)> = None;

        for index in 0..self.len {
            let Some(entry) = self.fonts[index] else {
                continue;
            };

            if entry.family != family {
                continue;
            }

            let index = u16::try_from(index).ok()?;
            let id = FontId::new(index);

            let effective = entry.weights.resolve(requested);
            let distance = entry.weights.distance(requested);

            let rank = if entry.weights.is_exact() && distance == 0 {
                0
            } else if entry.weights.contains(requested) {
                1
            } else {
                2
            };

            let replace = match best {
                None => true,

                Some((_, _, best_rank, best_distance)) => {
                    rank < best_rank || (rank == best_rank && distance < best_distance)
                }
            };

            if replace {
                best = Some((id, effective, rank, distance));
            }
        }

        let (id, effective, _, _) = best?;
        let entry = self.entry(id)?;

        Some(ResolvedFont::new(
            FontInstance::new(id, FontProperties::new(effective)),
            entry.face,
        ))
    }

    fn glyph_in_font(&self, font: FontInstance, character: char) -> Option<ResolvedGlyph<'font>> {
        let font = self.resolve_instance(font)?;
        let glyph = font.glyph_id(character)?;

        Some(ResolvedGlyph::from_resolved_font(font, glyph))
    }

    fn glyph_with_advance_in_font(
        &self,
        font: FontInstance,
        character: char,
        size_px: u16,
    ) -> Option<(ResolvedGlyph<'font>, Pixels)> {
        let font = self.resolve_instance(font)?;
        let (glyph, advance) = font.glyph_id_and_advance(character, size_px)?;

        Some((ResolvedGlyph::from_resolved_font(font, glyph), advance))
    }

    pub(crate) fn resolve_character_exact(
        &self,
        preferred: impl Into<FontInstance>,
        character: char,
    ) -> Option<ResolvedGlyph<'font>> {
        let preferred = preferred.into();

        if let Some(glyph) = self.glyph_in_font(preferred, character) {
            return Some(glyph);
        }

        let requested_weight = preferred.weight();

        // preserve the existing registration-order fallback semantics.
        // each fallback face gets the closest instance it can represent for the originally
        // requested weight.
        for index in 0..self.len {
            let index = u16::try_from(index).ok()?;
            let id = FontId::new(index);

            if id == preferred.font() {
                continue;
            }

            let Some(entry) = self.entry(id) else {
                continue;
            };

            let effective_weight = entry.weights.resolve(requested_weight);
            let candidate = FontInstance::new(id, FontProperties::new(effective_weight));

            if let Some(glyph) = self.glyph_in_font(candidate, character) {
                return Some(glyph);
            }
        }

        None
    }

    pub(crate) fn resolve_character_with_advance_exact(
        &self,
        preferred: impl Into<FontInstance>,
        character: char,
        size_px: u16,
    ) -> Option<(ResolvedGlyph<'font>, Pixels)> {
        let preferred = preferred.into();

        if let Some(glyph) = self.glyph_with_advance_in_font(preferred, character, size_px) {
            return Some(glyph);
        }

        let requested_weight = preferred.weight();

        for index in 0..self.len {
            let index = u16::try_from(index).ok()?;
            let id = FontId::new(index);

            if id == preferred.font() {
                continue;
            }

            let Some(entry) = self.entry(id) else {
                continue;
            };

            let effective_weight = entry.weights.resolve(requested_weight);
            let candidate = FontInstance::new(id, FontProperties::new(effective_weight));

            if let Some(glyph) = self.glyph_with_advance_in_font(candidate, character, size_px) {
                return Some(glyph);
            }
        }

        None
    }

    pub fn resolve_glyph(
        &self,
        preferred: impl Into<FontInstance>,
        character: char,
    ) -> Option<ResolvedGlyph<'font>> {
        let preferred = preferred.into();

        if let Some(glyph) = self.resolve_character_exact(preferred, character) {
            return Some(glyph);
        }

        // prefer the Unicode replacement character if any reigstered face contains it
        if character != '\u{FFFD}'
            && let Some(glyph) = self.resolve_character_exact(preferred, '\u{FFFD}')
        {
            return Some(glyph);
        }

        // small bitmap fonts commonly contain '?' but not U+FFFD
        if character != '?'
            && let Some(glyph) = self.resolve_character_exact(preferred, '?')
        {
            return Some(glyph);
        }

        None
    }

    pub(crate) fn resolve_glyph_with_advance(
        &self,
        preferred: impl Into<FontInstance>,
        character: char,
        size_px: u16,
    ) -> Option<(ResolvedGlyph<'font>, Pixels)> {
        let preferred = preferred.into();

        if let Some(glyph) =
            self.resolve_character_with_advance_exact(preferred, character, size_px)
        {
            return Some(glyph);
        }

        if character != '\u{FFFD}'
            && let Some(glyph) =
                self.resolve_character_with_advance_exact(preferred, '\u{FFFD}', size_px)
        {
            return Some(glyph);
        }

        if character != '?'
            && let Some(glyph) = self.resolve_character_with_advance_exact(preferred, '?', size_px)
        {
            return Some(glyph);
        }

        None
    }
}
