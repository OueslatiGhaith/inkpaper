use inkpaper_ui::{ElementId, Entity, EntityAccessError, ListenerInvokeError, Point, RuntimeApi};

use crate::{
    InkPaperApp,
    components::{
        control_center::ControlCenterHandleEvent,
        slider::{self, SliderPhase, SliderScrubEvent},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppInputEvent {
    Previous,
    Next,
    Home,
    PointerDown(Point),
    PointerDrag {
        origin: Point,
        position: Point,
    },
    /// The pointer stayed down without moving; see [`crate::TouchGesture`].
    PointerLongPress(Point),
    PointerUp(Point),
    PointerCancel,
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
    // a press or a touch hides the toast that waits for one, and still does
    // what it does. The rest of a touch has nothing left to hide
    if matches!(
        event,
        AppInputEvent::Previous
            | AppInputEvent::Next
            | AppInputEvent::Home
            | AppInputEvent::PointerDown(_)
    ) {
        runtime.update(app, |app, cx| app.hide_toast_on_input(cx))?;
    }

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
            let blocked = runtime.update(app, |app, _| app.blocks_pointer_input())?;
            if blocked {
                runtime.cancel_activation();
                return Ok(());
            }

            if let Some(hit) = runtime.hit_at::<SliderScrubEvent>(position)
                && slider::is_track_id(hit.id())
            {
                runtime.cancel_activation();
                runtime.update(app, |app, _| {
                    app.begin_slider_scrub(hit.target(), hit.bounds());
                })?;
                let value = slider::value_in_bounds(position.x, hit.bounds());
                runtime.dispatch_to(
                    hit.target(),
                    &SliderScrubEvent::new(value, SliderPhase::Start),
                )?;

                return Ok(());
            }

            let on_handle = runtime
                .hit_at::<ControlCenterHandleEvent>(position)
                .is_some();
            let action = runtime.update(app, |app, cx| {
                app.handle_pointer_down(position, on_handle, cx)
            })?;

            match action {
                PointerAction::Activate => {
                    runtime.begin_activation_at(position);
                }
                PointerAction::Capture => {
                    runtime.cancel_activation();
                }
            }
        }

        AppInputEvent::PointerDrag { origin, position } => {
            // A pointer that has crossed the host's drag threshold cannot still
            // complete a tap activation.
            runtime.cancel_activation();

            if let Some((target, bounds)) = runtime.update(app, |app, _| app.active_slider())? {
                let value = slider::value_in_bounds(position.x, bounds);
                runtime.dispatch_to(target, &SliderScrubEvent::new(value, SliderPhase::Move))?;

                return Ok(());
            }

            runtime.update(app, |app, cx| app.handle_pointer_drag(origin, position, cx))?;
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
            if let Some((target, bounds)) =
                runtime.update(app, |app, _| app.take_active_slider())?
            {
                let value = slider::value_in_bounds(position.x, bounds);
                runtime.dispatch_to(target, &SliderScrubEvent::new(value, SliderPhase::End))?;
                runtime.cancel_activation();

                return Ok(());
            }

            let action = runtime.update(app, |app, cx| app.handle_pointer_up(position, cx))?;

            match action {
                PointerAction::Activate => {
                    runtime.complete_activation_at(position)?;
                }
                PointerAction::Capture => {
                    runtime.cancel_activation();
                }
            }
        }

        AppInputEvent::PointerCancel => {
            runtime.update(app, |app, _| app.cancel_pointer_input())?;
            runtime.cancel_activation();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use inkpaper_ui::{ActivateEvent, Pool, Runtime, Size, px};

    use crate::{AppResources, InkPaperApp};

    use super::*;

    fn tap(
        runtime: &mut Runtime<AppResources<'_, 32, 4096>>,
        app: Entity<InkPaperApp>,
        point: Point,
    ) {
        dispatch_input(runtime, app, AppInputEvent::PointerDown(point)).unwrap();
        dispatch_input(runtime, app, AppInputEvent::PointerUp(point)).unwrap();
    }

    #[test]
    fn layout_driven_sliders_and_control_center_handle_input_without_leaking() {
        let viewport = Size::new(px(480), px(800));
        let mut runtime = Runtime::<AppResources<'_, 32, 4096>>::new(Pool::global());
        InkPaperApp::register_resources(&mut runtime).unwrap();
        let app = runtime.create_root(InkPaperApp::new).unwrap();

        // Open Settings -> Clock Settings so the timezone slider is rendered.
        runtime
            .update(app, |app, cx| {
                app.show_settings(&ActivateEvent::new(ElementId::Name("settings")), cx);
                app.open_clock_settings(cx);
            })
            .unwrap();
        runtime.prepare_frame(viewport).unwrap();

        // Scrub the timezone slider using its actual laid-out track bounds.
        let tz_hit = runtime
            .hit_at::<SliderScrubEvent>(Point::new(px(240), px(240)))
            .expect("timezone slider track should be hit-testable");
        assert!(slider::is_track_id(tz_hit.id()));

        let right_edge = Point::new(
            tz_hit.bounds().x() + tz_hit.bounds().width(),
            tz_hit.bounds().y() + px(10),
        );
        dispatch_input(
            &mut runtime,
            app,
            AppInputEvent::PointerDown(Point::new(
                tz_hit.bounds().x() + px(20),
                tz_hit.bounds().y() + px(10),
            )),
        )
        .unwrap();
        dispatch_input(
            &mut runtime,
            app,
            AppInputEvent::PointerDrag {
                origin: Point::new(tz_hit.bounds().x() + px(20), tz_hit.bounds().y() + px(10)),
                position: right_edge,
            },
        )
        .unwrap();
        dispatch_input(&mut runtime, app, AppInputEvent::PointerUp(right_edge)).unwrap();

        let tz_after_scrub = runtime
            .update(app, |app, _| {
                assert!(app.clock.take_save_request().is_some());
                app.clock.utc_offset()
            })
            .unwrap();
        assert_eq!(tz_after_scrub.slider_value(), 100);

        // Open Control Center with a top-edge swipe down.
        let origin = Point::new(px(200), px(4));
        let dragged = Point::new(px(200), px(40));
        dispatch_input(&mut runtime, app, AppInputEvent::PointerDown(origin)).unwrap();
        dispatch_input(
            &mut runtime,
            app,
            AppInputEvent::PointerDrag {
                origin,
                position: dragged,
            },
        )
        .unwrap();
        dispatch_input(
            &mut runtime,
            app,
            AppInputEvent::PointerUp(Point::new(px(200), px(500))),
        )
        .unwrap();
        runtime.prepare_frame(viewport).unwrap();
        assert!(runtime.update(app, |app, _| app.is_control_center_open()).unwrap());

        // Touching empty sheet space directly above ClockSettings's timezone slider
        // must neither scrub the timezone underneath nor dismiss the control center.
        let over_tz = Point::new(tz_hit.bounds().x() + px(10), tz_hit.bounds().y() + px(10));
        tap(&mut runtime, app, over_tz);
        assert_eq!(
            runtime.update(app, |app, _| app.clock.utc_offset()).unwrap(),
            tz_after_scrub
        );
        assert!(runtime.update(app, |app, _| app.is_control_center_open()).unwrap());

        // Tapping the lightbulb toggles frontlight via normal ActivateEvent.
        let toggle_hit = runtime
            .hit_at::<ActivateEvent>(Point::new(px(420), px(133)))
            .expect("lightbulb toggle should be hit-testable");
        assert_eq!(
            toggle_hit.id(),
            ElementId::Name("control-center-frontlight-toggle")
        );
        let before_on = runtime
            .update(app, |app, cx| app.frontlight_setting(cx).is_on())
            .unwrap();
        tap(
            &mut runtime,
            app,
            Point::new(
                toggle_hit.bounds().x() + px(10),
                toggle_hit.bounds().y() + px(10),
            ),
        );
        assert_eq!(
            runtime
                .update(app, |app, cx| app.frontlight_setting(cx).is_on())
                .unwrap(),
            !before_on
        );

        // Scrubbing the warmth slider adjusts warmth via layout bounds.
        let warmth_hit = runtime
            .hit_at::<SliderScrubEvent>(Point::new(px(240), px(293)))
            .expect("warmth slider track should be hit-testable");
        assert!(slider::is_track_id(warmth_hit.id()));
        let warmth_end = Point::new(
            warmth_hit.bounds().x() + warmth_hit.bounds().width() - px(1),
            warmth_hit.bounds().y() + px(10),
        );
        dispatch_input(&mut runtime, app, AppInputEvent::PointerDown(warmth_end)).unwrap();
        dispatch_input(&mut runtime, app, AppInputEvent::PointerUp(warmth_end)).unwrap();
        assert_eq!(
            runtime
                .update(app, |app, cx| app.frontlight_setting(cx).warmth())
                .unwrap(),
            100
        );

        // Swiping up on the laid-out drawer handle closes the control center.
        let handle_hit = runtime
            .hit_at::<ControlCenterHandleEvent>(Point::new(px(240), px(370)))
            .expect("drawer handle should be hit-testable");
        let handle_down = Point::new(
            handle_hit.bounds().x() + px(100),
            handle_hit.bounds().y() + px(16),
        );
        let handle_up = Point::new(handle_down.x, handle_down.y - px(24));
        dispatch_input(&mut runtime, app, AppInputEvent::PointerDown(handle_down)).unwrap();
        dispatch_input(
            &mut runtime,
            app,
            AppInputEvent::PointerDrag {
                origin: handle_down,
                position: handle_up,
            },
        )
        .unwrap();
        dispatch_input(&mut runtime, app, AppInputEvent::PointerUp(handle_up)).unwrap();
        assert!(!runtime.update(app, |app, _| app.is_control_center_open()).unwrap());

        // Reopen and verify tapping the backdrop also closes it.
        dispatch_input(&mut runtime, app, AppInputEvent::PointerDown(origin)).unwrap();
        dispatch_input(
            &mut runtime,
            app,
            AppInputEvent::PointerDrag {
                origin,
                position: dragged,
            },
        )
        .unwrap();
        dispatch_input(&mut runtime, app, AppInputEvent::PointerUp(dragged)).unwrap();
        runtime.prepare_frame(viewport).unwrap();
        assert!(runtime.update(app, |app, _| app.is_control_center_open()).unwrap());

        tap(&mut runtime, app, Point::new(px(200), px(500)));
        assert!(!runtime.update(app, |app, _| app.is_control_center_open()).unwrap());
    }
}
