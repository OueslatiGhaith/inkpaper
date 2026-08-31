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
}

#[derive(Clone, Copy)]
pub struct FontRegistry<'font, const FONTS: usize> {
    fonts: [Option<&'font dyn FontFace>; FONTS],
    len: usize,
}

impl<const FONTS: usize> Default for FontRegistry<'_, FONTS> {
    fn default() -> Self {
        Self {
            fonts: [None; FONTS],
            len: 0,
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

    pub fn register(&mut self, font: &'font dyn FontFace) -> Result<FontId, FontRegistryError> {
        if self.len >= FONTS {
            return Err(FontRegistryError::Full);
        }

        let index = u16::try_from(self.len).map_err(|_| FontRegistryError::Full)?;

        self.fonts[self.len] = Some(font);
        self.len += 1;

        Ok(FontId::new(index))
    }

    pub fn get(&self, id: FontId) -> Option<&'font dyn FontFace> {
        self.fonts.get(id.index()).copied().flatten()
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
