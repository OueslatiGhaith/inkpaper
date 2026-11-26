use embedded_hal_async::delay::DelayNs;
use epd_bus::{EpdInterface, EpdPanel, PanelError, RefreshMode, Region};
use ssd1677::{GDEQ0426T82, Ssd1677};
use uc8xxx::{UC8179_X4_PRO_800X480, UC8279_X4_PRO_800X480, Uc8xxx};
use xteink_display_probe::Controller;

#[cfg(feature = "trace")]
use crate::firmware::perf::ProfiledEpdBus;
use crate::firmware::{
    framebuffer::FramebufferStorage,
    presenter::{FrameUpdate, PresentationMode},
    refresh_policy::{EInkCapabilities, RefreshRequest},
};

pub mod power;

pub type Error<E> = PanelError<E>;

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

    pub fn supports_idle_power_hold(&self) -> bool {
        match self {
            Self::Ssd1677(panel) => panel.supports_idle_power_hold(),
            Self::Uc8xxx(panel) => panel.supports_idle_power_hold(),
        }
    }

    pub fn capabilities(&self) -> EInkCapabilities {
        match self {
            Self::Ssd1677(panel) => panel.capabilities(),
            Self::Uc8xxx(panel) => panel.capabilities(),
        }
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
            Self::Ssd1677(panel) => EpdPanel::initialize(panel, bus, delay).await,
            Self::Uc8xxx(panel) => EpdPanel::initialize(panel, bus, delay).await,
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
        if !self.supports_idle_power_hold() {
            return Ok(());
        }

        #[cfg(feature = "trace")]
        let mut profiled_bus = ProfiledEpdBus::new(bus);
        #[cfg(feature = "trace")]
        let bus = &mut profiled_bus;

        match self {
            Self::Ssd1677(panel) => panel.begin_idle_power_off(bus, delay).await,
            Self::Uc8xxx(panel) => panel.begin_idle_power_off(bus, delay).await,
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
        if !self.supports_idle_power_hold() {
            return Ok(());
        }

        #[cfg(feature = "trace")]
        let mut profiled_bus = ProfiledEpdBus::new(bus);
        #[cfg(feature = "trace")]
        let bus = &mut profiled_bus;

        match self {
            Self::Ssd1677(panel) => panel.prepare_power_on(bus, delay).await,
            Self::Uc8xxx(panel) => panel.prepare_power_on(bus, delay).await,
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
            let mut profiled_bus = ProfiledEpdBus::new(bus);
            let result = self
                .present_inner(&mut profiled_bus, delay, frame, update, power)
                .await;
            let metrics = profiled_bus.finish();

            (result, metrics)
        };

        #[cfg(not(feature = "trace"))]
        let result = self.present_inner(bus, delay, frame, update, power).await;

        // Finish the presentation interval before swapping the flight-recorder buffer
        // so this presentation is included in the capture emitted below.
        drop(present_trace);

        #[cfg(feature = "trace")]
        crate::firmware::perf::record_present_metrics(present_metrics);

        #[cfg(feature = "trace")]
        if let Some(trace_capture) = inkpaper_trace::capture() {
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
        match self {
            Self::Ssd1677(panel) => present_panel(panel, bus, delay, frame, update, power).await,
            Self::Uc8xxx(panel) => present_panel(panel, bus, delay, frame, update, power).await,
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
            Self::Ssd1677(panel) => EpdPanel::deep_sleep(panel, bus, delay).await,
            Self::Uc8xxx(panel) => EpdPanel::deep_sleep(panel, bus, delay).await,
        }
    }
}

async fn present_panel<P, B, D>(
    panel: &mut P,
    bus: &mut B,
    delay: &mut D,
    frame: &FramebufferStorage,
    update: FrameUpdate,
    power: PresentPower,
) -> Result<(), Error<B::Error>>
where
    P: EpdPanel,
    B: EpdInterface,
    D: DelayNs,
{
    let mode = match update.refresh() {
        RefreshRequest::Full => RefreshMode::Full,
        RefreshRequest::Fast => RefreshMode::Fast,
    };
    let damage = update.physical_damage();
    let region = Region::new(damage.x, damage.y, damage.width, damage.height);

    match update.presentation() {
        PresentationMode::Binary => {
            let binary_damage = if update.is_full_damage() {
                None
            } else {
                Some(region)
            };

            panel
                .display_binary(
                    bus,
                    delay,
                    frame.binary_plane(),
                    binary_damage,
                    mode,
                    power.turn_off(),
                )
                .await
        }
        PresentationMode::Gray4 => {
            let (lsb, msb) = frame.planes();
            panel
                .display_grayscale(bus, delay, lsb, msb, region, mode, power.turn_off())
                .await
        }
        PresentationMode::BinaryPreservingGray => {
            let (lsb, msb) = frame.planes();
            if update.refresh() == RefreshRequest::Full {
                panel
                    .display_grayscale(bus, delay, lsb, msb, region, mode, power.turn_off())
                    .await
            } else {
                panel
                    .display_binary_preserving_grayscale(
                        bus,
                        delay,
                        lsb,
                        msb,
                        region,
                        power.turn_off(),
                    )
                    .await
            }
        }
    }
}
