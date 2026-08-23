use esp_hal::gpio::{Level, Output, OutputConfig, OutputPin};

pub struct PowerRails<'d> {
    _peripheral: Output<'d>,
    touch: Output<'d>,
    sd: Output<'d>,
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
            touch: Output::new(touch, Level::High, config),
            sd: Output::new(sd, Level::High, config),
        }
    }

    pub fn enable_touch(&mut self) {
        self.touch.set_low();
    }

    pub fn disable_touch(&mut self) {
        self.touch.set_high();
    }

    pub fn prepare_for_deep_sleep(&mut self) {
        // both switched peripherals are active-low, so HIGH is off
        self.touch.set_high();
        self.sd.set_high();
    }
}
