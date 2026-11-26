#![no_std]

use core::mem::ManuallyDrop;

use embassy_futures::select::{Either, select};
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait, spi::SpiBus};

mod frame;
mod panel;

pub use frame::{Region, grayscale_base_byte, region_has_grayscale};
pub use panel::{
    BinaryOverGrayMode, BinaryUpdateMode, EpdPanel, GrayscaleUpdateMode, PanelCapabilities,
    PanelError, RefreshMode,
};

/// longest a BUSY wait may last before the panel is treated as stuck.
///
/// a cold full refresh takes a few seconds; this matches crosspoint's bound
const BUSY_TIMEOUT_MS: u32 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum BusyPolarity {
    ActiveHigh,
    ActiveLow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DisplayPhase {
    Unknown,
    PowerOn,
    BinaryFullRefresh,
    BinaryFastRefresh,
    GrayscaleBaseRefresh,
    GrayscalePrecondition,
    GrayscaleActivate,
    GrayscaleRefresh,
    PowerOff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ProtocolError {
    StreamAlreadyOpen,
    StreamNotOpen,
}

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error<Spi, Cs, Dc, Reset, Busy> {
    Spi(Spi),
    ChipSelect(Cs),
    DataCommand(Dc),
    Reset(Reset),
    Busy(Busy),
    BusyTimeout,
    Protocol(ProtocolError),
}

#[allow(async_fn_in_trait)]
pub trait EpdInterface {
    type Error;

    async fn command(&mut self, command: u8) -> Result<(), Self::Error>;
    async fn data(&mut self, data: &[u8]) -> Result<(), Self::Error>;

    async fn reset<D>(&mut self, delay: &mut D) -> Result<(), Self::Error>
    where
        D: DelayNs;

    fn reset_high(&mut self) -> Result<(), Self::Error>;

    fn is_busy(&mut self, polarity: BusyPolarity) -> Result<bool, Self::Error>;

    async fn wait_busy<D>(
        &mut self,
        polarity: BusyPolarity,
        delay: &mut D,
    ) -> Result<(), Self::Error>
    where
        D: DelayNs;

    /// Wait for the panel to become idle, tagging the wait with its logical
    /// display phase for instrumentation wrappers.
    async fn wait_busy_for<D>(
        &mut self,
        polarity: BusyPolarity,
        _phase: DisplayPhase,
        delay: &mut D,
    ) -> Result<(), Self::Error>
    where
        D: DelayNs,
    {
        self.wait_busy(polarity, delay).await
    }

    /// begin a streamed-data transaction.
    ///
    /// CS remains asserted until `end_data_stream()`. Prefer [`DataStream`] so
    /// early returns and cancelled futures always release CS.
    async fn begin_data_stream(&mut self) -> Result<(), Self::Error>;

    /// write another chunk of data while the stream remains selected.
    async fn stream_data(&mut self, data: &[u8]) -> Result<(), Self::Error>;

    /// finish a data stream and deassert CS.
    fn end_data_stream(&mut self) -> Result<(), Self::Error>;
}

/// Drop-safe RAII guard for a streamed EPD data transfer.
///
/// Keeps CS asserted across multiple chunks and guarantees `end_data_stream()`
/// runs even if a caller returns early on error or drops the surrounding async
/// future mid-transfer.
pub struct DataStream<'a, B: EpdInterface + ?Sized> {
    bus: &'a mut B,
    closed: bool,
}

impl<'a, B: EpdInterface + ?Sized> DataStream<'a, B> {
    pub async fn open(bus: &'a mut B) -> Result<Self, B::Error> {
        bus.begin_data_stream().await?;

        Ok(Self { bus, closed: false })
    }

    pub async fn write(&mut self, data: &[u8]) -> Result<(), B::Error> {
        if let Err(error) = self.bus.stream_data(data).await {
            self.closed = true;
            let _ = self.bus.end_data_stream();

            return Err(error);
        }

        Ok(())
    }

    pub fn finish(mut self) -> Result<(), B::Error> {
        self.closed = true;
        self.bus.end_data_stream()
    }
}

impl<B: EpdInterface + ?Sized> Drop for DataStream<'_, B> {
    fn drop(&mut self) {
        if !self.closed {
            self.closed = true;
            let _ = self.bus.end_data_stream();
        }
    }
}

