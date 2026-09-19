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
pub enum BinaryOverGrayMode {
    Unsupported,
    NativeWindow,
    PreconditionedWindow,
}

#[derive(Debug, defmt::Format, Clone, Copy, PartialEq, Eq)]
pub enum GrayscaleUpdateMode {
    FullPlane,
    Window,
}

#[derive(Debug, defmt::Format, Clone, Copy, PartialEq, Eq)]
pub enum RefreshContent {
    Binary {
        /// a valid binary frame is already established on the panel, so a differential
        /// FAST transition may be used even when the new damage covers most or all
        /// of the display
        differential_eligible: bool,
    },
    Grayscale {
        /// the panel already contains a valid grayscale frame and the controller supports
        /// a damage-scoped grayscale transition
        window_eligible: bool,
    },
}

#[derive(Debug, defmt::Format, Clone, Copy, PartialEq, Eq)]
pub struct EInkCapabilities {
    binary_update: BinaryUpdateMode,
    binary_over_gray: BinaryOverGrayMode,
    grayscale_update: GrayscaleUpdateMode,
}

impl EInkCapabilities {
    pub const fn new(
        binary_update: BinaryUpdateMode,
        binary_over_gray: BinaryOverGrayMode,
        grayscale_update: GrayscaleUpdateMode,
    ) -> Self {
        Self {
            binary_update,
            binary_over_gray,
            grayscale_update,
        }
    }

    pub const fn binary_update(self) -> BinaryUpdateMode {
        self.binary_update
    }

    pub const fn binary_over_gray(self) -> BinaryOverGrayMode {
        self.binary_over_gray
    }

    pub const fn grayscale_update(self) -> GrayscaleUpdateMode {
        self.grayscale_update
    }

    pub const fn supports_partial_grayscale(self) -> bool {
        matches!(self.grayscale_update, GrayscaleUpdateMode::Window)
    }

    pub const fn can_binary_update_over_grayscale(self) -> bool {
        !matches!(self.binary_over_gray, BinaryOverGrayMode::Unsupported)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RefreshContext {
    content: RefreshContent,
    full_damage: bool,
    continuous_tone_images: bool,
    damaged_pixels: u32,
    total_pixels: u32,
}

impl RefreshContext {
    pub(crate) const fn new(
        content: RefreshContent,
        full_damage: bool,
        continuous_tone_images: bool,
        damaged_pixels: u32,
        total_pixels: u32,
    ) -> Self {
        Self {
            content,
            full_damage,
            continuous_tone_images,
            damaged_pixels,
            total_pixels,
        }
    }

    const fn binary_differential_eligible(self) -> bool {
        matches!(
            self.content,
            RefreshContent::Binary {
                differential_eligible: true,
            }
        )
    }

    const fn grayscale_requires_full(self) -> bool {
        matches!(
            self.content,
            RefreshContent::Grayscale {
                window_eligible: false,
            }
        )
    }

    const fn grayscale_window_eligible(self) -> bool {
        matches!(
            self.content,
            RefreshContent::Grayscale {
                window_eligible: true,
            }
        )
    }
}

#[derive(Debug, Default)]
pub(crate) struct RefreshPolicy {
    consecutive_fast_refreshes: u8,
}

impl RefreshPolicy {
    pub fn select(&mut self, context: RefreshContext) -> RefreshRequest {
        let request = if context.total_pixels == 0 {
            // a malformed/unknown display size should fail conservatively.
            RefreshRequest::Full
        } else if context.grayscale_requires_full() {
            // this covers:
            // - the first grayscale frame after binary content;
            // - controllers without damage-scoped grayscale updates.
            RefreshRequest::Full
        } else if self.consecutive_fast_refreshes >= MAX_CONSECUTIVE_FAST_REFRESHES {
            // periodically re-establish the panel using the complete waveform, regardless
            // of whether the current content is binary or grayscale.
            RefreshRequest::Full
        } else if context.binary_differential_eligible() {
            // DTM1/controller state already represents the preceding binary frame.
            // A complete repaint does not require a complete waveform:
            // the controller can transition previous -> current with FAST.
            RefreshRequest::Fast
        } else if context.grayscale_window_eligible() {
            // once a valid grayscale baseline exists, large/full Gray4 damage is still
            // eligible for the controller's fast grayscale path.
            RefreshRequest::Fast
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
