use std::{
    cell::{Cell, RefCell},
    time::{Duration, Instant},
};

use inkpaper_app::{ClockSyncFailure, InkPaperApp, WifiJoinFailure, WifiJoinPlan, WifiNetwork};
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
    clock_sync_plan: RefCell<Option<WifiJoinPlan>>,
    join: Cell<Option<Instant>>,
}

impl SimulatedRadio {
    pub(super) fn start_scan(&self) {
        self.scan.set(Some(Instant::now()));
    }

    pub(super) fn start_clock_sync(&self, plan: WifiJoinPlan) {
        self.clock_sync.set(Some(Instant::now()));
        self.clock_sync_plan.replace(Some(plan));
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
        let synced = take_due(&self.clock_sync, CLOCK_SYNC_DURATION)
            .then(|| self.clock_sync_plan.take())
            .flatten();
        let joined = take_due(&self.join, JOIN_DURATION);

        if !scanned && synced.is_none() && !joined {
            return false;
        }

        runtime
            .update(app, move |app, cx| {
                if scanned {
                    app.apply_wifi_scan_result(Ok(scan_results()), cx);
                }

                if let Some(plan) = synced {
                    let (joined, result) = clock_sync_outcome(&plan);
                    app.apply_clock_sync_result(joined, result, cx);
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

/// Joins the connected network, or when `INKPAPER_SIM_CLOCK_SYNC_FAIL` is set
/// and it can't be joined, the strongest fallback in range. Returns the
/// network joined. A successful sync leaves the simulated clock as it is.
fn clock_sync_outcome(plan: &WifiJoinPlan) -> (Option<&str>, Result<(), ClockSyncFailure>) {
    let connected_fails = std::env::var_os("INKPAPER_SIM_CLOCK_SYNC_FAIL").is_some();

    let joined = match plan.connected() {
        Some(connected) if !connected_fails => Some(connected),
        _ => plan.fallbacks_in_range(&scan_results()).first().copied(),
    };

    match joined {
        Some(network) => (Some(network.ssid()), Ok(())),
        None => (None, Err(ClockSyncFailure::Join)),
    }
}
