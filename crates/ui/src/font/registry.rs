use crate::{FontFamilyId, FontWeight};

use super::{FontFace, FontId, GlyphId};

#[derive(Clone, Copy)]
pub struct ResolvedGlyph<'font> {
    font: FontId,
    face: &'font dyn FontFace,
    glyph: GlyphId,
}

impl<'font> ResolvedGlyph<'font> {
    pub const fn new(font: FontId, face: &'font dyn FontFace, glyph: GlyphId) -> Self {
        Self { font, face, glyph }
    }

    pub const fn font(self) -> FontId {
        self.font
    }

    pub const fn face(self) -> &'font dyn FontFace {
        self.face
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
    weight: FontWeight,
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

        self.register_face(family, FontWeight::NORMAL, font)
    }

    pub fn register_face(
        &mut self,
        family: FontFamilyId,
        weight: FontWeight,
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
            weight,
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

    pub fn resolve_weight(&self, weight: FontWeight) -> Option<(FontId, &'font dyn FontFace)> {
        self.resolve_family_weight(FontFamilyId::DEFAULT, weight)
    }

    pub fn resolve_family_weight(
        &self,
        family: FontFamilyId,
        weight: FontWeight,
    ) -> Option<(FontId, &'font dyn FontFace)> {
        if let Some(resolved) = self.resolve_family_weight_exact(family, weight) {
            return Some(resolved);
        }

        if family != FontFamilyId::DEFAULT
            && let Some(resolved) = self.resolve_family_weight_exact(FontFamilyId::DEFAULT, weight)
        {
            return Some(resolved);
        }

        self.resolve_with_id(FontId::DEFAULT)
    }

    fn resolve_family_weight_exact(
        &self,
        family: FontFamilyId,
        weight: FontWeight,
    ) -> Option<(FontId, &'font dyn FontFace)> {
        let mut best = None;

        for index in 0..self.len {
            let Some(entry) = self.fonts[index] else {
                continue;
            };

            if entry.family != family {
                continue;
            }

            let index = u16::try_from(index).ok()?;
            let id = FontId::new(index);
            let distance = entry.weight.distance(weight);

            if distance == 0 {
                return Some((id, entry.face));
            }

            match best {
                None => best = Some((id, distance)),
                Some((_, best_distance)) if distance < best_distance => best = Some((id, distance)),
                _ => {}
            }
        }

        let (id, _) = best?;
        let face = self.get(id)?;

        Some((id, face))
    }

    fn glyph_in_font(&self, font: FontId, character: char) -> Option<ResolvedGlyph<'font>> {
        let face = self.get(font)?;
        let glyph = face.glyph_id(character)?;

        Some(ResolvedGlyph::new(font, face, glyph))
    }

    pub(crate) fn resolve_character_exact(
        &self,
        preferred: FontId,
        character: char,
    ) -> Option<ResolvedGlyph<'font>> {
        let preferred = self.resolve_id(preferred)?;
        if let Some(glyph) = self.glyph_in_font(preferred, character) {
            return Some(glyph);
        }

        // registration order is the initial fallback order
        // this is deliberately simple. We don't need font-family weight-aware fallback
        // until the app actually requires it
        for index in 0..self.len {
            let index = u16::try_from(index).ok()?;
            let font = FontId::new(index);
            if font == preferred {
                continue;
            }
            if let Some(glyph) = self.glyph_in_font(font, character) {
                return Some(glyph);
            }
        }

        None
    }

    pub fn resolve_glyph(
        &self,
        preferred: FontId,
        character: char,
    ) -> Option<ResolvedGlyph<'font>> {
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
}
