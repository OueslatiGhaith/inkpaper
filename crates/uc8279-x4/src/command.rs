#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    PanelSetting = 0x00,

    PowerOff = 0x02,
    PowerOffSequence = 0x03,
    PowerOn = 0x04,

    DeepSleep = 0x07,

    OldPlane = 0x10,
    DisplayRefresh = 0x12,
    NewPlane = 0x13,

    Pll = 0x30,

    VcomDataInterval = 0x50,

    Resolution = 0x61,
    GateSourceStart = 0x65,

    PartialWindow = 0x90,
    PartialIn = 0x91,
    PartialOut = 0x92,

    CascadeControl = 0xe0,
    GateScan = 0xe1,
    Temperature = 0xe5,
}

impl Command {
    pub(crate) const fn byte(self) -> u8 {
        self as u8
    }
}
