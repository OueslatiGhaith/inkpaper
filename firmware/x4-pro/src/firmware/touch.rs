use defmt::warn;
use embassy_time::{Duration, Instant, Timer};
use embedded_hal_async::{delay::DelayNs, i2c::I2c};
use esp_hal::gpio::{Flex, InputConfig, Level, Output, OutputConfig, OutputPin, Pin, Pull};
use gt911::{
    ALTERNATE_ADDRESS, Gt911, PRIMARY_ADDRESS, PointLayout, ProductInfo, TouchFrame, TouchPoint,
};
use inkpaper_app::TouchGesture;
use inkpaper_ui::{Point, px};

use crate::firmware::{
    framebuffer::{LOGICAL_HEIGHT, LOGICAL_WIDTH},
    i2c_bus::SharedI2cDevice,
    input::{INPUT_EVENTS, InputEvent, TouchEvent},
};

const POLL_INTERVAL_MS: u64 = 10;
pub struct TouchController<'d, I2C> {
    driver: Gt911<I2C>,
    reset: Output<'d>,
    interrupt: Flex<'d>,
}

impl<'d, I2C> TouchController<'d, I2C> {
    pub fn new(i2c: I2C, reset: impl OutputPin + 'd, interrupt: impl Pin + 'd) -> Self {
        let reset = Output::new(reset, Level::High, OutputConfig::default());
        let mut interrupt = Flex::new(interrupt);

        interrupt.apply_input_config(&InputConfig::default().with_pull(Pull::Up));
        interrupt.apply_output_config(&OutputConfig::default());
        interrupt.set_output_enable(false);
        interrupt.set_input_enable(true);

        Self {
            driver: Gt911::new(i2c, PRIMARY_ADDRESS, PointLayout::CoordinatesAtByte0),
            reset,
            interrupt,
        }
    }

    pub const fn address(&self) -> u8 {
        self.driver.address()
    }
}

impl<I2C> TouchController<'_, I2C>
where
    I2C: I2c,
{
    pub async fn initialize<D>(
        &mut self,
        delay: &mut D,
    ) -> Result<ProductInfo, gt911::Error<I2C::Error>>
    where
        D: DelayNs,
    {
        // the X4 Pro normally selects 0x5D with INT LOW
        self.reset_for_address(delay, Level::Low).await;

        if let Ok(info) = self.probe_addresses().await {
            return Ok(info);
        }

        // retyr the complete reset/address sequence with INT HIGH
        self.reset_for_address(delay, Level::High).await;
        self.probe_addresses().await
    }

    async fn poll(&mut self) -> Result<Option<TouchFrame>, gt911::Error<I2C::Error>> {
        self.driver.poll().await
    }

    async fn probe_addresses(&mut self) -> Result<ProductInfo, gt911::Error<I2C::Error>> {
        self.driver.set_address(PRIMARY_ADDRESS);
        if let Ok(info) = self.driver.product_info().await {
            return Ok(info);
        }

        self.driver.set_address(ALTERNATE_ADDRESS);
        self.driver.product_info().await
    }

    async fn reset_for_address<D>(&mut self, delay: &mut D, address_select: Level)
    where
        D: DelayNs,
    {
        self.reset.set_low();
        self.interrupt.set_input_enable(false);
        self.interrupt.set_level(address_select);
        self.interrupt.set_output_enable(true);
        delay.delay_ms(10).await;

        self.reset.set_high();
        delay.delay_ms(10).await;

        self.interrupt.set_level(address_select);
        delay.delay_ms(50).await;

        self.interrupt.set_output_enable(false);
        self.interrupt
            .apply_input_config(&InputConfig::default().with_pull(Pull::Up));
        self.interrupt.set_input_enable(true);

        delay.delay_ms(50).await;
    }
}

#[embassy_executor::task]
pub async fn touch_task(mut touch: TouchController<'static, SharedI2cDevice>) {
    let mut gesture = TouchGesture::default();
    let mut home_pressed = false;
    let mut consecutive_errors = 0u16;

    loop {
        match touch.poll().await {
            Ok(Some(frame)) => {
                consecutive_errors = 0;

                let home = frame.home_key();
                if home && !home_pressed {
                    home_pressed = true;
                } else if !home && home_pressed {
                    home_pressed = false;
                    INPUT_EVENTS
                        .send(InputEvent::Touch(TouchEvent::HomeTap))
                        .await;
                }

                let input = match frame.first_point() {
                    Some(point) => gesture.touch(logical_position(point), now_ms()),
                    None => gesture.release(),
                };

                if let Some(input) = input {
                    INPUT_EVENTS
                        .send(InputEvent::Touch(TouchEvent::Pointer(input)))
                        .await;
                }
            }
            Ok(None) => consecutive_errors = 0,
            Err(error) => {
                consecutive_errors = consecutive_errors.saturating_add(1);
                if consecutive_errors == 1 || consecutive_errors.is_multiple_of(100) {
                    warn!("GT911 poll error ({}): {:?}", consecutive_errors, error);
                }
            }
        }

        // a finger held still may not report new frames, so the long press
        // is timed here
        if let Some(input) = gesture.tick(now_ms()) {
            INPUT_EVENTS
                .send(InputEvent::Touch(TouchEvent::Pointer(input)))
                .await;
        }

        Timer::after(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}

fn now_ms() -> u64 {
    Instant::now().as_millis()
}

fn logical_position(point: TouchPoint) -> Point {
    // X4 Pro's GT911 is physically mounted in portrait coordinates.

    let x = point.x().min(LOGICAL_WIDTH as u16 - 1);
    let y = point.y().min(LOGICAL_HEIGHT as u16 - 1);

    Point::new(px(i32::from(x)), px(i32::from(y)))
}
