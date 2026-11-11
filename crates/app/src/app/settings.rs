use inkpaper_ui::prelude::*;

use super::{InkPaperApp, Screen};
use crate::{
    resume::ResumeLimit,
    screens::settings::{SettingsEvent, SettingsView},
};

impl InkPaperApp {
    /// What the Settings screen's taps mean.
    pub(crate) fn on_settings_event(
        &mut self,
        settings: Entity<SettingsView>,
        event: &SettingsEvent,
        cx: &mut Context<'_, Self>,
    ) {
        match *event {
            SettingsEvent::Back => self.navigate_back(cx),
            SettingsEvent::OpenClock => self.open_screen(Screen::ClockSettings, cx),
            SettingsEvent::OpenText => self.open_screen(Screen::TextSettings, cx),
            SettingsEvent::ChooseDictionary(index) => {
                if self.choose_dictionary_option(index, cx) {
                    settings.update(cx, SettingsView::close_picker);
                }
            }
            SettingsEvent::ChooseResumeLimit(index) => {
                let Some(&limit) = ResumeLimit::ALL.get(index) else {
                    return;
                };

                self.reader.choose_resume_limit(limit);
                settings.update(cx, SettingsView::close_picker);
                cx.notify();
            }
        }
    }
}
