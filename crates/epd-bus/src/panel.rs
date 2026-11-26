use embedded_hal_async::delay::DelayNs;

use crate::{EpdInterface, Region};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RefreshMode {
    Full,
    /// Strong charge-scrubbing refresh.
    Clean,
    /// Differential page/UI refresh.
    Fast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum BinaryUpdateMode {
    FullPlane,
    Window,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum BinaryOverGrayMode {
    Unsupported,
    NativeWindow,
    PreconditionedWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum GrayscaleUpdateMode {
    FullPlane,
    Window,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PanelCapabilities {
    binary_update: BinaryUpdateMode,
    binary_over_gray: BinaryOverGrayMode,
    grayscale_update: GrayscaleUpdateMode,
}

impl PanelCapabilities {
    pub const fn new(
        binary_update: BinaryUpdateMode,
        binary_over_gray: BinaryOverGrayMode,
        grayscale_update: GrayscaleUpdateMode,
    ) -> Self {
        Self {
            binary_update,
            binary_over_gray,
            grayscale_update,
        }
    }

    pub const fn binary_update(self) -> BinaryUpdateMode {
        self.binary_update
    }

    pub const fn binary_over_gray(self) -> BinaryOverGrayMode {
        self.binary_over_gray
    }

    pub const fn grayscale_update(self) -> GrayscaleUpdateMode {
        self.grayscale_update
    }

    pub const fn supports_partial_grayscale(self) -> bool {
        matches!(self.grayscale_update, GrayscaleUpdateMode::Window)
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum PanelError<E> {
    Bus(E),
    InvalidGeometry,
    InvalidFrameLength { expected: usize, actual: usize },
    InvalidPreviousFrameLength { expected: usize, actual: usize },
    EmptyRegion,
    RegionOutOfBounds { region: Region },
}

#[allow(async_fn_in_trait)]
pub trait EpdPanel {
    fn capabilities(&self) -> PanelCapabilities;

    fn supports_idle_power_hold(&self) -> bool {
        false
    }

    async fn initialize<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs;

    async fn prepare_power_on<B, D>(
        &mut self,
        _bus: &mut B,
        _delay: &mut D,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        Ok(())
    }

    async fn begin_idle_power_off<B, D>(
        &mut self,
        _bus: &mut B,
        _delay: &mut D,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        Ok(())
    }

    async fn display_binary<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        frame: &[u8],
        damage: Option<Region>,
        mode: RefreshMode,
        turn_off: bool,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs;

    #[allow(clippy::too_many_arguments)]
    async fn display_grayscale<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        lsb: &[u8],
        msb: &[u8],
        damage: Region,
        mode: RefreshMode,
        turn_off: bool,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs;

    async fn display_binary_preserving_grayscale<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        lsb: &[u8],
        msb: &[u8],
        damage: Region,
        turn_off: bool,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs;

    async fn deep_sleep<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs;
}
