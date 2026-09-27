use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use inkpaper_app::AppInputEvent;

const INPUT_QUEUE_CAPACITY: usize = 16;

pub static INPUT_EVENTS: Channel<CriticalSectionRawMutex, InputEvent, INPUT_QUEUE_CAPACITY> =
    Channel::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonEdge {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonEvent {
    button: Button,
    edge: ButtonEdge,
}

impl ButtonEvent {
    pub const fn new(button: Button, edge: ButtonEdge) -> Self {
        Self { button, edge }
    }

    pub const fn button(self) -> Button {
        self.button
    }

    pub const fn edge(self) -> ButtonEdge {
        self.edge
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerButtonEvent {
    ShortPress,
    LongPress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchEvent {
    /// touch screen input, already turned into taps, drags and long presses.
    Pointer(AppInputEvent),
    /// short press of the capacitive Home pad.
    HomeTap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    Button(ButtonEvent),
    Power(PowerButtonEvent),
    Touch(TouchEvent),
}
