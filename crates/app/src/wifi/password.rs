use alloc::string::String;

use crate::keyboard::{Key, KeyResult, KeyboardState};

use super::{
    MAX_PASSWORD_BYTES, MIN_PASSWORD_BYTES, WifiCredentials, WifiJoinFailure, WifiJoinToken,
};

/// Where a typed password is in being tried, like crosspoint, which joins
/// the network before saving it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JoinStatus {
    #[default]
    Typing,
    Checking,
    Failed(WifiJoinFailure),
}

/// Outcome of applying a join result to the active password entry.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum JoinOutcome {
    Ignored,
    Failed,
    Succeeded(WifiCredentials),
}

/// The password being typed for a secured network.
#[derive(Debug)]
pub(crate) struct PasswordEntry {
    ssid: String,
    keyboard: KeyboardState,
    shown: bool,
    status: JoinStatus,
    join_requested: bool,
    in_flight_join: Option<WifiJoinToken>,
}

impl PasswordEntry {
    pub(super) fn new(ssid: String) -> Self {
        Self {
            ssid,
            keyboard: KeyboardState::new(MIN_PASSWORD_BYTES, MAX_PASSWORD_BYTES),
            shown: false,
            status: JoinStatus::Typing,
            join_requested: false,
            in_flight_join: None,
        }
    }

    pub(crate) const fn status(&self) -> JoinStatus {
        self.status
    }

    /// Presses a key. Submitting tries to join with the password; the keys
    /// wait while that runs. Returns whether anything changed.
    pub(crate) fn press(&mut self, key: Key) -> bool {
        if self.status == JoinStatus::Checking {
            return false;
        }

        match self.keyboard.press(key) {
            KeyResult::Unchanged => false,
            // editing clears a failure
            KeyResult::Changed => {
                self.status = JoinStatus::Typing;
                true
            }
            KeyResult::Submit => {
                self.status = JoinStatus::Checking;
                self.join_requested = true;
                true
            }
        }
    }

    pub(super) fn take_join_request(&mut self, token: WifiJoinToken) -> Option<WifiCredentials> {
        if !core::mem::take(&mut self.join_requested) {
            return None;
        }

        let credentials = self.credentials()?;
        self.in_flight_join = Some(token);

        Some(credentials)
    }

    /// Ends a join matching `token`. Stale results from earlier attempts are
    /// ignored.
    pub(super) fn finish_join(
        &mut self,
        token: WifiJoinToken,
        result: Result<(), WifiJoinFailure>,
    ) -> JoinOutcome {
        if self.status != JoinStatus::Checking || self.in_flight_join != Some(token) {
            return JoinOutcome::Ignored;
        }

        self.in_flight_join = None;

        match result {
            Ok(()) => self
                .credentials()
                .map_or(JoinOutcome::Ignored, JoinOutcome::Succeeded),
            Err(failure) => {
                self.status = JoinStatus::Failed(failure);
                JoinOutcome::Failed
            }
        }
    }

    pub(crate) fn ssid(&self) -> &str {
        &self.ssid
    }

    pub(crate) fn keyboard(&self) -> &KeyboardState {
        &self.keyboard
    }

    /// Whether the password is shown instead of masked.
    pub(crate) const fn shown(&self) -> bool {
        self.shown
    }

    pub(crate) fn toggle_shown(&mut self) {
        self.shown = !self.shown;
    }

    fn credentials(&self) -> Option<WifiCredentials> {
        WifiCredentials::new(self.ssid.clone(), self.keyboard.text())
    }
}
