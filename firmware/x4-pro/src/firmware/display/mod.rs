use defmt::Format;
use embedded_hal_async::delay::DelayNs;
use epd_bus::{EpdInterface, PanelError, RefreshMode as EpdRefreshMode, Region};
use ssd1677::{GDEQ0426T82, RefreshMode as SsdRefreshMode, Ssd1677};
use uc8xxx::{UC8179_X4_PRO_800X480, UC8279_X4_PRO_800X480, Uc8xxx, Variant as UcVariant};
use xteink_display_probe::Controller;

#[cfg(feature = "trace")]
use crate::firmware::perf::{DisplayController, ProfiledEpdBus};
use crate::firmware::{
    framebuffer::FramebufferStorage,
    presenter::{FrameUpdate, PresentationMode},
    refresh_policy::{
        BinaryOverGrayMode, BinaryUpdateMode, EInkCapabilities, GrayscaleUpdateMode, RefreshRequest,
    },
};

pub mod power;

#[derive(Debug, Format)]
pub enum Error<E> {
    Ssd1677(ssd1677::Error<E>),
    Uc8xxx(PanelError<E>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresentPower {
    TurnOff,
    KeepOn,
}

impl PresentPower {
    const fn turn_off(self) -> bool {
        matches!(self, Self::TurnOff)
    }
}

pub enum X4Panel {
    Ssd1677(Ssd1677),
    Uc8xxx(Uc8xxx),
}

impl X4Panel {
    pub const fn new(controller: Controller) -> Self {
        match controller {
            Controller::Ssd1677 => Self::Ssd1677(Ssd1677::new(GDEQ0426T82)),
            Controller::Uc8179 => Self::Uc8xxx(Uc8xxx::new(UC8179_X4_PRO_800X480)),
            Controller::Uc8279 => Self::Uc8xxx(Uc8xxx::new(UC8279_X4_PRO_800X480)),
        }
    }

    pub const fn supports_idle_power_hold(&self) -> bool {
        matches!(self, Self::Uc8xxx(panel) if matches!(panel.variant(), UcVariant::Uc8179))
    }

    pub async fn initialize<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        match self {
            X4Panel::Ssd1677(panel) => panel.initialize(bus, delay).await.map_err(Error::Ssd1677),
            X4Panel::Uc8xxx(panel) => panel.initialize(bus, delay).await.map_err(Error::Uc8xxx),
        }
    }

    pub async fn begin_idle_power_off<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        let Self::Uc8xxx(panel) = self else {
            return Ok(());
        };

        #[cfg(feature = "trace")]
        {
            let mut profiled_bus = ProfiledEpdBus::new_preparation(bus);

            panel
                .begin_power_off(&mut profiled_bus, delay)
                .await
                .map_err(Error::Uc8xxx)
        }

        #[cfg(not(feature = "trace"))]
        {
            panel
                .begin_power_off(bus, delay)
                .await
                .map_err(Error::Uc8xxx)
        }
    }

    pub async fn prepare_present<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        let Self::Uc8xxx(panel) = self else {
            return Ok(());
        };

        #[cfg(feature = "trace")]
        {
            let power_off_pending = panel.power_off_pending();

            let mut profiled_bus = ProfiledEpdBus::new_preparation(bus);

            if power_off_pending {
                profiled_bus.expect_power_off_completion();
            }

            panel
                .prepare_power_on(&mut profiled_bus, delay)
                .await
                .map_err(Error::Uc8xxx)
        }

