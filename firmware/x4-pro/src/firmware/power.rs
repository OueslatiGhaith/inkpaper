use esp_hal::gpio::{Level, Output, OutputConfig, OutputPin};

use crate::firmware::sleep_pins;

pub struct PowerRails<'d> {
    peripheral: Output<'d>,
    touch: Output<'d>,
    sd: Output<'d>,
}

impl<'d> PowerRails<'d> {
    pub fn new(
        peripheral: impl OutputPin + 'd,
        touch: impl OutputPin + 'd,
        sd: impl OutputPin + 'd,
    ) -> Self {
        // configure the awake states while any RTC pad holds left by the previous
        // deep-sleep cycle are still active.
        // once all 3 output configurations are ready we can release the holds without
        // allowing the pins to pass through their reset state
        let peripheral = Output::new(peripheral, Level::High, OutputConfig::default());
        let touch = Output::new(touch, Level::High, OutputConfig::default());
        let sd = Output::new(sd, Level::High, OutputConfig::default());

        sleep_pins::release_power_holds();

        Self {
            peripheral,
            touch,
            sd,
        }
    }

    pub fn enable_touch(&mut self) {
        self.touch.set_low();
    }

    pub fn disable_touch(&mut self) {
        self.touch.set_high();
    }

    pub fn enable_sd(&mut self) {
        self.sd.set_low();
    }

    pub fn disable_sd(&mut self) {
        self.sd.set_high();
    }

    pub fn prepare_for_deep_sleep(&mut self) {
        // explicitly establish every state that will subsequently be latched
        // - GPIO1 HIGH: master peripheral rail stays asserted
        // - GPIO2 HIGH: GT911 off
        // - GPIO5 HIGH: sd off
        self.peripheral.set_high();
        self.touch.set_high();
        self.sd.set_high();
    }
}
