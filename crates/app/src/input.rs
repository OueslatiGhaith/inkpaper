use inkpaper_ui::{
    ElementId, Entity, EntityAccessError, ListenerInvokeError, Offset, Point, RuntimeApi,
};

use crate::InkPaperApp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppInputEvent {
    Previous,
    Next,
    Home,
    PointerDown(Point),
    PointerDrag {
        origin: Point,
        previous: Point,
        position: Point,
    },
    /// The pointer stayed down without moving; see [`crate::TouchGesture`].
    PointerLongPress(Point),
    PointerUp(Point),
    PointerCancel,
    ScrollWheel {
        position: Point,
        delta: Offset,
    },
}

/// A touch held still on an element. Like `ActivateEvent`, it carries the
/// element's id, so one listener can serve many elements. Elements take it
/// with `on:LongPressEvent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LongPressEvent {
    id: ElementId,
    position: Point,
}

impl LongPressEvent {
    /// Where the touch was held.
    pub(crate) const fn position(self) -> Point {
        self.position
    }

    pub(crate) const fn index(self) -> Option<usize> {
        match self.id {
            ElementId::Value(value) | ElementId::NamedValue(_, value) => Some(value as usize),
            ElementId::Name(_) => None,
        }
    }
}

#[derive(Debug)]
pub enum AppInputError {
    Entity(EntityAccessError),
    Listener(ListenerInvokeError),
}

impl From<EntityAccessError> for AppInputError {
    fn from(error: EntityAccessError) -> Self {
        Self::Entity(error)
    }
}

impl From<ListenerInvokeError> for AppInputError {
    fn from(error: ListenerInvokeError) -> Self {
        Self::Listener(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointerAction {
    Activate,
    Capture,
    Scroll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LongPressAction {
    /// Nothing takes it; the touch can still end as a tap.
    Ignore,
    /// Offer it to the element under the pointer.
    Dispatch,
    /// The screen took it.
    Capture,
}

pub fn dispatch_input(
    runtime: &mut impl RuntimeApi,
    app: Entity<InkPaperApp>,
    event: AppInputEvent,
) -> Result<(), AppInputError> {
    match event {
        AppInputEvent::Previous => {
            runtime.update(app, |app, cx| app.handle_previous_input(cx))?;
        }

        AppInputEvent::Next => {
            runtime.update(app, |app, cx| app.handle_next_input(cx))?;
        }

        AppInputEvent::Home => {
            runtime.update(app, |app, cx| app.handle_home_input(cx))?;
        }

        AppInputEvent::PointerDown(position) => {
            let action = runtime.update(app, |app, cx| app.handle_pointer_down(position, cx))?;

            match action {
                PointerAction::Activate => {
                    runtime.begin_activation_at(position);
                }
                PointerAction::Capture | PointerAction::Scroll => {
                    runtime.cancel_activation();
                }
            }
        }

        AppInputEvent::PointerDrag {
            origin,
            previous,
            position,
        } => {
            // A pointer that has crossed the host's drag threshold cannot still
            // complete a tap activation.
            runtime.cancel_activation();

            let action =
                runtime.update(app, |app, cx| app.handle_pointer_drag(origin, position, cx))?;

            if action == PointerAction::Scroll {
                runtime.scroll_at(origin, previous - position);
            }
        }

        AppInputEvent::PointerLongPress(position) => {
            let action =
                runtime.update(app, |app, cx| app.handle_pointer_long_press(position, cx))?;

            let captured = match action {
                LongPressAction::Ignore => false,
                LongPressAction::Dispatch => {
                    runtime.dispatch_at_with(position, |id| LongPressEvent { id, position })?
                }
                LongPressAction::Capture => true,
            };

            // the rest of a touch that long pressed something isn't also a tap
            if captured {
                runtime.cancel_activation();
                runtime.update(app, |app, _| app.capture_pointer())?;
            }
        }

        AppInputEvent::PointerUp(position) => {
            let action = runtime.update(app, |app, cx| app.handle_pointer_up(position, cx))?;

            match action {
                PointerAction::Activate => {
                    runtime.complete_activation_at(position)?;
                }
                PointerAction::Capture | PointerAction::Scroll => {
                    runtime.cancel_activation();
                }
            }
        }

        AppInputEvent::PointerCancel => {
            runtime.update(app, |app, _| app.cancel_pointer_input())?;
            runtime.cancel_activation();
        }

        AppInputEvent::ScrollWheel { position, delta } => {
            let allowed = runtime.update(app, |app, _| app.allows_wheel_scroll())?;

            if allowed {
                runtime.scroll_at(position, delta);
            }
        }
    }

    Ok(())
}
