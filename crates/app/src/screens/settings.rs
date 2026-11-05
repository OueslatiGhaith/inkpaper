use alloc::{format, string::String};
use core::mem;
use inkpaper_ui::prelude::*;

use crate::{
    ClockStatus, InkPaperApp,
    app::{Back, Entry, ScreenInput, ScreenLifecycle, ScreenView},
    components::{
        header::{BackHeader, BackHeaderProps, BatteryIndicator},
        option_picker::{OptionPicker, OptionPickerProps},
        settings_row::{ListRow, ListRowProps, SettingsValueRow, SettingsValueRowProps},
    },
};

#[component]
pub(crate) struct SettingsScreen<'a> {
    battery: Entity<BatteryIndicator>,
    clock: Option<ClockStatus>,
    /// shown like crosspoint's only when the card has a dictionary
    dictionary: Option<SettingsValueRow<'a>>,
    dictionary_picker: Option<OptionPicker<'a>>,
    on_back: Listener<ActivateEvent>,
    on_clock: Listener<ActivateEvent>,
    on_text: Listener<ActivateEvent>,
}

impl RenderOnce for SettingsScreen<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let clock = match self.clock {
            None => String::from("Date/time unavailable"),
            Some(clock) => format!(
                "{:02}/{:02}/{:04}  {:02}:{:02}",
                clock.day(),
                clock.month(),
                clock.year(),
                clock.hour(),
                clock.minute(),
            ),
        };

        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title="Settings"
                        battery={self.battery}
                        on_back={self.on_back}
                    />

                    <div class="absolute right-3 top-[32px]">
                        <text class="text-base">
                            {clock}
                        </text>
                    </div>
                </div>

                <div class="absolute left-0 top-[98px] w-[480px] flex flex-col">
                    <ListRow
                        id={("settings-text", 0)}
                        label="Text Settings"
                        depth={0}
                        selected={false}
                        chevron={true}
                        on_activate={Some(self.on_text)}
                    />

                    <ListRow
                        id={("settings-clock", 0)}
                        label="Clock"
                        depth={0}
                        selected={false}
                        chevron={true}
                        on_activate={Some(self.on_clock)}
                    />

                    {#if let Some(row) = self.dictionary}
                        {row}
                    {/if}
                </div>

                {#if let Some(picker) = self.dictionary_picker}
                    {picker}
                {/if}
            </div>
        }
    }
}

/// What the Settings screen asks of the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsEvent {
    Back,
    OpenClock,
    OpenText,
    /// option `0` of the dictionary picker turns lookups off; the others choose a
    /// dictionary
    ChooseDictionary(usize),
}

/// the Settings screen. As its own entity it keeps its dictionary picker. It reads
/// the app, and tells it what was tapped with [`SettingsEvent`]s.
pub(crate) struct SettingsView {
    app: Entity<InkPaperApp>,
    dictionary_picker_open: bool,
}

impl EventEmitter<SettingsEvent> for SettingsView {}

impl SettingsView {
    pub(crate) const fn new(app: Entity<InkPaperApp>) -> Self {
        Self {
            app,
            dictionary_picker_open: false,
        }
    }

    /// Returns whether the dictionary picker was open.
    pub(crate) fn close_dictionary_picker(&mut self, cx: &mut Context<'_, Self>) -> bool {
        let closed = mem::take(&mut self.dictionary_picker_open);

        if closed {
            cx.notify();
        }

        closed
    }

    fn activate_dictionary_row(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        if !mem::replace(&mut self.dictionary_picker_open, true) {
            cx.notify();
        }
    }

    fn activate_dictionary_option(&mut self, event: &ActivateEvent, cx: &mut Context<'_, Self>) {
        if let Some(index) = event.index() {
            cx.emit(SettingsEvent::ChooseDictionary(index));
        }
    }

    fn activate_back(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        cx.emit(SettingsEvent::Back);
    }

    fn activate_clock(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        cx.emit(SettingsEvent::OpenClock);
    }

    fn activate_text(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        cx.emit(SettingsEvent::OpenText);
    }

    fn activate_dismiss_dictionary_picker(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        self.close_dictionary_picker(cx);
    }
}

impl Render for SettingsView {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let picker_open = self.dictionary_picker_open;

        let (battery, clock, dictionary, picker) = self.app.read(cx, |app| {
            let names = app.dictionaries.names();
            // shown like crosspoint's only when the card has a dictionary
            let dictionary =
                (!names.is_empty()).then(|| String::from(app.dictionary_name().unwrap_or("None")));
            // none is the first option
            let picker = picker_open.then(|| {
                let selected = app
                    .reader
                    .dictionary()
                    .map_or(0, |index| usize::from(index) + 1);

                (app.dictionaries.options(), selected)
            });

            (
                app.battery_indicator,
                app.local_clock(cx),
                dictionary,
                picker,
            )
        });

        let dictionary = dictionary.map(|name| {
            SettingsValueRow::from(SettingsValueRowProps {
                id: ("settings-dictionary", 0),
                label: "Dictionary".into(),
                value: name.into(),
                selected: false,
                on_activate: Some(cx.listener(Self::activate_dictionary_row)),
            })
        });

        let dictionary_picker = picker.map(|(options, selected)| {
            OptionPicker::from(OptionPickerProps {
                title: "Dictionary",
                options,
                selected: Some(selected),
                on_option: cx.listener(Self::activate_dictionary_option),
                on_dismiss: cx.listener(Self::activate_dismiss_dictionary_picker),
            })
        });

        SettingsScreen::from(SettingsScreenProps {
            battery,
            clock,
            dictionary,
            dictionary_picker,
            on_back: cx.listener(Self::activate_back),
            on_clock: cx.listener(Self::activate_clock),
            on_text: cx.listener(Self::activate_text),
        })
    }
}

pub(crate) struct SettingsRoute;

impl ScreenLifecycle for SettingsRoute {
    fn enter(&self, app: &mut InkPaperApp, entry: Entry, cx: &mut Context<'_, InkPaperApp>) {
        if entry == Entry::Opened {
            app.settings
                .update(cx, SettingsView::close_dictionary_picker);
        }
    }

    /// Back closes the dictionary picker first.
    fn back(&self, app: &mut InkPaperApp, cx: &mut Context<'_, InkPaperApp>) -> Back {
        if app
            .settings
            .update(cx, SettingsView::close_dictionary_picker)
        {
            return Back::Handled;
        }

        Back::Leave
    }
}

impl ScreenInput for SettingsRoute {}

impl ScreenView for SettingsRoute {
    fn render<'a>(&self, app: &'a InkPaperApp, _: &mut Context<'_, InkPaperApp>) -> AnyElement<'a> {
        app.settings.into_any_element()
    }
}
