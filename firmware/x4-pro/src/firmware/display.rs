use embedded_hal_async::delay::DelayNs;
use epd_bus::EpdInterface;
use ssd1677::{GDEQ0426T82, RefreshMode as SsdRefreshMode, Region as SsdRegion, Ssd1677};
use uc8179::{RefreshMode as Uc8179RefreshMode, Uc8179, X4_PRO_800X480 as UC8179_X4_PRO};
use uc8279_x4::{RefreshMode as Uc8279RefreshMode, Uc8279X4, X4_PRO_800X480 as UC8279_X4_PRO};
use xteink_display_probe::Controller;

use crate::firmware::presenter::{FrameUpdate, RefreshRequest};

#[derive(Debug)]
pub enum Error<E> {
    Ssd1677(ssd1677::Error<E>),
    Uc8179(uc8179::Error<E>),
    Uc8279(uc8279_x4::Error<E>),
}

pub enum X4Panel {
    Ssd1677(Ssd1677),
    Uc8179(Uc8179),
    Uc8279(Uc8279X4),
}

impl X4Panel {
    pub const fn new(controller: Controller) -> Self {
        match controller {
            Controller::Ssd1677 => Self::Ssd1677(Ssd1677::new(GDEQ0426T82)),
            Controller::Uc8179 => Self::Uc8179(Uc8179::new(UC8179_X4_PRO)),
            Controller::Uc8279 => Self::Uc8279(Uc8279X4::new(UC8279_X4_PRO)),
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
            X4Panel::Ssd1677(panel) => panel.initialize(bus, delay).await.map_err(Error::Ssd1677),
            X4Panel::Uc8179(panel) => panel.initialize(bus, delay).await.map_err(Error::Uc8179),
            X4Panel::Uc8279(panel) => panel.initialize(bus, delay).await.map_err(Error::Uc8279),
        }
    }

    pub async fn present<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        frame: &[u8],
        update: FrameUpdate,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        match self {
            Self::Ssd1677(panel) => present_ssd1677(panel, bus, delay, frame, update).await,
            Self::Uc8179(panel) => present_uc8179(panel, bus, delay, frame, update).await,
            Self::Uc8279(panel) => present_uc8279(panel, bus, delay, frame, update).await,
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
            X4Panel::Uc8179(panel) => panel.deep_sleep(bus, delay).await.map_err(Error::Uc8179),
            X4Panel::Uc8279(panel) => panel.deep_sleep(bus, delay).await.map_err(Error::Uc8279),
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
                    SsdRegion::new(region.x, region.y, region.width, region.height),
                )
                .await
                .map(|_| ())
                .map_err(Error::Ssd1677)
        }
    }
}

async fn present_uc8179<B, D>(
    panel: &mut Uc8179,
    bus: &mut B,
    delay: &mut D,
    frame: &[u8],
    update: FrameUpdate,
) -> Result<(), Error<B::Error>>
where
    B: EpdInterface,
    D: DelayNs,
{
    let mode = match update.refresh() {
        RefreshRequest::Full => Uc8179RefreshMode::Full,
        RefreshRequest::Fast => Uc8179RefreshMode::Fast,
    };

    // UC8179 currently uses a whole-plane differential FAST refresh.
    // do NOT program a sub-window here yet. FreeInk's hardware path also deliberately
    // uses PTIN/PTOUT without PTL for ordinary B/W FAST refreshes.
    panel
        .display(bus, delay, frame, mode, true)
        .await
        .map_err(Error::Uc8179)
}

async fn present_uc8279<B, D>(
    panel: &mut Uc8279X4,
    bus: &mut B,
    delay: &mut D,
    frame: &[u8],
    update: FrameUpdate,
) -> Result<(), Error<B::Error>>
where
    B: EpdInterface,
    D: DelayNs,
{
    let mode = match update.refresh() {
        RefreshRequest::Full => Uc8279RefreshMode::Full,

        RefreshRequest::Fast => Uc8279RefreshMode::Fast,
    };

    // the X4 UC8279 variant already owns its required full PTL sequence.
    // keep that controller-specific behavior inside its driver.
    panel
        .display(bus, delay, frame, mode, true)
        .await
        .map_err(Error::Uc8279)
}
