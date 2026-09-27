use std::{
    cell::Cell,
    time::{Duration, Instant},
};

use inkpaper_app::{ClockSyncFailure, InkPaperApp, WifiJoinFailure, WifiNetwork};
use inkpaper_ui::prelude::*;

// long enough to see the scanning and syncing states
const SCAN_DURATION: Duration = Duration::from_millis(1500);
const CLOCK_SYNC_DURATION: Duration = Duration::from_secs(2);
const JOIN_DURATION: Duration = Duration::from_secs(2);

/// Stands in for the WiFi radio: work the platform starts finishes a moment
/// later, when the main loop calls [`SimulatedRadio::finish_due`].
#[derive(Default)]
pub(super) struct SimulatedRadio {
    scan: Cell<Option<Instant>>,
    clock_sync: Cell<Option<Instant>>,
    join: Cell<Option<Instant>>,
}

impl SimulatedRadio {
    pub(super) fn start_scan(&self) {
        self.scan.set(Some(Instant::now()));
    }

    pub(super) fn start_clock_sync(&self) {
        self.clock_sync.set(Some(Instant::now()));
    }

    pub(super) fn start_join(&self) {
        self.join.set(Some(Instant::now()));
    }

    /// Reports work whose time has come. Returns whether anything finished.
    pub(super) fn finish_due(
        &self,
        runtime: &mut impl RuntimeApi,
        app: Entity<InkPaperApp>,
    ) -> bool {
        let scanned = take_due(&self.scan, SCAN_DURATION);
        let synced = take_due(&self.clock_sync, CLOCK_SYNC_DURATION);
        let joined = take_due(&self.join, JOIN_DURATION);

        if !scanned && !synced && !joined {
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

                if joined {
                    app.apply_wifi_join_result(join_result(), cx);
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

/// A few networks around the desk.
fn scan_results() -> Vec<WifiNetwork> {
    vec![
        WifiNetwork::new("Cafe Corner", -58, false),
        WifiNetwork::new("Neighbors 5G", -71, true),
        WifiNetwork::new("Library Guest", -80, false),
        WifiNetwork::new("Office", -66, true),
        WifiNetwork::new("Office", -49, true),
        WifiNetwork::new("", -45, true),
    ]
}

/// Succeeds, or turns the password away when `INKPAPER_SIM_WIFI_JOIN_FAIL` is
/// set.
fn join_result() -> Result<(), WifiJoinFailure> {
    match std::env::var_os("INKPAPER_SIM_WIFI_JOIN_FAIL") {
        Some(_) => Err(WifiJoinFailure::Rejected),
        None => Ok(()),
    }
}

/// Succeeds, or fails to join WiFi when `INKPAPER_SIM_CLOCK_SYNC_FAIL` is set.
/// A successful sync leaves the simulated clock as it is.
fn clock_sync_result() -> Result<(), ClockSyncFailure> {
    match std::env::var_os("INKPAPER_SIM_CLOCK_SYNC_FAIL") {
        Some(_) => Err(ClockSyncFailure::Join),
        None => Ok(()),
    }
}
