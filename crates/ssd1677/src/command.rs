#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Command {
    DriverOutputControl = 0x01,
    GateVoltage = 0x03,
    SourceVoltage = 0x04,

    BoosterSoftStart = 0x0c,

    DeepSleep = 0x10,
    DataEntryMode = 0x11,
    SoftReset = 0x12,

    TemperatureSensorControl = 0x18,
    WriteTemperature = 0x1a,

    MasterActivation = 0x20,
    DisplayUpdateControl1 = 0x21,
    DisplayUpdateControl2 = 0x22,

    WriteBlackWhiteRam = 0x24,
    WriteRedRam = 0x26,

    WriteVcom = 0x2c,
    WriteLut = 0x32,

    BorderWaveform = 0x3c,

    SetRamXRange = 0x44,
    SetRamYRange = 0x45,

    AutoWriteBlackWhiteRam = 0x46,
    AutoWriteRedRam = 0x47,

    SetRamXCounter = 0x4e,
    SetRamYCounter = 0x4f,
}

impl Command {
    pub(crate) const fn byte(self) -> u8 {
        self as u8
    }
}
