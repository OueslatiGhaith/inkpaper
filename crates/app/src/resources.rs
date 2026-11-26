use inkpaper_ui::RuntimeResources;

use crate::typography::FONT_FACES;

/// Images one reader chapter can show. Later images are left out.
pub(crate) const READER_IMAGE_SLOTS: usize = 32;
/// Dedicated slot for the Home screen's current book cover.
const COVER_IMAGE_SLOTS: usize = 1;
const IMAGE_SLOTS: usize = READER_IMAGE_SLOTS + COVER_IMAGE_SLOTS;

/// UI resources with the font and image capacity this app registers. Platforms
/// only choose the glyph cache budget.
pub type AppResources<'resource, const GLYPH_SLOTS: usize, const GLYPH_BYTES: usize> =
    RuntimeResources<'resource, FONT_FACES, GLYPH_SLOTS, GLYPH_BYTES, IMAGE_SLOTS>;
