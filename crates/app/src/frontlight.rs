#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrontlightSetting {
    brightness: u8,
    warmth: u8,
    on: bool,
}

impl FrontlightSetting {
    pub const fn new(brightness: u8, warmth: u8, on: bool) -> Option<Self> {
        if brightness == 0 || brightness > 100 || warmth > 100 {
            return None;
        }

        Some(Self {
            brightness,
            warmth,
            on,
        })
    }

    pub const fn brightness(self) -> u8 {
        self.brightness
    }

    pub const fn warmth(self) -> u8 {
        self.warmth
    }

    pub const fn is_on(self) -> bool {
        self.on
    }
}

impl Default for FrontlightSetting {
    fn default() -> Self {
        Self {
            // Keep the current InkPaper boot behaviour for now.
            brightness: 25,
            warmth: 50,
            on: true,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct FrontlightState {
    setting: FrontlightSetting,
    pending: Option<FrontlightSetting>,
}

impl FrontlightState {
    pub(crate) fn request_apply(&mut self) {
        self.pending = Some(self.setting);
    }

    pub(crate) fn take_request(&mut self) -> Option<FrontlightSetting> {
        self.pending.take()
    }
}
