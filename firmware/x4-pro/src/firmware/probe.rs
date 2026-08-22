use core::convert::Infallible;

use esp_hal::{
    delay::Delay,
    gpio::{Flex, InputConfig, InputPin, Level, OutputConfig, OutputPin, Pull},
};
use xteink_display_probe::ProbeIo;

pub struct ProbePins<'d> {
    sclk: Flex<'d>,
    data: Flex<'d>,
    cs: Flex<'d>,
    dc: Flex<'d>,
    reset: Flex<'d>,

    clock_delay: Delay,
}

impl<'d> ProbePins<'d> {
    pub fn new(
        sclk: impl OutputPin + 'd,
        data: impl InputPin + OutputPin + 'd,
        cs: impl OutputPin + 'd,
        dc: impl OutputPin + 'd,
        reset: impl OutputPin + 'd,
    ) -> Self {
        let mut this = Self {
            sclk: Flex::new(sclk),
            data: Flex::new(data),
            cs: Flex::new(cs),
            dc: Flex::new(dc),
            reset: Flex::new(reset),
            clock_delay: Delay::new(),
        };

        configure_output(&mut this.sclk, Level::Low);
        configure_output(&mut this.data, Level::Low);
        configure_output(&mut this.cs, Level::High);
        configure_output(&mut this.dc, Level::Low);
        configure_output(&mut this.reset, Level::High);

        this
    }

    pub fn into_parts(mut self) -> (Flex<'d>, Flex<'d>, Flex<'d>, Flex<'d>, Flex<'d>) {
        configure_output(&mut self.sclk, Level::Low);
        configure_output(&mut self.data, Level::Low);
        configure_output(&mut self.cs, Level::High);
        configure_output(&mut self.dc, Level::High);
        configure_output(&mut self.reset, Level::High);

        (self.sclk, self.data, self.cs, self.dc, self.reset)
    }
}

impl ProbeIo for ProbePins<'_> {
    type Error = Infallible;

    fn cs_high(&mut self) -> Result<(), Self::Error> {
        self.cs.set_high();
        Ok(())
    }

    fn cs_low(&mut self) -> Result<(), Self::Error> {
        self.cs.set_low();
        Ok(())
    }

    fn dc_high(&mut self) -> Result<(), Self::Error> {
        self.dc.set_high();
        Ok(())
    }

    fn dc_low(&mut self) -> Result<(), Self::Error> {
        self.dc.set_low();
        Ok(())
    }

    fn clock_high(&mut self) -> Result<(), Self::Error> {
        self.sclk.set_high();
        Ok(())
    }

    fn clock_low(&mut self) -> Result<(), Self::Error> {
        self.sclk.set_low();
        Ok(())
    }

    fn reset_high(&mut self) -> Result<(), Self::Error> {
        self.reset.set_high();
        Ok(())
    }

    fn reset_low(&mut self) -> Result<(), Self::Error> {
        self.reset.set_low();
        Ok(())
    }

    fn data_output(&mut self) -> Result<(), Self::Error> {
        self.data.set_low();
        self.data.apply_output_config(&OutputConfig::default());
        self.data.set_input_enable(false);
        self.data.set_output_enable(true);

        Ok(())
    }

    fn data_input_pullup(&mut self) -> Result<(), Self::Error> {
        // stop driving before enabling the pull-up/input buffer.
        self.data.set_high();
        self.data.set_output_enable(false);
        self.data
            .apply_input_config(&InputConfig::default().with_pull(Pull::Up));

        self.data.set_input_enable(true);

        Ok(())
    }

    fn data_high(&mut self) -> Result<(), Self::Error> {
        self.data.set_high();
        Ok(())
    }

    fn data_low(&mut self) -> Result<(), Self::Error> {
        self.data.set_low();
        Ok(())
    }

    fn data_is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.data.is_high())
    }

    fn clock_delay(&mut self) {
        self.clock_delay.delay_micros(1);
    }

    fn release(&mut self) -> Result<(), Self::Error> {
        // leave CS explicitly pulled high while the normal SPI peripheral
        // hasn't taken ownership yet.
        self.cs.set_high();
        self.cs.set_output_enable(false);
        self.cs
            .apply_input_config(&InputConfig::default().with_pull(Pull::Up));
        self.cs.set_input_enable(true);

        // RST_N should never be left floating low.
        self.reset.set_high();
        self.reset.set_output_enable(false);
        self.reset
            .apply_input_config(&InputConfig::default().with_pull(Pull::Up));
        self.reset.set_input_enable(true);

        float_pin(&mut self.sclk);
        float_pin(&mut self.data);
        float_pin(&mut self.dc);

        Ok(())
    }
}

fn configure_output(pin: &mut Flex<'_>, level: Level) {
    pin.set_level(level);
    pin.apply_output_config(&OutputConfig::default());
    pin.set_input_enable(false);
    pin.set_output_enable(true);
}

fn float_pin(pin: &mut Flex<'_>) {
    pin.set_output_enable(false);
    pin.set_input_enable(true);
}
