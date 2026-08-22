use esp_hal::gpio::{Level, Output, OutputConfig, OutputPin};

pub struct PowerRails<'d> {
    _peripheral: Output<'d>,
    _touch: Output<'d>,
    _sd: Output<'d>,
}

impl<'d> PowerRails<'d> {
    pub fn new(
        peripheral: impl OutputPin + 'd,
        touch: impl OutputPin + 'd,
        sd: impl OutputPin + 'd,
    ) -> Self {
        let config = OutputConfig::default();

        Self {
            _peripheral: Output::new(peripheral, Level::High, config),
            _touch: Output::new(touch, Level::High, config),
            _sd: Output::new(sd, Level::High, config),
        }
    }
}
