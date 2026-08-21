use bitfield_struct::bitfield;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Register {
    Version = 0x00,
    Vcell = 0x02,
    StateOfCharge = 0x04,
    Temperature = 0x06,
    Config = 0x08,
    InterruptConfig = 0x0a,
    SocAlert = 0x0b,
    TemperatureMax = 0x0c,
    TemperatureMin = 0x0d,
    IdVoltage = 0x0e,
    BatteryProfile = 0x10,
}

impl Register {
    pub(crate) const fn address(self) -> u8 {
        self as u8
    }
}

/// raw 16-bit contents of `VCELL_H/VCELL_L`
#[bitfield(u16)]
pub(crate) struct VcellRegister {
    #[bits(14)]
    pub counts: u16,
    #[bits(2)]
    _reserved: u8,
}

/// raw 16-bit contents of `SOC_H/SOC_L`
#[bitfield(u16)]
pub(crate) struct SocRegister {
    pub fraction_256ths: u8,
    pub whole_percent: u8,
}

/// `CONFIG` reisgter, 0x08
#[bitfield(u8)]
pub(crate) struct ConfigRegister {
    #[bits(4)]
    _reserved: u8,
    #[bits(2)]
    pub restart: u8,
    #[bits(2)]
    pub sleep: u8,
}

/// `SOC_ALERT` register, 0x0b
#[bitfield(u8)]
pub(crate) struct SocAlertRegister {
    #[bits(7)]
    pub threshold: u8,
    pub profile_updated: bool,
}