        #[cfg(not(feature = "trace"))]
        {
            panel
                .prepare_power_on(bus, delay)
                .await
                .map_err(Error::Uc8xxx)
        }
    }

    pub async fn present<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        frame: &FramebufferStorage,
        update: FrameUpdate,
        power: PresentPower,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        let present_trace = inkpaper_trace::async_span!(
            target: "display.present",
            "present",
            frame = update.frame_id(),
        );

        #[cfg(feature = "trace")]
        let (result, present_metrics) = {
            let controller = match self {
                Self::Ssd1677(_) => DisplayController::Ssd1677,
                Self::Uc8xxx(panel) => match panel.variant() {
                    UcVariant::Uc8179 => DisplayController::Uc8179,
                    UcVariant::Uc8279X4 => DisplayController::Uc8279,
                },
            };

            let (pending_power_off, pending_power_on) = match self {
                Self::Uc8xxx(panel) => (panel.power_off_pending(), panel.power_on_pending()),
                Self::Ssd1677(_) => (false, false),
            };

            debug_assert!(!(pending_power_off && pending_power_on));

            let mut profiled_bus = ProfiledEpdBus::new(bus, controller, update);

            if pending_power_off {
                profiled_bus.expect_power_off_completion();
            }

            if pending_power_on {
                profiled_bus.expect_power_on_completion();
            }

            let result = self
                .present_inner(&mut profiled_bus, delay, frame, update, power)
                .await;

            let metrics = profiled_bus.finish();

            (result, metrics)
        };

        #[cfg(not(feature = "trace"))]
        let result = self.present_inner(bus, delay, frame, update, power).await;

        // finish the presentation interval before swapping the flight-recorder buffer
        // so this presentation is included in the capture emitted below.
        drop(present_trace);

        #[cfg(feature = "trace")]
        crate::firmware::perf::record_present_metrics(present_metrics);

        #[cfg(feature = "trace")]
        let trace_capture = inkpaper_trace::capture();

        #[cfg(feature = "trace")]
        if let Some(trace_capture) = trace_capture {
            crate::firmware::perf::log_trace(trace_capture);
        }

        result
    }

    async fn present_inner<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        frame: &FramebufferStorage,
        update: FrameUpdate,
        power: PresentPower,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        match update.presentation() {
            PresentationMode::Gray4 => {
                self.present_grayscale(bus, delay, frame, update, power)
                    .await
            }
            PresentationMode::BinaryPreservingGray => {
                self.present_binary_preserving_gray(bus, delay, frame, update, power)
                    .await
            }
            PresentationMode::Binary => {
                let frame = frame.binary_plane();

                match self {
                    Self::Ssd1677(panel) => present_ssd1677(panel, bus, delay, frame, update).await,
                    Self::Uc8xxx(panel) => {
                        present_uc8xxx(panel, bus, delay, frame, update, power.turn_off()).await
                    }
                }
            }
        }
    }

    async fn present_grayscale<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        frame: &FramebufferStorage,
        update: FrameUpdate,
        power: PresentPower,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        let (lsb, msb) = frame.planes();
        let damage = update.physical_damage();

        match self {
            Self::Ssd1677(panel) => {
                let mode = match update.refresh() {
                    RefreshRequest::Full => SsdRefreshMode::Full,
                    RefreshRequest::Fast => SsdRefreshMode::Fast,
                };

                panel
                    .display_grayscale_window(
                        bus,
                        delay,
                        lsb,
                        msb,
                        Region::new(damage.x, damage.y, damage.width, damage.height),
                        mode,
                        true,
                    )
                    .await
                    .map(|_| ())
                    .map_err(Error::Ssd1677)
            }
            Self::Uc8xxx(panel) => {
                let mode = match update.refresh() {
                    RefreshRequest::Full => EpdRefreshMode::Full,
                    RefreshRequest::Fast => EpdRefreshMode::Fast,
                };
                let turn_off = match panel.variant() {
                    UcVariant::Uc8179 => power.turn_off(),
                    UcVariant::Uc8279X4 => true,
                };

                panel
                    .display_grayscale_window(
                        bus,
                        delay,
                        lsb,
                        msb,
                        Region::new(damage.x, damage.y, damage.width, damage.height),
                        mode,
                        turn_off,
                    )
                    .await
                    .map(|_| ())
                    .map_err(Error::Uc8xxx)
            }
        }
    }

    async fn present_binary_preserving_gray<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        frame: &FramebufferStorage,
        update: FrameUpdate,
        power: PresentPower,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        // a requested maintenance/full refresh should use the established grayscale-preserving
        // full path rather than the short differential window waveform.
        if update.refresh() == RefreshRequest::Full {
            return self
                .present_grayscale(bus, delay, frame, update, power)
                .await;
        }

        let (lsb, msb) = frame.planes();
        let damage = update.physical_damage();

        match self {
            Self::Ssd1677(panel) => panel
                .display_grayscale_window(
                    bus,
                    delay,
                    lsb,
                    msb,
                    Region::new(damage.x, damage.y, damage.width, damage.height),
                    SsdRefreshMode::Fast,
                    true,
                )
                .await
                .map(|_| ())
                .map_err(Error::Ssd1677),

            Self::Uc8xxx(panel) => {
                let turn_off = match panel.variant() {
                    UcVariant::Uc8179 => power.turn_off(),
                    UcVariant::Uc8279X4 => true,
                };

                panel
                    .display_binary_window_preserving_grayscale(
                        bus,
                        delay,
                        lsb,
                        msb,
                        Region::new(damage.x, damage.y, damage.width, damage.height),
                        turn_off,
                    )
                    .await
                    .map(|_| ())
                    .map_err(Error::Uc8xxx)
            }
        }
    }

    pub async fn deep_sleep<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        match self {
            Self::Ssd1677(panel) => panel.deep_sleep(bus, delay).await.map_err(Error::Ssd1677),
            Self::Uc8xxx(panel) => panel.deep_sleep(bus, delay).await.map_err(Error::Uc8xxx),
        }
    }

    pub const fn capabilities(&self) -> EInkCapabilities {
        match self {
            Self::Ssd1677(_) => EInkCapabilities::new(
                BinaryUpdateMode::Window,
                BinaryOverGrayMode::NativeWindow,
                GrayscaleUpdateMode::Window,
            ),
            Self::Uc8xxx(panel) => match panel.variant() {
                UcVariant::Uc8179 => EInkCapabilities::new(
                    BinaryUpdateMode::FullPlane,
                    BinaryOverGrayMode::PreconditionedWindow,
                    GrayscaleUpdateMode::Window,
                ),
                UcVariant::Uc8279X4 => EInkCapabilities::new(
                    BinaryUpdateMode::FullPlane,
                    BinaryOverGrayMode::Unsupported,
                    GrayscaleUpdateMode::FullPlane,
                ),
            },
        }
    }
}

