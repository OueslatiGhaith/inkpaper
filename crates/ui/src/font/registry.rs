#[cfg(feature = "alloc")]
use alloc::boxed::Box;

use crate::{
    FontFamilyId, FontInstance, FontProperties, FontStyle, FontWeight, FontWeightRange, Pixels,
    PreparedFont, ResolvedFont,
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

enum RegisteredFace<'font> {
    Borrowed(&'font dyn FontFace),
    #[cfg(feature = "alloc")]
    Owned(Box<dyn FontFace>),
}

impl RegisteredFace<'_> {
    fn face(&self) -> &dyn FontFace {
        match self {
            Self::Borrowed(face) => *face,
            #[cfg(feature = "alloc")]
            Self::Owned(face) => face.as_ref(),
        }
    }

    #[cfg(feature = "alloc")]
    const fn is_owned(&self) -> bool {
        matches!(self, Self::Owned(_))
    }
}

struct RegisteredFont<'font> {
    family: FontFamilyId,
    weights: FontWeightRange,
    style: FontStyle,
    face: RegisteredFace<'font>,
}

/// The registered faces, by [`FontId`]. Faces are borrowed for the
/// registry's lifetime, or owned when the `alloc` feature can free them
/// again, so what the registry hands out borrows the registry.
pub struct FontRegistry<'font, const FONTS: usize> {
    fonts: [Option<RegisteredFont<'font>>; FONTS],
    len: usize,
    family_count: usize,
    /// bumped whenever a face goes away, so caches keyed by [`FontId`] know
    /// their entries may belong to another face now
    revision: u16,
}

