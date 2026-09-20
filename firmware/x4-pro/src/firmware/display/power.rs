use core::future::pending;

use embassy_time::{Duration, Instant, Timer};
use embedded_hal_async::delay::DelayNs;
use epd_bus::EpdInterface;

use crate::firmware::{
    display::{Error as DisplayError, PresentPower, X4Panel},
    framebuffer::FramebufferStorage,
    presenter::FrameUpdate,
};

const UC8179_IDLE_GRACE: Duration = Duration::from_millis(250);

#[derive(Debug, Default)]
pub(crate) struct DisplayPowerManager {
    idle_deadline: Option<Instant>,
}

impl DisplayPowerManager {
    pub(crate) async fn prepare<B, D>(
        &mut self,
        panel: &mut X4Panel,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), DisplayError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        // Do not cancel the current absolute idle deadline here.
        //
        // While rendering, the main task is not polling the deadline future anyway.
        // If rendering produces a frame, present() rearms the grace period from
        // the new completion time. If rendering produces no physical frame, the old
        // deadline is retained and will fire when the main loop resumes.
        panel.prepare_present(bus, delay).await
    }

    pub(crate) async fn present_initial<B, D>(
        &mut self,
        panel: &mut X4Panel,
        bus: &mut B,
        delay: &mut D,
        frame: &FramebufferStorage,
        update: FrameUpdate,
    ) -> Result<(), DisplayError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.idle_deadline = None;

        // Keep boot behavior conservative. The main event loop is not running yet, so
        // there is no useful burst timeout to service after this frame.
        panel
            .present(bus, delay, frame, update, PresentPower::TurnOff)
            .await
    }

    pub(crate) async fn present<B, D>(
        &mut self,
        panel: &mut X4Panel,
        bus: &mut B,
        delay: &mut D,
        frame: &FramebufferStorage,
        update: FrameUpdate,
    ) -> Result<(), DisplayError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        let hold_power = panel.supports_idle_power_hold();

        let power = if hold_power {
            PresentPower::KeepOn
        } else {
            PresentPower::TurnOff
        };

        let result = panel.present(bus, delay, frame, update, power).await;

        if result.is_ok() && hold_power {
            self.arm_idle_at(Instant::now());
        } else {
            self.idle_deadline = None;
        }

        result
    }

    pub(crate) async fn wait_idle_timeout(&self) {
        match self.idle_deadline {
            Some(deadline) => {
                Timer::at(deadline).await;
            }

            None => {
                pending::<()>().await;
            }
        }
    }

    pub(crate) async fn handle_idle_timeout<B, D>(
        &mut self,
        panel: &mut X4Panel,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), DisplayError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        let Some(deadline) = self.idle_deadline else {
            return Ok(());
        };

        // Normally wait_idle_timeout() guarantees this. Keep the check so the state machine
        // remains correct if this method is ever called directly.
        if Instant::now() < deadline {
            return Ok(());
        }

        self.idle_deadline = None;

        // UC8179 PowerOff is itself deferred. This sends the command and immediately gives
        // control back to the main loop; it does not add the old ~80 ms shutdown wait here.
        panel.begin_idle_power_off(bus, delay).await
    }

    fn arm_idle_at(&mut self, now: Instant) {
        self.idle_deadline = Some(now.saturating_add(UC8179_IDLE_GRACE));
    }
}
