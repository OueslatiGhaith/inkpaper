pub const BATTERY_PROFILE_LEN: usize = 80;

#[derive(Debug)]
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
pub enum InitializationStats {
    AlreadyLoaded,
    Updated,
}
