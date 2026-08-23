use esp_hal::peripherals::LPWR;

/// pins whose electrical state must remain latched while the X4 PRO is in deep sleep:
/// - GPIO1 = master peripheral rail, HIGH
/// - GPIO2 = GT911 power, HIGH = off
/// - GPIO5 = SD power, HIGH = off
/// - GPIO14 = EPD RESET, HIGH
///
/// these pins are all RTC IOs on ESP32-S3. `esp-hal`'s [`RtcPin`](esp_hal::gpio::RtcPin)
/// implementation maps their pad holds ontp these exact LPWR bits
pub fn hold_for_deep_sleep() {
    LPWR::regs().pad_hold().modify(|_, w| {
        w.touch_pad1()
            .set_bit()
            .touch_pad2()
            .set_bit()
            .touch_pad5()
            .set_bit()
            .touch_pad14()
            .set_bit()
    });
}

/// release the 3 board-rail holds.
///
/// # IMPORTANT:
/// call this only *after* GPIO1/2/5 have already been configured to the desired awake
/// output levels. That avoids a transient caused by releasing a held ouptput into its
/// reset/default configuration
pub fn release_power_holds() {
    LPWR::regs().pad_hold().modify(|_, w| {
        w.touch_pad1()
            .clear_bit()
            .touch_pad2()
            .clear_bit()
            .touch_pad5()
            .clear_bit()
    });
}

/// release the EPD reset hold
///
/// call this only after the display reset pin has already been configured as an output
/// HIGH by `ProbePins`
pub fn release_display_reset_hold() {
    LPWR::regs()
        .pad_hold()
        .modify(|_, w| w.touch_pad14().clear_bit());
}
