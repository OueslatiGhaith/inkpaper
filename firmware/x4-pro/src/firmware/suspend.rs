use defmt::{info, warn};
use embedded_hal_async::delay::DelayNs;
use epd_bus::EpdInterface;
use inkpaper_app::{AppService, InkPaperApp};
use inkpaper_ui::prelude::*;
use xteink_display_probe::ProbeIo;

use crate::firmware::{
    display::{X4Panel, power::DisplayPowerManager},
    framebuffer::FramebufferStorage,
    frontlight::frontlight_off_and_wait,
    platform::X4Platform,
    power::PowerRails,
    power_button::ENTER_DEEP_SLEEP,
    presenter::{Presenter, UiRuntime},
    sleep_pins::hold_for_deep_sleep,
    storage,
};

pub(crate) struct SuspendContext<'a, 'd, B, D> {
    pub(crate) runtime: &'a mut UiRuntime,
    pub(crate) app: Entity<InkPaperApp>,
    pub(crate) app_service: &'a mut AppService<X4Platform>,
    pub(crate) presenter: &'a mut Presenter,
    pub(crate) display_power: &'a mut DisplayPowerManager,
    pub(crate) panel: &'a mut X4Panel,
    pub(crate) bus: &'a mut B,
    pub(crate) delay: &'a mut D,
    pub(crate) frame: &'a mut FramebufferStorage,
    pub(crate) rails: &'a mut PowerRails<'d>,
}

pub(crate) async fn enter<B, D>(context: SuspendContext<'_, '_, B, D>) -> !
where
    B: EpdInterface,
    D: DelayNs,
{
    let SuspendContext {
        runtime,
        app,
        app_service,
        presenter,
        display_power,
        panel,
        bus,
        delay,
        frame,
        rails,
    } = context;

    prepare_sleep_screen(
        runtime,
        app,
        presenter,
        display_power,
        panel,
        bus,
        delay,
        frame,
    )
    .await;

    info!("suspend: turning frontlight off");
    frontlight_off_and_wait().await;

    info!("suspend: flushing application state");

    if !app_service.flush().await {
        warn!("application state was not fully persisted");
    }

    info!("suspend: shutting down storage");
    storage::shutdown_and_wait().await;
    info!("suspend: storage shutdown complete");

    info!("suspend: putting display controller to sleep");

    // ORDER MATTERS:
    // step 1:
    // tell the actual display controller to enter its own low-power state
    // while SPI, RESET, and the board rails are all still operational
    panel
        .deep_sleep(bus, delay)
        .await
        .unwrap_or_else(|_| panic!("failed to put display controller to sleep"));

    info!("suspend: display controller asleep");

    // step 2:
    // the X4 PRO keeps the panel rail powered in deep sleep. Force RESET high
    // before latching the pin so a sleeping UC controller can't drift back into
    // an active state
    bus.reset_high()
        .unwrap_or_else(|_| panic!("failed to hold display reset high"));

    // step 3:
    // latch GPIO1, GPIO2, GPIO5, and GPIO14 while they are actively driven
    // to those known states.
    // the RTC pad-hold bits survive the ESP32-S3 deep-sleep interval and remain
    // set until the next boot deliberately releases them
    rails.prepare_for_deep_sleep();
    hold_for_deep_sleep();

    info!("suspend: board pins latched for deep sleep");

    // step 5:
    // the power task owns GPIO3 and LPWR. It waits for the current button press
    // to be released, arms EXT0 LOW, then performs the final SoC deep-sleep
    // transition
    ENTER_DEEP_SLEEP.signal(());

    // `power_button_task()` will take the MCU into deep sleep.
    // nothing in this task should touch the hardware again
    loop {
        core::future::pending::<()>().await;
    }
}

async fn prepare_sleep_screen<B, D>(
    runtime: &mut UiRuntime,
    app: Entity<InkPaperApp>,
    presenter: &mut Presenter,
    display_power: &mut DisplayPowerManager,
    panel: &mut X4Panel,
    bus: &mut B,
    delay: &mut D,
    frame: &mut FramebufferStorage,
) where
    B: EpdInterface,
    D: DelayNs,
{
    info!("suspend: preparing sleep screen");

    if runtime
        .update(app, |app, cx| app.prepare_for_sleep(cx))
        .is_err()
    {
        warn!("failed to prepare sleep screen");
        return;
    }

    if runtime.render_invalidation().is_none() {
        warn!("sleep screen did not invalidate display");
        return;
    }

    display_power
        .prepare(panel, bus, delay)
        .await
        .unwrap_or_else(|_| panic!("failed to prepare display for sleep screen"));

    let Some(update) = presenter.render_pending(runtime, frame, panel.capabilities()) else {
        warn!("sleep screen produced no display update");
        return;
    };

    info!("suspend: presenting sleep screen");

    display_power
        .present_sleep(panel, bus, delay, frame, update)
        .await
        .unwrap_or_else(|_| panic!("failed to present sleep screen"));

    info!("suspend: sleep screen displayed");
}