/// Ensures CS returns high if an async single-transaction transfer is dropped
/// before `spi.write().await` completes.
struct CsGuard<'a, CS: OutputPin> {
    cs: &'a mut CS,
}

impl<'a, CS: OutputPin> CsGuard<'a, CS> {
    fn assert_low(cs: &'a mut CS) -> Result<Self, CS::Error> {
        cs.set_low()?;
        Ok(Self { cs })
    }

    fn finish(self) -> Result<(), CS::Error> {
        let mut this = ManuallyDrop::new(self);
        this.cs.set_high()
    }
}

impl<CS: OutputPin> Drop for CsGuard<'_, CS> {
    fn drop(&mut self) {
        let _ = self.cs.set_high();
    }
}

pub struct SpiEpdBus<SPI, CS, DC, RESET, BUSY> {
    spi: SPI,
    cs: CS,
    dc: DC,
    reset: RESET,
    busy: BUSY,
    stream_open: bool,
}

impl<SPI, CS, DC, RESET, BUSY> SpiEpdBus<SPI, CS, DC, RESET, BUSY>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
    DC: OutputPin,
    RESET: OutputPin,
    BUSY: InputPin + Wait,
{
    pub fn new(
        spi: SPI,
        mut cs: CS,
        mut dc: DC,
        mut reset: RESET,
        busy: BUSY,
    ) -> Result<Self, <Self as EpdInterface>::Error> {
        cs.set_high().map_err(Error::ChipSelect)?;
        dc.set_high().map_err(Error::DataCommand)?;
        reset.set_high().map_err(Error::Reset)?;

        Ok(Self {
            spi,
            cs,
            dc,
            reset,
            busy,
            stream_open: false,
        })
    }

    pub fn release(self) -> (SPI, CS, DC, RESET, BUSY) {
        (self.spi, self.cs, self.dc, self.reset, self.busy)
    }

    fn ensure_no_stream(&self) -> Result<(), <Self as EpdInterface>::Error> {
        if self.stream_open {
            return Err(Error::Protocol(ProtocolError::StreamAlreadyOpen));
        }

        Ok(())
    }

    pub fn reset_high(&mut self) -> Result<(), <Self as EpdInterface>::Error> {
        self.ensure_no_stream()?;
        self.reset.set_high().map_err(Error::Reset)
    }
}

impl<SPI, CS, DC, RESET, BUSY> EpdInterface for SpiEpdBus<SPI, CS, DC, RESET, BUSY>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
    DC: OutputPin,
    RESET: OutputPin,
    BUSY: InputPin + Wait,
{
    type Error = Error<SPI::Error, CS::Error, DC::Error, RESET::Error, BUSY::Error>;

    async fn command(&mut self, command: u8) -> Result<(), Self::Error> {
        self.ensure_no_stream()?;

        self.dc.set_low().map_err(Error::DataCommand)?;
        let cs = CsGuard::assert_low(&mut self.cs).map_err(Error::ChipSelect)?;

        let result = self.spi.write(&[command]).await;
        let cs_result = cs.finish().map_err(Error::ChipSelect);

        match result {
            Err(error) => Err(Error::Spi(error)),
            Ok(()) => cs_result,
        }
    }

    async fn data(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        self.ensure_no_stream()?;

        self.dc.set_high().map_err(Error::DataCommand)?;
        let cs = CsGuard::assert_low(&mut self.cs).map_err(Error::ChipSelect)?;

        let result = self.spi.write(data).await;
        let cs_result = cs.finish().map_err(Error::ChipSelect);

        match result {
            Err(error) => Err(Error::Spi(error)),
            Ok(()) => cs_result,
        }
    }

    async fn reset<D>(&mut self, delay: &mut D) -> Result<(), Self::Error>
    where
        D: DelayNs,
    {
        self.ensure_no_stream()?;

        self.reset.set_high().map_err(Error::Reset)?;
        delay.delay_ms(10).await;

        self.reset.set_low().map_err(Error::Reset)?;
        delay.delay_ms(10).await;

        self.reset.set_high().map_err(Error::Reset)?;
        delay.delay_ms(10).await;

        Ok(())
    }

    fn reset_high(&mut self) -> Result<(), Self::Error> {
        SpiEpdBus::reset_high(self)
    }

    fn is_busy(&mut self, polarity: BusyPolarity) -> Result<bool, Self::Error> {
        match polarity {
            BusyPolarity::ActiveHigh => self.busy.is_high(),
            BusyPolarity::ActiveLow => self.busy.is_low(),
        }
        .map_err(Error::Busy)
    }

    async fn wait_busy<D>(
        &mut self,
        polarity: BusyPolarity,
        delay: &mut D,
    ) -> Result<(), Self::Error>
    where
        D: DelayNs,
    {
        let idle = async {
            match polarity {
                BusyPolarity::ActiveHigh => self.busy.wait_for_low().await,
                BusyPolarity::ActiveLow => self.busy.wait_for_high().await,
            }
        };

        match select(idle, delay.delay_ms(BUSY_TIMEOUT_MS)).await {
            Either::First(result) => result.map_err(Error::Busy),
            Either::Second(()) => Err(Error::BusyTimeout),
        }
    }

    async fn begin_data_stream(&mut self) -> Result<(), Self::Error> {
        self.ensure_no_stream()?;

        self.dc.set_high().map_err(Error::DataCommand)?;
        self.cs.set_low().map_err(Error::ChipSelect)?;

        self.stream_open = true;

        Ok(())
    }

    async fn stream_data(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        if !self.stream_open {
            return Err(Error::Protocol(ProtocolError::StreamNotOpen));
        }

        if let Err(error) = self.spi.write(data).await {
            self.stream_open = false;
            let _ = self.cs.set_high();

            return Err(Error::Spi(error));
        }

        Ok(())
    }

    fn end_data_stream(&mut self) -> Result<(), Self::Error> {
        if !self.stream_open {
            return Err(Error::Protocol(ProtocolError::StreamNotOpen));
        }

        self.stream_open = false;
        self.cs.set_high().map_err(Error::ChipSelect)
    }
}

