const MAX_CONSECUTIVE_FAST_REFRESHES: u8 = 12;
const LARGE_DAMAGE_PERCENT: u8 = 60;
const CONTINUOUS_TONE_DAMAGE_PERCENT: u8 = 20;

#[derive(Debug, defmt::Format, Clone, Copy, PartialEq, Eq)]
pub enum RefreshRequest {
    Full,
    Fast,
}

#[derive(Debug, defmt::Format, Clone, Copy, PartialEq, Eq)]
pub enum BinaryUpdateMode {
    FullPlane,
    Window,
}

#[derive(Debug, defmt::Format, Clone, Copy, PartialEq, Eq)]
pub enum GrayscaleUpdateMode {
    FullPlane,
    Window,
}

#[derive(Debug, defmt::Format, Clone, Copy, PartialEq, Eq)]
pub struct EInkCapabilities {
    binary_update: BinaryUpdateMode,
    grayscale_update: GrayscaleUpdateMode,
}

impl EInkCapabilities {
    pub const fn new(
        binary_update: BinaryUpdateMode,
        grayscale_update: GrayscaleUpdateMode,
    ) -> Self {
        Self {
            binary_update,
            grayscale_update,
        }
    }

    pub const fn binary_update(self) -> BinaryUpdateMode {
        self.binary_update
    }

    pub const fn grayscale_update(self) -> GrayscaleUpdateMode {
        self.grayscale_update
    }

    pub const fn supports_partial_grayscale(self) -> bool {
        matches!(self.grayscale_update, GrayscaleUpdateMode::Window)
    }

    pub const fn can_binary_update_over_grayscale(self) -> bool {
        matches!(self.binary_update, BinaryUpdateMode::Window)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RefreshContext {
    full_damage: bool,
    continuous_tone_images: bool,
    damaged_pixels: u32,
    total_pixels: u32,
}

impl RefreshContext {
    pub(crate) const fn new(
        full_damage: bool,
        continuous_tone_images: bool,
        damaged_pixels: u32,
        total_pixels: u32,
    ) -> Self {
        Self {
            full_damage,
            continuous_tone_images,
            damaged_pixels,
            total_pixels,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct RefreshPolicy {
    consecutive_fast_refreshes: u8,
}

impl RefreshPolicy {
    pub fn select(&mut self, context: RefreshContext) -> RefreshRequest {
        let request = if context.total_pixels == 0 {
            // a malformed/unknown display size should fail conservatively
            RefreshRequest::Full
        } else if context.full_damage {
            RefreshRequest::Full
        } else if ratio_at_least(
            context.damaged_pixels,
            context.total_pixels,
            LARGE_DAMAGE_PERCENT,
        ) {
            RefreshRequest::Full
        } else if context.continuous_tone_images
            && ratio_at_least(
                context.damaged_pixels,
                context.total_pixels,
                CONTINUOUS_TONE_DAMAGE_PERCENT,
            )
        {
            RefreshRequest::Full
        } else if self.consecutive_fast_refreshes >= MAX_CONSECUTIVE_FAST_REFRESHES {
            RefreshRequest::Full
        } else {
            RefreshRequest::Fast
        };

        self.record(request);

        request
    }

    pub(crate) fn record_full_refresh(&mut self) {
        self.record(RefreshRequest::Full);
    }

    pub(crate) const fn consecutive_fast_refreshes(&self) -> u8 {
        self.consecutive_fast_refreshes
    }

    fn record(&mut self, request: RefreshRequest) {
        match request {
            RefreshRequest::Full => self.consecutive_fast_refreshes = 0,
            RefreshRequest::Fast => {
                self.consecutive_fast_refreshes = self.consecutive_fast_refreshes.saturating_add(1)
            }
        }
    }
}

fn ratio_at_least(part: u32, total: u32, percent: u8) -> bool {
    if total == 0 {
        return false;
    }

    u64::from(part).saturating_mul(100) >= u64::from(total).saturating_mul(u64::from(percent))
}
