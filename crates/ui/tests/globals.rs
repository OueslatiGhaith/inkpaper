use std::{cell::Cell, rc::Rc};

use inkpaper_ui::prelude::*;

type TestRuntime = Runtime;

#[derive(Debug)]
struct Theme {
    value: u8,
}

impl Global for Theme {}

#[derive(Debug)]
struct Locale {
    value: u8,
}

impl Global for Locale {}

struct ReadsTheme {
    observed: Rc<Cell<u8>>,
}

impl Render for ReadsTheme {
    fn render(&mut self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let value = cx.global::<Theme>().value;

        self.observed.set(value);

        div().child("theme")
    }
}

#[test]
fn persistent_component_can_read_global() {
    let observed = Rc::new(Cell::new(0));

    let mut runtime = TestRuntime::new(pool());

    runtime.set_global(Theme { value: 42 }).unwrap();

    runtime
        .create_root({
            let observed = observed.clone();

            move |_| ReadsTheme { observed }
        })
        .unwrap();

    runtime.rebuild().unwrap();

    assert_eq!(observed.get(), 42,);
}

struct ThemeBadge<'a> {
    observed: &'a Cell<u8>,
}

impl RenderOnce for ThemeBadge<'_> {
    fn render(self, cx: &AppContext<'_>) -> impl IntoElement {
        let value = cx.global::<Theme>().value;

        self.observed.set(value);

        div().child("badge")
    }
}

struct RenderOnceApp {
    observed: Rc<Cell<u8>>,
}

impl Render for RenderOnceApp {
    fn render(&mut self, _: &mut Context<'_, Self>) -> impl IntoElement {
        div().child(ThemeBadge {
            observed: self.observed.as_ref(),
        })
    }
}

#[test]
fn render_once_component_can_read_global() {
    let observed = Rc::new(Cell::new(0));

    let mut runtime = TestRuntime::new(pool());

    runtime.set_global(Theme { value: 61 }).unwrap();

    runtime
        .create_root({
            let observed = observed.clone();

            move |_| RenderOnceApp { observed }
        })
        .unwrap();

    runtime.rebuild().unwrap();

    assert_eq!(observed.get(), 61,);
}

struct CallbackApp {
    observed: Rc<Cell<u8>>,
}

impl CallbackApp {
    fn activated(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.observed.set(cx.global::<Theme>().value);
    }
}

impl Render for CallbackApp {
    fn render(&mut self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let activated = cx.listener(Self::activated);

        div().child(div().id("button").on_activate(activated).child("Activate"))
    }
}

#[test]
fn event_callback_can_read_global() {
    let observed = Rc::new(Cell::new(0));

    let mut runtime = TestRuntime::new(pool());

    runtime.set_global(Theme { value: 73 }).unwrap();

    runtime
        .create_root({
            let observed = observed.clone();

            move |_| CallbackApp { observed }
        })
        .unwrap();

    runtime.rebuild().unwrap();

    assert!(runtime.focus_next());

    runtime.take_invalidation();

    assert!(runtime.activate_focused().unwrap());

    assert_eq!(observed.get(), 73,);
}

#[test]
fn missing_global_is_reported() {
    let runtime = TestRuntime::new(pool());

    assert!(matches!(
        runtime.try_global::<Theme>(),
        Err(GlobalAccessError::NotFound),
    ));
}

#[test]
fn shared_global_borrows_can_nest() {
    let mut runtime = TestRuntime::new(pool());

    runtime.set_global(Theme { value: 12 }).unwrap();

    let first = runtime.global::<Theme>();

    let second = runtime.global::<Theme>();

    assert_eq!(first.value, 12,);

    assert_eq!(second.value, 12,);
}

#[test]
fn mutable_global_borrow_conflicts_with_shared_borrow() {
    let mut runtime = TestRuntime::new(pool());

    runtime.set_global(Theme { value: 12 }).unwrap();

    let shared = runtime.global::<Theme>();

    assert!(matches!(
        runtime.try_global_mut::<Theme>(),
        Err(GlobalAccessError::BorrowConflict),
    ));

    drop(shared);

    assert!(runtime.try_global_mut::<Theme>().is_ok());
}

#[test]
fn mutating_global_requests_rebuild_of_its_readers() {
    let observed = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::new(pool());

    runtime.set_global(Theme { value: 10 }).unwrap();
    runtime.set_global(Locale { value: 1 }).unwrap();
    runtime
        .create_root({
            let observed = observed.clone();
            move |_| ReadsTheme { observed }
        })
        .unwrap();
    runtime.rebuild().unwrap();
    runtime.take_invalidation();

    // nothing rendered reads the locale
    runtime.global_mut::<Locale>().value = 2;

    assert_eq!(runtime.invalidation(), Invalidation::None);

    {
        let mut theme = runtime.global_mut::<Theme>();

        theme.value = 99;
    }

    assert_eq!(runtime.invalidation(), Invalidation::Rebuild,);

    runtime.take_invalidation();
    runtime.rebuild().unwrap();

    assert_eq!(observed.get(), 99);
}

struct DroppableGlobal {
    drops: Rc<Cell<u32>>,
}

impl Global for DroppableGlobal {}

impl Drop for DroppableGlobal {
    fn drop(&mut self) {
        self.drops.set(self.drops.get().saturating_add(1));
    }
}

#[test]
fn replacing_and_dropping_global_drops_each_value_once() {
    let drops = Rc::new(Cell::new(0));

    {
        let mut runtime = TestRuntime::new(pool());

        runtime
            .set_global(DroppableGlobal {
                drops: drops.clone(),
            })
            .unwrap();

        assert_eq!(drops.get(), 0,);

        runtime
            .set_global(DroppableGlobal {
                drops: drops.clone(),
            })
            .unwrap();

        assert_eq!(drops.get(), 1,);
    }

    assert_eq!(drops.get(), 2,);
}

/// a pool large enough for this file's runtimes, over leaked memory
fn pool() -> inkpaper_ui::Pool {
    inkpaper_ui::Pool::new(Box::leak(Box::<[u8]>::new_uninit_slice(256 * 1024)))
}
