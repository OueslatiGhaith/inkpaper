use embassy_time::{Duration, Instant, Timer};

/// Enter deep sleep after this long without user input.
const AUTO_SLEEP_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Inactivity countdown for automatic deep sleep.
///
/// Only user input restarts the countdown. Battery, RTC and USB updates do not.
pub(crate) struct AutoSleep {
    deadline: Instant,
}

impl AutoSleep {
    pub(crate) fn new() -> Self {
        Self {
            deadline: Instant::now() + AUTO_SLEEP_TIMEOUT,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.deadline = Instant::now() + AUTO_SLEEP_TIMEOUT;
    }

    pub(crate) async fn wait(&self) {
        Timer::at(self.deadline).await;
    }
}