impl<const FONTS: usize> Default for FontRegistry<'_, FONTS> {
    fn default() -> Self {
        Self {
            fonts: core::array::from_fn(|_| None),
            len: 0,
            family_count: 0,
            revision: 0,
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

    pub(crate) const fn revision(&self) -> u16 {
        self.revision
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
        self.insert(family, weights, RegisteredFace::Borrowed(font))
    }

    /// Registers a face the registry owns, so [`Self::clear_owned_faces`]
    /// can free it again.
    #[cfg(feature = "alloc")]
    pub fn register_owned_face(
        &mut self,
        family: FontFamilyId,
        font: Box<dyn FontFace>,
    ) -> Result<FontId, FontRegistryError> {
        self.insert(family, font.weight_range(), RegisteredFace::Owned(font))
    }

    /// Frees every owned face. Their ids go to the next faces registered.
    #[cfg(feature = "alloc")]
    pub fn clear_owned_faces(&mut self) {
        let mut cleared = false;

        for font in &mut self.fonts {
            if font.as_ref().is_some_and(|font| font.face.is_owned()) {
                *font = None;
                self.len -= 1;
                cleared = true;
            }
        }

        if cleared {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// Fills the first free slot, so ids freed by clearing owned faces are
    /// reused and the borrowed faces keep their ids.
    fn insert(
        &mut self,
        family: FontFamilyId,
        weights: FontWeightRange,
        face: RegisteredFace<'font>,
    ) -> Result<FontId, FontRegistryError> {
        if family.index() >= self.family_count {
            return Err(FontRegistryError::InvalidFamily);
        }

        let index = self
            .fonts
            .iter()
            .position(Option::is_none)
            .ok_or(FontRegistryError::Full)?;
        let id = FontId::new(u16::try_from(index).map_err(|_| FontRegistryError::Full)?);

        self.fonts[index] = Some(RegisteredFont {
            family,
            weights,
            style: face.face().style(),
            face,
        });
        self.len += 1;

        Ok(id)
    }

    fn entry(&self, id: FontId) -> Option<&RegisteredFont<'font>> {
        self.fonts.get(id.index())?.as_ref()
    }

    /// The registered fonts with their ids, in id order.
    fn entries(&self) -> impl Iterator<Item = (FontId, &RegisteredFont<'font>)> {
        self.fonts.iter().enumerate().filter_map(|(index, font)| {
            let font = font.as_ref()?;
            let index = u16::try_from(index).ok()?;

            Some((FontId::new(index), font))
        })
    }

    pub fn get(&self, id: FontId) -> Option<&dyn FontFace> {
        self.entry(id).map(|entry| entry.face.face())
    }

    pub fn default_font(&self) -> Option<&dyn FontFace> {
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

    pub fn resolve_with_id(&self, id: FontId) -> Option<(FontId, &dyn FontFace)> {
        let resolved = self.resolve_id(id)?;
        let font = self.get(resolved)?;

        Some((resolved, font))
    }

    pub fn resolve(&self, id: FontId) -> Option<&dyn FontFace> {
        self.resolve_with_id(id).map(|(_, font)| font)
    }

    pub fn resolve_instance(&self, instance: FontInstance) -> Option<ResolvedFont<'_>> {
        let entry = self.entry(instance.font())?;
        let weight = entry.weights.resolve(instance.weight());

        Some(ResolvedFont::new(
            FontInstance::new(instance.font(), FontProperties::new(weight)),
            entry.face.face(),
        ))
    }

    pub(crate) fn prepare_instance(&self, instance: FontInstance) -> Option<PreparedFont<'_>> {
        self.resolve_instance(instance).map(ResolvedFont::prepare)
    }

    pub fn resolve_weight(&self, weight: FontWeight) -> Option<ResolvedFont<'_>> {
        self.resolve_family_weight(FontFamilyId::DEFAULT, weight)
    }

    pub fn resolve_family_weight(
        &self,
        family: FontFamilyId,
        weight: FontWeight,
    ) -> Option<ResolvedFont<'_>> {
        self.resolve_family_font(family, weight, FontStyle::Normal)
    }

    /// picks the family's face for `style`, falling back to its other faces, then
    /// to the default family
    pub fn resolve_family_font(
        &self,
        family: FontFamilyId,
        weight: FontWeight,
        style: FontStyle,
    ) -> Option<ResolvedFont<'_>> {
        if let Some(resolved) = self.resolve_family_font_exact(family, weight, style) {
            return Some(resolved);
        }

        if family != FontFamilyId::DEFAULT
            && let Some(resolved) =
                self.resolve_family_font_exact(FontFamilyId::DEFAULT, weight, style)
        {
            return Some(resolved);
        }

        let entry = self.entry(FontId::DEFAULT)?;
        let weight = entry.weights.default_weight();

        Some(ResolvedFont::new(
            FontInstance::new(FontId::DEFAULT, FontProperties::new(weight)),
            entry.face.face(),
        ))
    }

    fn resolve_family_font_exact(
        &self,
        family: FontFamilyId,
        requested: FontWeight,
        style: FontStyle,
    ) -> Option<ResolvedFont<'_>> {
        let mut best: Option<(FontId, FontWeight, (bool, u8), u16)> = None;

        for (id, entry) in self.entries() {
            if entry.family != family {
                continue;
            }

            let effective = entry.weights.resolve(requested);
            let distance = entry.weights.distance(requested);

            let weight_rank = if entry.weights.is_exact() && distance == 0 {
                0
            } else if entry.weights.contains(requested) {
                1
            } else {
                2
            };

            // a face in the requested style beats any weight match
            let rank = (entry.style != style, weight_rank);

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
            entry.face.face(),
        ))
    }

    fn glyph_in_font(&self, font: FontInstance, character: char) -> Option<ResolvedGlyph<'_>> {
        let font = self.resolve_instance(font)?;
        let glyph = font.glyph_id(character)?;

        Some(ResolvedGlyph::from_resolved_font(font, glyph))
    }

    fn glyph_with_advance_in_font(
        &self,
        font: FontInstance,
        character: char,
        size_px: u16,
    ) -> Option<(ResolvedGlyph<'_>, Pixels)> {
        let font = self.resolve_instance(font)?;
        let (glyph, advance) = font.glyph_id_and_advance(character, size_px)?;

        Some((ResolvedGlyph::from_resolved_font(font, glyph), advance))
    }

    fn glyph_with_advance_in_font_prepared(
        &self,
        font: FontInstance,
        prepared: Option<&PreparedFont<'_>>,
        character: char,
        size_px: u16,
    ) -> Option<(ResolvedGlyph<'_>, Pixels)> {
        let resolved = self.resolve_instance(font)?;

        let (glyph, advance) = match prepared {
            Some(prepared) if prepared.instance() == resolved.instance() => {
                prepared.glyph_id_and_advance(character, size_px)?
            }

            _ => resolved.glyph_id_and_advance(character, size_px)?,
        };

        Some((ResolvedGlyph::from_resolved_font(resolved, glyph), advance))
    }

    pub(crate) fn resolve_character_exact(
        &self,
        preferred: impl Into<FontInstance>,
        character: char,
    ) -> Option<ResolvedGlyph<'_>> {
        let preferred = preferred.into();

        if let Some(glyph) = self.glyph_in_font(preferred, character) {
            return Some(glyph);
        }

        let requested_weight = preferred.weight();

        // preserve the existing registration-order fallback semantics.
        // each fallback face gets the closest instance it can represent for the originally
        // requested weight.
        for (id, entry) in self.entries() {
            if id == preferred.font() {
                continue;
            }

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
    ) -> Option<(ResolvedGlyph<'_>, Pixels)> {
        let preferred = preferred.into();

        if let Some(glyph) = self.glyph_with_advance_in_font(preferred, character, size_px) {
            return Some(glyph);
        }

        let requested_weight = preferred.weight();

        for (id, entry) in self.entries() {
            if id == preferred.font() {
                continue;
            }

            let effective_weight = entry.weights.resolve(requested_weight);
            let candidate = FontInstance::new(id, FontProperties::new(effective_weight));

            if let Some(glyph) = self.glyph_with_advance_in_font(candidate, character, size_px) {
                return Some(glyph);
            }
        }

        None
    }

    pub(crate) fn resolve_character_with_advance_exact_prepared(
        &self,
        preferred: FontInstance,
        prepared: Option<&PreparedFont<'_>>,
        character: char,
        size_px: u16,
    ) -> Option<(ResolvedGlyph<'_>, Pixels)> {
        if let Some(glyph) =
            self.glyph_with_advance_in_font_prepared(preferred, prepared, character, size_px)
        {
            return Some(glyph);
        }

        let requested_weight = preferred.weight();

        for (id, entry) in self.entries() {
            if id == preferred.font() {
                continue;
            }

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
    ) -> Option<ResolvedGlyph<'_>> {
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
    ) -> Option<(ResolvedGlyph<'_>, Pixels)> {
        self.resolve_glyph_with_advance_prepared(preferred.into(), None, character, size_px)
    }

    pub(crate) fn resolve_glyph_with_advance_prepared(
        &self,
        preferred: FontInstance,
        prepared: Option<&PreparedFont<'_>>,
        character: char,
        size_px: u16,
    ) -> Option<(ResolvedGlyph<'_>, Pixels)> {
        if let Some(glyph) = self
            .resolve_character_with_advance_exact_prepared(preferred, prepared, character, size_px)
        {
            return Some(glyph);
        }

        if character != '\u{FFFD}'
            && let Some(glyph) = self.resolve_character_with_advance_exact_prepared(
                preferred, prepared, '\u{FFFD}', size_px,
            )
        {
            return Some(glyph);
        }

        if character != '?'
            && let Some(glyph) = self
                .resolve_character_with_advance_exact_prepared(preferred, prepared, '?', size_px)
        {
            return Some(glyph);
        }

        None
    }
}
