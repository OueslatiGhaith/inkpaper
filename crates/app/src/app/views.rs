use inkpaper_ui::prelude::*;

use crate::screens::{
    clock_settings::TimezoneView,
    settings::SettingsView,
    text_settings::TextSettingsView,
    wifi_networks::WifiNetworksView,
    wifi_password::{PasswordFieldView, PasswordKeyboardView},
};

use super::InkPaperApp;

/// View entities owned together by the WiFi password screen.
#[derive(Clone, Copy)]
pub(crate) struct WifiPasswordViews {
    pub(crate) field: Entity<PasswordFieldView>,
    pub(crate) keyboard: Entity<PasswordKeyboardView>,
}

/// View entities owned by open screens and released when their screen closes.
#[derive(Default)]
pub(crate) struct ScreenViews {
    pub(crate) wifi_password: Option<WifiPasswordViews>,
    pub(crate) timezone: Option<Entity<TimezoneView>>,
    pub(crate) settings: Option<Entity<SettingsView>>,
    pub(crate) text_settings: Option<Entity<TextSettingsView>>,
    pub(crate) wifi_networks: Option<Entity<WifiNetworksView>>,
}

impl ScreenViews {
    /// Creates a screen's view entity into `slot`, releasing any previous view
    /// still held there.
    pub(crate) fn open<V: 'static>(
        slot: &mut Option<Entity<V>>,
        cx: &mut Context<'_, InkPaperApp>,
        view: impl FnOnce(Entity<InkPaperApp>) -> V,
    ) -> Entity<V> {
        Self::close(slot, cx);

        let app = cx.entity();
        let entity = cx.new(|_| view(app));
        *slot = Some(entity);

        entity
    }

    /// Releases the view entity in `slot`, if open.
    pub(crate) fn close<V: 'static>(
        slot: &mut Option<Entity<V>>,
        cx: &mut Context<'_, InkPaperApp>,
    ) {
        if let Some(view) = slot.take() {
            cx.release(view);
        }
    }

    /// Creates the WiFi password screen's field and keyboard views together.
    pub(crate) fn open_wifi_password(
        &mut self,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> WifiPasswordViews {
        self.close_wifi_password(cx);

        let app = cx.entity();
        let views = WifiPasswordViews {
            field: cx.new(|_| PasswordFieldView::new(app)),
            keyboard: cx.new(|_| PasswordKeyboardView::new(app)),
        };
        self.wifi_password = Some(views);

        views
    }

    /// Releases both WiFi password screen views together.
    pub(crate) fn close_wifi_password(&mut self, cx: &mut Context<'_, InkPaperApp>) {
        if let Some(views) = self.wifi_password.take() {
            cx.release(views.field);
            cx.release(views.keyboard);
        }
    }
}
