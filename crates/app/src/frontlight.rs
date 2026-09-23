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
    pub(crate) const fn setting(&self) -> FrontlightSetting {
        self.setting
    }

    pub(crate) fn request_apply(&mut self) {
        self.pending = Some(self.setting);
    }

    pub(crate) fn take_request(&mut self) -> Option<FrontlightSetting> {
        self.pending.take()
    }

    pub(crate) fn set_brightness(&mut self, brightness: u8) -> bool {
        let brightness = brightness.clamp(1, 100);

        self.replace(FrontlightSetting {
            brightness,
            warmth: self.setting.warmth,
            on: true,
        })
    }

    pub(crate) fn adjust_brightness(&mut self, delta: i16) -> bool {
        let brightness = (i16::from(self.setting.brightness) + delta).clamp(1, 100) as u8;
        self.set_brightness(brightness)
    }

    pub(crate) fn set_warmth(&mut self, warmth: u8) -> bool {
        let warmth = warmth.min(100);

        self.replace(FrontlightSetting {
            brightness: self.setting.brightness,
            warmth,
            on: self.setting.on,
        })
    }

    pub(crate) fn adjust_warmth(&mut self, delta: i16) -> bool {
        let warmth = (i16::from(self.setting.warmth) + delta).clamp(0, 100) as u8;
        self.set_warmth(warmth)
    }

    pub(crate) fn toggle(&mut self) -> bool {
        self.replace(FrontlightSetting {
            brightness: self.setting.brightness,
            warmth: self.setting.warmth,
            on: !self.setting.on,
        })
    }

    fn replace(&mut self, setting: FrontlightSetting) -> bool {
        if self.setting == setting {
            return false;
        }

        self.setting = setting;
        self.pending = Some(setting);
        true
    }
}