#[cfg(test)]
mod tests {
    use core::{
        convert::Infallible,
        future::{Future, pending},
        pin::pin,
        task::{Context, Poll, Waker},
    };

    use embedded_hal::digital::{ErrorType as DigitalErrorType, InputPin, OutputPin};
    use embedded_hal_async::{
        digital::Wait,
        spi::{ErrorType as SpiErrorType, SpiBus},
    };

    use super::{DataStream, EpdInterface, Error, ProtocolError, SpiEpdBus};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct MockPin {
        high: bool,
    }

    impl MockPin {
        fn new(high: bool) -> Self {
            Self { high }
        }
    }

    impl DigitalErrorType for MockPin {
        type Error = Infallible;
    }

    impl OutputPin for MockPin {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            self.high = false;
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            self.high = true;
            Ok(())
        }
    }

    impl InputPin for MockPin {
        fn is_high(&mut self) -> Result<bool, Self::Error> {
            Ok(self.high)
        }

        fn is_low(&mut self) -> Result<bool, Self::Error> {
            Ok(!self.high)
        }
    }

    impl Wait for MockPin {
        async fn wait_for_high(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn wait_for_low(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn wait_for_rising_edge(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn wait_for_falling_edge(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn wait_for_any_edge(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct MockSpiError;

    impl embedded_hal::spi::Error for MockSpiError {
        fn kind(&self) -> embedded_hal::spi::ErrorKind {
            embedded_hal::spi::ErrorKind::Other
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SpiBehavior {
        Ok,
        FailOnCall(usize),
        HangOnCall(usize),
    }

    struct MockSpi {
        behavior: SpiBehavior,
        calls: usize,
        bytes_written: usize,
    }

    impl MockSpi {
        fn new(behavior: SpiBehavior) -> Self {
            Self {
                behavior,
                calls: 0,
                bytes_written: 0,
            }
        }
    }

    impl SpiErrorType for MockSpi {
        type Error = MockSpiError;
    }

    impl SpiBus<u8> for MockSpi {
        async fn read(&mut self, _words: &mut [u8]) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
            let call = self.calls;
            self.calls += 1;

            match self.behavior {
                SpiBehavior::Ok => {
                    self.bytes_written += words.len();
                    Ok(())
                }
                SpiBehavior::FailOnCall(n) if n == call => Err(MockSpiError),
                SpiBehavior::HangOnCall(n) if n == call => pending::<Result<(), Self::Error>>().await,
                _ => {
                    self.bytes_written += words.len();
                    Ok(())
                }
            }
        }

        async fn transfer(&mut self, _read: &mut [u8], _write: &[u8]) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn transfer_in_place(&mut self, _words: &mut [u8]) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn flush(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    fn poll_once<F: Future>(future: F) -> Poll<F::Output> {
        let mut future = pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        future.as_mut().poll(&mut cx)
    }

    fn ready<F: Future>(future: F) -> F::Output {
        match poll_once(future) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("expected future to complete immediately"),
        }
    }

    fn make_bus(behavior: SpiBehavior) -> SpiEpdBus<MockSpi, MockPin, MockPin, MockPin, MockPin> {
        SpiEpdBus::new(
            MockSpi::new(behavior),
            MockPin::new(false),
            MockPin::new(false),
            MockPin::new(false),
            MockPin::new(true),
        )
        .unwrap()
    }

    #[test]
    fn data_stream_releases_cs_on_finish() {
        let mut bus = make_bus(SpiBehavior::Ok);

        {
            let mut stream = ready(DataStream::open(&mut bus)).unwrap();
            ready(stream.write(&[1, 2, 3])).unwrap();
            ready(stream.write(&[4, 5])).unwrap();
            stream.finish().unwrap();
        }

        assert!(bus.cs.high);
        assert!(!bus.stream_open);
        assert_eq!(bus.spi.bytes_written, 5);
        ready(bus.command(0x12)).unwrap();
    }

    #[test]
    fn data_stream_releases_cs_when_dropped_without_finish() {
        let mut bus = make_bus(SpiBehavior::Ok);

        {
            let mut stream = ready(DataStream::open(&mut bus)).unwrap();
            ready(stream.write(&[1, 2])).unwrap();
            // intentionally dropped without calling finish()
        }

        assert!(bus.cs.high);
        assert!(!bus.stream_open);
        ready(bus.command(0x12)).unwrap();
    }

    #[test]
    fn data_stream_releases_cs_when_cancelled_mid_write() {
        let mut bus = make_bus(SpiBehavior::HangOnCall(1));

        let poll = poll_once(async {
            let mut stream = DataStream::open(&mut bus).await.unwrap();
            stream.write(&[1, 2]).await.unwrap();
            // Second write hangs forever; polling once and dropping simulates cancellation.
            stream.write(&[3, 4]).await.unwrap();
            stream.finish().unwrap();
        });

        assert!(poll.is_pending());
        assert!(bus.cs.high);
        assert!(!bus.stream_open);
        assert_eq!(bus.spi.bytes_written, 2);

        // Subsequent commands succeed instead of returning StreamAlreadyOpen.
        bus.spi.behavior = SpiBehavior::Ok;
        ready(bus.command(0x12)).unwrap();
    }

    #[test]
    fn data_stream_releases_cs_on_spi_error() {
        let mut bus = make_bus(SpiBehavior::FailOnCall(0));

        {
            let mut stream = ready(DataStream::open(&mut bus)).unwrap();
            let err = ready(stream.write(&[1, 2, 3])).unwrap_err();
            assert!(matches!(err, Error::Spi(MockSpiError)));
        }

        assert!(bus.cs.high);
        assert!(!bus.stream_open);
        bus.spi.behavior = SpiBehavior::Ok;
        ready(bus.command(0x12)).unwrap();
    }

    #[test]
    fn command_and_data_release_cs_when_cancelled_mid_transfer() {
        let mut bus = make_bus(SpiBehavior::HangOnCall(0));

        assert!(poll_once(bus.command(0x12)).is_pending());
        assert!(bus.cs.high);

        bus.spi.behavior = SpiBehavior::HangOnCall(1);
        assert!(poll_once(bus.data(&[0xaa, 0xbb])).is_pending());
        assert!(bus.cs.high);
    }

    #[test]
    fn nested_command_rejected_while_stream_is_open() {
        let mut bus = make_bus(SpiBehavior::Ok);

        ready(bus.begin_data_stream()).unwrap();
        let err = ready(bus.command(0x12)).unwrap_err();
        assert!(matches!(err, Error::Protocol(ProtocolError::StreamAlreadyOpen)));

        bus.end_data_stream().unwrap();
        ready(bus.command(0x12)).unwrap();
    }
}
