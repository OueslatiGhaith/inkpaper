use alloc::string::String;

use inkpaper_ui::prelude::*;

use crate::{
    InkPaperApp,
    app::{Exit, ScreenInput, ScreenLifecycle, ScreenView},
    components::{
        header::{BackHeader, BackHeaderProps, BatteryIndicator},
        keyboard::{Keyboard, KeyboardProps},
        text_field::{TextField, TextFieldProps},
    },
    keyboard::Layer,
    wifi::{JoinStatus, PasswordEntry},
};

/// Typing the password of a secured network, like crosspoint's keyboard entry.
/// Join tries the network first and saves it as the one in use once it joins.
#[component]
pub(crate) struct WifiPasswordScreen<'a> {
    ssid: &'a str,
    battery: Entity<BatteryIndicator>,
    field: Entity<PasswordFieldView>,
    keyboard: Entity<PasswordKeyboardView>,
    on_back: Listener<ActivateEvent>,
}

impl RenderOnce for WifiPasswordScreen<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title={self.ssid}
                        battery={self.battery}
                        on_back={self.on_back}
                    />
                </div>

                <div class="absolute left-7 top-[98px] w-[424px] h-8 flex items-center">
                    <text class="text-base no-wrap">
                        "Password"
                    </text>
                </div>

                {self.field}

                {self.keyboard}
            </div>
        }
    }
}

/// what the keyboard shows, to render it again only when that changes
pub(crate) fn keyboard_look(entry: &PasswordEntry) -> (Layer, bool, bool) {
    let keyboard = entry.keyboard();

    (
        keyboard.layer(),
        keyboard.shift(),
        keyboard.can_submit() && entry.status() != JoinStatus::Checking,
    )
}

/// the password field and the hint under it. As its own entity, typing renders and
/// paints only these. Its state stays in the app, which renders it again through
/// [`InkPaperApp::refresh_password_field`].
pub(crate) struct PasswordFieldView {
    app: Entity<InkPaperApp>,
    text: String,
}

impl PasswordFieldView {
    pub(crate) const fn new(app: Entity<InkPaperApp>) -> Self {
        Self {
            app,
            text: String::new(),
        }
    }
}

impl Render for PasswordFieldView {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let text = &mut self.text;
        text.clear();

        // the listener targets the app, so its handler can render this field again
        let field = self
            .app
            .update(cx, |app, cx| {
                let entry = app.wifi.password_entry()?;
                let keyboard = entry.keyboard();
                text.push_str(keyboard.text());

                let hint = match entry.status() {
                    // WPA passwords are at least 8 characters
                    JoinStatus::Typing if !keyboard.can_submit() => "At least 8 characters",
                    JoinStatus::Typing => "",
                    JoinStatus::Checking => "Checking...",
                    JoinStatus::Failed(failure) => failure.message(),
                };

                Some((
                    entry.shown(),
                    hint,
                    cx.listener(InkPaperApp::activate_toggle_wifi_password),
                ))
            })
            .ok()
            .flatten();

        let Some((shown, hint, on_toggle)) = field else {
            return div().into_any_element();
        };

        rsx! {
            // the field 134 px down, and its hint 72 px under it
            <div class="absolute left-7 top-[134px] w-[424px] h-[104px]">
                <TextField
                    text={self.text.as_str()}
                    shown={shown}
                    on_toggle={on_toggle}
                />

                <div class="absolute left-0 top-[72px] w-full h-8 flex items-center">
                    <text class="text-base no-wrap">
                        {hint}
                    </text>
                </div>
            </div>
        }
        .into_any_element()
    }
}

/// the on-screen keyboard. As its own entity, a key that leaves it unchanged renders
/// and paints only the field. The app renders it again through
/// [`InkPaperApp::refresh_password_keyboard`].
pub(crate) struct PasswordKeyboardView {
    app: Entity<InkPaperApp>,
}

impl PasswordKeyboardView {
    pub(crate) const fn new(app: Entity<InkPaperApp>) -> Self {
        Self { app }
    }
}

impl Render for PasswordKeyboardView {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        // the listener targets the app, so its handler can render this keyboard again
        let keyboard = self
            .app
            .update(cx, |app, cx| {
                let entry = app.wifi.password_entry()?;

                Some((
                    entry.keyboard().rows(),
                    keyboard_look(entry),
                    cx.listener(InkPaperApp::activate_wifi_password_key),
                ))
            })
            .ok()
            .flatten();

        let Some((rows, (layer, shift, can_submit), on_key)) = keyboard else {
            return div().into_any_element();
        };

        rsx! {
            <div class="absolute left-0 top-[480px] w-[480px]">
                <Keyboard
                    rows={rows}
                    layer={layer}
                    shift={shift}
                    submit_label="Join"
                    can_submit={can_submit}
                    on_key={on_key}
                />
            </div>
        }
        .into_any_element()
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
            ssid: entry.ssid(),
            battery: app.battery_indicator,
            field: app.password_field,
            keyboard: app.password_keyboard,
            on_back: cx.listener(InkPaperApp::activate_back),
        })
        .into_any_element()
    }
}
