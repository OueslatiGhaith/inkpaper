use inkpaper_ui::{Entity, Offset, Point, RuntimeApi, px};

use crate::{InkPaperApp, Route, clock::TimeOfDay};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Button {
    Previous,
    Next,
    Activate,
    Power,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ButtonEdge {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TouchEvent {
    Down(TouchPosition),
    Up(TouchPosition),
    HomeTap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ScrollEvent {
    position: TouchPosition,
    delta_x: i32,
    delta_y: i32,
}

impl ScrollEvent {
    pub const fn new(position: TouchPosition, delta_x: i32, delta_y: i32) -> Self {
        Self {
            position,
            delta_x,
            delta_y,
        }
    }

    pub const fn position(self) -> TouchPosition {
        self.position
    }

    pub const fn delta_x(self) -> i32 {
        self.delta_x
    }

    pub const fn delta_y(self) -> i32 {
        self.delta_y
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum InputEvent {
    Button(ButtonEvent),
    Touch(TouchEvent),
    Scroll(ScrollEvent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ClockState {
    Unavailable,
    Utc(TimeOfDay),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AppEvent {
    Input(InputEvent),
    BatteryPercent(u8),
    Clock(ClockState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum PlatformAction {
    None,
    Suspend,
}

impl InkPaperApp {
    pub fn handle_event(
        runtime: &mut impl RuntimeApi,
        app: Entity<Self>,
        event: AppEvent,
    ) -> PlatformAction {
        match event {
            AppEvent::Input(input) => {
                match input {
                    InputEvent::Button(event) => match (event.button(), event.edge()) {
                        (Button::Previous, ButtonEdge::Pressed) => {
                            runtime.focus_previous();
                        }
                        (Button::Next, ButtonEdge::Pressed) => {
                            runtime.focus_next();
                        }
                        (Button::Activate, ButtonEdge::Pressed) => {
                            runtime.begin_focused_activation();
                        }
                        (Button::Activate, ButtonEdge::Released) => {
                            runtime
                                .complete_focused_activation()
                                .expect("focused activation callback must remain valid");
                        }
                        (Button::Power, ButtonEdge::Pressed) => return PlatformAction::Suspend,
                        (Button::Previous | Button::Next | Button::Power, ButtonEdge::Released) => {
                        }
                    },
                    InputEvent::Touch(TouchEvent::Down(position)) => {
                        runtime.begin_activation_at(to_ui_point(position));
                    }
                    InputEvent::Touch(TouchEvent::Up(position)) => {
                        runtime
                            .complete_activation_at(to_ui_point(position))
                            .expect("pointer activation callback must remain valid");
                    }
                    InputEvent::Touch(TouchEvent::HomeTap) => {
                        runtime
                            .update(app, |app, cx| {
                                app.navigate(Route::Home, cx);
                            })
                            .expect("InkPaper application entity must remain alive");
                    }
                    InputEvent::Scroll(event) => {
                        runtime.scroll_at(
                            to_ui_point(event.position()),
                            Offset::new(px(event.delta_x()), px(event.delta_y())),
                        );
                    }
                }

                PlatformAction::None
            }
            AppEvent::BatteryPercent(percent) => {
                let percent = percent.min(100);

                runtime
                    .update(app, |app, cx| {
                        if app.model().battery().value() == percent {
                            return;
                        }

                        app.model_mut().set_battery_percent(percent);
                        cx.notify();
                    })
                    .expect("InkPaper application entity must remain alive");

                PlatformAction::None
            }
            AppEvent::Clock(state) => {
                runtime
                    .update(app, |app, cx| {
                        let was_available = app.model().clock().is_available();

                        match state {
                            ClockState::Unavailable => {
                                let changed = app.model_mut().clock_mut().invalidate();

                                // once a displayed clock becomes untrustworthy, repaint
                                // once rather than leaving a stale time on screen.
                                if changed && was_available {
                                    cx.notify();
                                }
                            }
                            ClockState::Utc(time) => {
                                app.model_mut().clock_mut().set_utc(time);

                                // the first valid reading replaces "--:--" immediately.
                                // subsequent minute ticks update only the in-memory model.
                                // A later normal e-ink refresh will reveal the latest time.
                                if !was_available {
                                    cx.notify();
                                }
                            }
                        }
                    })
                    .expect("InkPaper application entity must remain alive");

                PlatformAction::None
            }
        }
    }
}

fn to_ui_point(position: TouchPosition) -> Point {
    Point::new(px(position.x() as i32), px(position.y() as i32))
}
