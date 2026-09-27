use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus, InkPaperApp,
    app::{Exit, ScreenInput, ScreenLifecycle, ScreenView},
    components::{
        header::{BackHeader, BackHeaderProps},
        keyboard::{Keyboard, KeyboardProps},
        text_field::{TextField, TextFieldProps},
    },
    wifi::PasswordEntry,
};

/// Typing the password of a secured network, like crosspoint's keyboard entry.
/// Join saves the network as the one in use.
#[component]
pub(crate) struct WifiPasswordScreen<'a> {
    entry: &'a PasswordEntry,
    battery: Option<BatteryStatus>,
    on_back: Listener<ActivateEvent>,
    on_toggle: Listener<ActivateEvent>,
    on_key: Listener<ActivateEvent>,
}

impl RenderOnce for WifiPasswordScreen<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let keyboard = self.entry.keyboard();

        // WPA passwords are at least 8 characters
        let hint = if keyboard.can_submit() {
            ""
        } else {
            "At least 8 characters"
        };

        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title={self.entry.ssid()}
                        battery={self.battery}
                        on_back={self.on_back}
                    />
                </div>

                <div class="absolute left-7 top-[98px] w-[424px] h-8 flex items-center">
                    <text class="text-base no-wrap">
                        "Password"
                    </text>
                </div>

                <div class="absolute left-7 top-[134px]">
                    <TextField
                        text={keyboard.text()}
                        shown={self.entry.shown()}
                        on_toggle={self.on_toggle}
                    />
                </div>

                <div class="absolute left-7 top-[206px] w-[424px] h-8 flex items-center">
                    <text class="text-base no-wrap">
                        {hint}
                    </text>
                </div>

                <div class="absolute left-0 top-[480px] w-[480px]">
                    <Keyboard
                        rows={keyboard.rows()}
                        layer={keyboard.layer()}
                        shift={keyboard.shift()}
                        submit_label="Join"
                        can_submit={keyboard.can_submit()}
                        on_key={self.on_key}
                    />
                </div>
            </div>
        }
    }
}

pub(crate) struct WifiPasswordRoute;

impl ScreenLifecycle for WifiPasswordRoute {
    fn exit(&self, app: &mut InkPaperApp, exit: Exit) {
        // a password typed but not joined is dropped
        if exit == Exit::Closed {
            app.wifi.end_password_entry();
        }
    }
}

impl ScreenInput for WifiPasswordRoute {}

impl ScreenView for WifiPasswordRoute {
    fn render<'a>(
        &self,
        app: &'a InkPaperApp,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> AnyElement<'a> {
        let Some(entry) = app.wifi.password_entry() else {
            return div().into_any_element();
        };

        WifiPasswordScreen::from(WifiPasswordScreenProps {
            entry,
            battery: app.system_status.battery(),
            on_back: cx.listener(InkPaperApp::activate_back),
            on_toggle: cx.listener(InkPaperApp::activate_toggle_wifi_password),
            on_key: cx.listener(InkPaperApp::activate_wifi_password_key),
        })
        .into_any_element()
    }
}
