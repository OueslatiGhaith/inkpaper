#![no_std]

use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, spi::SpiBus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusyPolarity {
    ActiveHigh,
    ActiveLow,
}

#[derive(Debug)]
pub enum Error<Spi, Cs, Dc, Reset, Busy> {
    Spi(Spi),
    ChipSelect(Cs),
    DataCommand(Dc),
    Reset(Reset),
    Busy(Busy),
}

#[allow(async_fn_in_trait)]
pub trait EpdInterface {
    type Error;

    async fn command(&mut self, command: u8) -> Result<(), Self::Error>;
    async fn data(&mut self, data: &[u8]) -> Result<(), Self::Error>;
    async fn command_data(&mut self, command: u8, data: &[u8]) -> Result<(), Self::Error>;
    async fn reset<D>(&mut self, delay: &mut D) -> Result<(), Self::Error>
    where
        D: DelayNs;
    async fn wait_busy<D>(
        &mut self,
        polarity: BusyPolarity,
        delay: &mut D,
    ) -> Result<(), Self::Error>
    where
        D: DelayNs;
    fn is_busy(&mut self, polarity: BusyPolarity) -> Result<bool, Self::Error>;
}

pub struct SpiEpdBus<SPI, CS, DC, RESET, BUSY> {
    spi: SPI,
    cs: CS,
    dc: DC,
    reset: RESET,
    busy: BUSY,
}

impl<SPI, CS, DC, RESET, BUSY> SpiEpdBus<SPI, CS, DC, RESET, BUSY>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
    DC: OutputPin,
    RESET: OutputPin,
    BUSY: InputPin,
{
    pub fn new(
        spi: SPI,
        mut cs: CS,
        mut dc: DC,
        mut reset: RESET,
        busy: BUSY,
    ) -> Result<Self, Error<SPI::Error, CS::Error, DC::Error, RESET::Error, BUSY::Error>> {
        cs.set_high().map_err(Error::ChipSelect)?;
        dc.set_high().map_err(Error::DataCommand)?;
        reset.set_high().map_err(Error::Reset)?;

        Ok(Self {
            spi,
            cs,
            dc,
            reset,
            busy,
        })
    }

    pub fn release(self) -> (SPI, CS, DC, RESET, BUSY) {
        (self.spi, self.cs, self.dc, self.reset, self.busy)
    }

    async fn finish_transaction(
        &mut self,
        spi_result: Result<(), SPI::Error>,
    ) -> Result<(), Error<SPI::Error, CS::Error, DC::Error, RESET::Error, BUSY::Error>> {
        let deselect = self.cs.set_high().map_err(Error::ChipSelect);
        match spi_result {
            Err(error) => {
                let _ = deselect;
                Err(Error::Spi(error))
            }
            Ok(()) => deselect,
        }
    }
}

impl<SPI, CS, DC, RESET, BUSY> EpdInterface for SpiEpdBus<SPI, CS, DC, RESET, BUSY>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
    DC: OutputPin,
    RESET: OutputPin,
    BUSY: InputPin,
{
    type Error = Error<SPI::Error, CS::Error, DC::Error, RESET::Error, BUSY::Error>;

    async fn command(&mut self, command: u8) -> Result<(), Self::Error> {
        self.dc.set_low().map_err(Error::DataCommand)?;
        self.cs.set_low().map_err(Error::ChipSelect)?;
        let result = self.spi.write(&[command]).await;

        self.finish_transaction(result).await
    }

    async fn data(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        self.dc.set_high().map_err(Error::DataCommand)?;
        self.cs.set_low().map_err(Error::ChipSelect)?;
        let result = self.spi.write(data).await;

        self.finish_transaction(result).await
    }

    async fn command_data(&mut self, command: u8, data: &[u8]) -> Result<(), Self::Error> {
        self.cs.set_low().map_err(Error::ChipSelect)?;
        self.dc.set_low().map_err(Error::DataCommand)?;

        if let Err(error) = self.spi.write(&[command]).await {
            let _ = self.cs.set_high();
            return Err(Error::Spi(error));
        }

        if !data.is_empty() {
            if let Err(error) = self.dc.set_high() {
                let _ = self.cs.set_high();
                return Err(Error::DataCommand(error));
            }

            if let Err(error) = self.spi.write(data).await {
                let _ = self.cs.set_high();
                return Err(Error::Spi(error));
            }
        }

        self.cs.set_high().map_err(Error::ChipSelect)
    }

    async fn reset<D>(&mut self, delay: &mut D) -> Result<(), Self::Error>
    where
        D: DelayNs,
    {
        self.reset.set_high().map_err(Error::Reset)?;
        delay.delay_ms(10).await;

        self.reset.set_low().map_err(Error::Reset)?;
        delay.delay_ms(10).await;

        self.reset.set_high().map_err(Error::Reset)?;
        delay.delay_ms(10).await;

        Ok(())
    }

    async fn wait_busy<D>(
        &mut self,
        polarity: BusyPolarity,
        delay: &mut D,
    ) -> Result<(), Self::Error>
    where
        D: DelayNs,
    {
        while self.is_busy(polarity)? {
            delay.delay_ms(1).await;
        }

        Ok(())
    }

    fn is_busy(&mut self, polarity: BusyPolarity) -> Result<bool, Self::Error> {
        match polarity {
            BusyPolarity::ActiveHigh => self.busy.is_high(),
            BusyPolarity::ActiveLow => self.busy.is_low(),
        }
        .map_err(Error::Busy)
    }
}
