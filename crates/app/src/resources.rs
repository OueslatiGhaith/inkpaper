use inkpaper_ui::RuntimeResources;

use crate::typography::FONT_FACES;

/// Images one reader chapter can show. Later images are left out.
const IMAGE_SLOTS: usize = 32;

/// UI resources with the font and image capacity this app registers. Platforms
/// only choose the glyph cache budget.
pub type AppResources<'resource, const GLYPH_SLOTS: usize, const GLYPH_BYTES: usize> =
    RuntimeResources<'resource, FONT_FACES, GLYPH_SLOTS, GLYPH_BYTES, IMAGE_SLOTS>;
