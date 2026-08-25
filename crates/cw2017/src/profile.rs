pub const BATTERY_PROFILE_LEN: usize = 80;

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct BatteryProfile {
    bytes: [u8; BATTERY_PROFILE_LEN],
}

impl BatteryProfile {
    pub const fn new(bytes: [u8; BATTERY_PROFILE_LEN]) -> Self {
        Self { bytes }
    }

    pub const fn as_bytes(&self) -> &[u8; BATTERY_PROFILE_LEN] {
        &self.bytes
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum InitializationStats {
    AlreadyLoaded,
    Updated,
}
