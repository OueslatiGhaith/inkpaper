use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};

const INPUT_QUEUE_CAPACITY: usize = 16;

pub static INPUT_EVENTS: Channel<CriticalSectionRawMutex, InputEvent, INPUT_QUEUE_CAPACITY> =
    Channel::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Power,
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
pub struct TouchPosition {
    x: u16,
    y: u16,
}

impl TouchPosition {
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }

    pub const fn x(self) -> u16 {
        self.x
    }

    pub const fn y(self) -> u16 {
        self.y
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchEvent {
    Down(TouchPosition),
    Up(TouchPosition),
    /// short press of the capacitive Home pad.
    HomeTap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    Button(ButtonEvent),
    Touch(TouchEvent),
}