async fn present_ssd1677<B, D>(
    panel: &mut Ssd1677,
    bus: &mut B,
    delay: &mut D,
    frame: &[u8],
    update: FrameUpdate,
) -> Result<(), Error<B::Error>>
where
    B: EpdInterface,
    D: DelayNs,
{
    match update.refresh() {
        RefreshRequest::Full => panel
            .display(bus, delay, frame, None, SsdRefreshMode::Full)
            .await
            .map_err(Error::Ssd1677),

        RefreshRequest::Fast => {
            if update.is_full_damage() {
                return panel
                    .display(bus, delay, frame, None, SsdRefreshMode::Fast)
                    .await
                    .map_err(Error::Ssd1677);
            }

            let region = update.physical_damage();

            panel
                .display_window(
                    bus,
                    delay,
                    frame,
                    None,
                    Region::new(region.x, region.y, region.width, region.height),
                )
                .await
                .map(|_| ())
                .map_err(Error::Ssd1677)
        }
    }
}

async fn present_uc8xxx<B, D>(
    panel: &mut Uc8xxx,
    bus: &mut B,
    delay: &mut D,
    frame: &[u8],
    update: FrameUpdate,
    turn_off: bool,
) -> Result<(), Error<B::Error>>
where
    B: EpdInterface,
    D: DelayNs,
{
    let mode = match update.refresh() {
        RefreshRequest::Full => EpdRefreshMode::Full,
        RefreshRequest::Fast => EpdRefreshMode::Fast,
    };
    let turn_off = match panel.variant() {
        UcVariant::Uc8179 => turn_off,
        UcVariant::Uc8279X4 => true,
    };

    panel
        .display(bus, delay, frame, mode, turn_off)
        .await
        .map_err(Error::Uc8xxx)
}
