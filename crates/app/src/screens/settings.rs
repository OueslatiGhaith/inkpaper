use alloc::{format, string::String};
use inkpaper_ui::prelude::*;

use crate::{
    ClockStatus, InkPaperApp,
    app::{Back, Entry, ScreenInput, ScreenLifecycle, ScreenView},
    components::{
        header::{BackHeader, BackHeaderProps, BatteryIndicator},
        option_picker::{OptionPicker, OptionPickerProps},
        settings_row::{ListRow, ListRowProps, SettingsValueRow, SettingsValueRowProps},
    },
    resume::ResumeLimit,
};

#[component]
pub(crate) struct SettingsScreen<'a> {
    battery: Entity<BatteryIndicator>,
    clock: Option<ClockStatus>,
    /// shown like crosspoint's only when the card has a dictionary
    dictionary: Option<SettingsValueRow<'a>>,
    resume: SettingsValueRow<'a>,
    picker: Option<OptionPicker<'a>>,
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

                    {self.resume}
                </div>

                {#if let Some(picker) = self.picker}
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
    /// option `index` of [`ResumeLimit::ALL`]
    ChooseResumeLimit(usize),
}

/// The value a picker of the Settings screen chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsPicker {
    Dictionary,
    Resume,
}

/// the Settings screen. As its own entity it keeps its open picker. It reads the
/// app, and tells it what was tapped with [`SettingsEvent`]s.
pub(crate) struct SettingsView {
    app: Entity<InkPaperApp>,
    picker: Option<SettingsPicker>,
}

impl EventEmitter<SettingsEvent> for SettingsView {}

impl SettingsView {
    pub(crate) const fn new(app: Entity<InkPaperApp>) -> Self {
        Self { app, picker: None }
    }

    /// Returns whether a picker was open.
    pub(crate) fn close_picker(&mut self, cx: &mut Context<'_, Self>) -> bool {
        let closed = self.picker.take().is_some();

        if closed {
            cx.notify();
        }

        closed
    }

    fn open_picker(&mut self, picker: SettingsPicker, cx: &mut Context<'_, Self>) {
        if self.picker.replace(picker) != Some(picker) {
            cx.notify();
        }
    }

    fn activate_dictionary_row(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.open_picker(SettingsPicker::Dictionary, cx);
    }

    fn activate_resume_row(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.open_picker(SettingsPicker::Resume, cx);
    }

    fn activate_option(&mut self, event: &ActivateEvent, cx: &mut Context<'_, Self>) {
        let (Some(picker), Some(index)) = (self.picker, event.index()) else {
            return;
        };

        cx.emit(match picker {
            SettingsPicker::Dictionary => SettingsEvent::ChooseDictionary(index),
            SettingsPicker::Resume => SettingsEvent::ChooseResumeLimit(index),
        });
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

    fn activate_dismiss_picker(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.close_picker(cx);
    }
}

impl Render for SettingsView {
    fn render(&mut self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let open_picker = self.picker;

        let (battery, clock, dictionary, resume, picker) = self.app.read_with(cx, |app| {
            let names = app.dictionaries.names();
            // shown like crosspoint's only when the card has a dictionary
            let dictionary =
                (!names.is_empty()).then(|| String::from(app.dictionary_name().unwrap_or("None")));
            let resume = app.reader.resume_limit();

            let picker = open_picker.map(|picker| match picker {
                SettingsPicker::Dictionary => {
                    // none is the first option
                    let selected = app
                        .reader
                        .dictionary()
                        .map_or(0, |index| usize::from(index) + 1);

                    ("Dictionary", app.dictionaries.options(), selected)
                }
                SettingsPicker::Resume => (
                    "Reopen Book on Wake",
                    ResumeLimit::options(),
                    usize::from(resume.index()),
                ),
            });

            (
                app.battery_indicator,
                app.local_clock(cx),
                dictionary,
                resume,
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

        // like crosspoint's, a book read before sleep opens again at boot
        let resume = SettingsValueRow::from(SettingsValueRowProps {
            id: ("settings-resume", 0),
            label: "Reopen Book on Wake".into(),
            value: resume.label().into(),
            selected: false,
            on_activate: Some(cx.listener(Self::activate_resume_row)),
        });

        let picker = picker.map(|(title, options, selected)| {
            OptionPicker::from(OptionPickerProps {
                title,
                options,
                selected: Some(selected),
                on_option: cx.listener(Self::activate_option),
                on_dismiss: cx.listener(Self::activate_dismiss_picker),
            })
        });

        SettingsScreen::from(SettingsScreenProps {
            battery,
            clock,
            dictionary,
            resume,
            picker,
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
            app.settings.update(cx, SettingsView::close_picker);
        }
    }

    /// Back closes a picker first.
    fn back(&self, app: &mut InkPaperApp, cx: &mut Context<'_, InkPaperApp>) -> Back {
        if app.settings.update(cx, SettingsView::close_picker) {
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
