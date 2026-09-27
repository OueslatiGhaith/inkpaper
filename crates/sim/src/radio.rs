use std::{
    cell::Cell,
    time::{Duration, Instant},
};

use inkpaper_app::{ClockSyncFailure, InkPaperApp, WifiCredentials, WifiNetwork};
use inkpaper_ui::prelude::*;

// long enough to see the scanning and syncing states
const SCAN_DURATION: Duration = Duration::from_millis(1500);
const CLOCK_SYNC_DURATION: Duration = Duration::from_secs(2);

/// Stands in for the WiFi radio: work the platform starts finishes a moment
/// later, when the main loop calls [`SimulatedRadio::finish_due`].
#[derive(Default)]
pub(super) struct SimulatedRadio {
    scan: Cell<Option<Instant>>,
    clock_sync: Cell<Option<Instant>>,
}

impl SimulatedRadio {
    pub(super) fn start_scan(&self) {
        self.scan.set(Some(Instant::now()));
    }

    pub(super) fn start_clock_sync(&self) {
        self.clock_sync.set(Some(Instant::now()));
    }

    /// Reports work whose time has come. Returns whether anything finished.
    pub(super) fn finish_due(
        &self,
        runtime: &mut impl RuntimeApi,
        app: Entity<InkPaperApp>,
    ) -> bool {
        let scanned = take_due(&self.scan, SCAN_DURATION);
        let synced = take_due(&self.clock_sync, CLOCK_SYNC_DURATION);

        if !scanned && !synced {
            return false;
        }

        runtime
            .update(app, move |app, cx| {
                if scanned {
                    app.apply_wifi_scan_result(Ok(scan_results()), cx);
                }

                if synced {
                    app.apply_clock_sync_result(clock_sync_result(), cx);
                }
            })
            .expect("application root must remain available");

        true
    }
}

fn take_due(started: &Cell<Option<Instant>>, duration: Duration) -> bool {
    match started.get() {
        Some(at) if at.elapsed() >= duration => {
            started.set(None);
            true
        }
        _ => false,
    }
}

/// The network from `INKPAPER_WIFI_SSID` and `INKPAPER_WIFI_PASS`, as on the
/// device, until passwords can be typed.
pub(super) fn build_credentials() -> Option<WifiCredentials> {
    let ssid = std::env::var("INKPAPER_WIFI_SSID").ok()?;
    let password = std::env::var("INKPAPER_WIFI_PASS").unwrap_or_default();

    WifiCredentials::new(ssid, password)
}

/// A few networks around the desk, plus the build-time one if set.
fn scan_results() -> Vec<WifiNetwork> {
    let mut networks = vec![
        WifiNetwork::new("Cafe Corner", -58, false),
        WifiNetwork::new("Neighbors 5G", -71, true),
        WifiNetwork::new("Library Guest", -80, false),
        WifiNetwork::new("Office", -66, true),
        WifiNetwork::new("Office", -49, true),
        WifiNetwork::new("", -45, true),
    ];

    if let Some(credentials) = build_credentials() {
        networks.push(WifiNetwork::new(
            credentials.ssid(),
            -40,
            !credentials.password().is_empty(),
        ));
    }

    networks
}

/// Succeeds, or fails to join WiFi when `INKPAPER_SIM_CLOCK_SYNC_FAIL` is set.
/// A successful sync leaves the simulated clock as it is.
fn clock_sync_result() -> Result<(), ClockSyncFailure> {
    match std::env::var_os("INKPAPER_SIM_CLOCK_SYNC_FAIL") {
        Some(_) => Err(ClockSyncFailure::Join),
        None => Ok(()),
    }
}
