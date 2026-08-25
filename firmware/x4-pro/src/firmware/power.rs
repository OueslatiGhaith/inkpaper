use esp_hal::gpio::{Level, Output, OutputConfig, OutputPin};

use crate::firmware::sleep_pins;

pub struct PowerRails<'d> {
    peripheral: Output<'d>,
    touch: Output<'d>,
}

impl<'d> PowerRails<'d> {
    pub fn new(
        peripheral: impl OutputPin + 'd,
        touch: impl OutputPin + 'd,
        sd: impl OutputPin + 'd,
    ) -> (Self, SdPower<'d>) {
        // configure the awake states while any RTC pad holds left by the previous
        // deep-sleep cycle are still active.
        // once all 3 output configurations are ready we can release the holds without
        // allowing the pins to pass through their reset state
        let peripheral = Output::new(peripheral, Level::High, OutputConfig::default());
        let touch = Output::new(touch, Level::High, OutputConfig::default());
        let sd = Output::new(sd, Level::High, OutputConfig::default());

        sleep_pins::release_power_holds();

        (Self { peripheral, touch }, SdPower { enable: sd })
    }

    pub fn enable_touch(&mut self) {
        self.touch.set_low();
    }

    pub fn disable_touch(&mut self) {
        self.touch.set_high();
    }

    pub fn prepare_for_deep_sleep(&mut self) {
        // explicitly establish every state that will subsequently be latched
        // - GPIO1 HIGH: master peripheral rail stays asserted
        // - GPIO2 HIGH: GT911 off
        // - GPIO5 HIGH: sd off
        self.peripheral.set_high();
        self.touch.set_high();
        // GPIO5 is owned by the storage task and is already driven HIGH before storage
        // acknowledges shutdown
    }
}

pub struct SdPower<'d> {
    enable: Output<'d>,
}

impl SdPower<'_> {
    pub fn enable(&mut self) {
        self.enable.set_low();
    }

    pub fn disable(&mut self) {
        self.enable.set_high();
    }
}
