use std::{cell::Cell, rc::Rc};

use inkpaper_ui::prelude::*;

type TestRuntime = Runtime<4096, 8, 4096, 16, 64, 1024, 32, 256, 4>;

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
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let value = cx.global::<Theme>().value;

        self.observed.set(value);

        div().child("theme")
    }
}

#[test]
fn persistent_component_can_read_global() {
    let observed = Rc::new(Cell::new(0));

    let mut runtime = TestRuntime::default();

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
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div().child(ThemeBadge {
            observed: self.observed.as_ref(),
        })
    }
}

#[test]
fn render_once_component_can_read_global() {
    let observed = Rc::new(Cell::new(0));

    let mut runtime = TestRuntime::default();

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
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let activated = cx.listener(Self::activated);

        div().child(div().id("button").on_activate(activated).child("Activate"))
    }
}

#[test]
fn event_callback_can_read_global() {
    let observed = Rc::new(Cell::new(0));

    let mut runtime = TestRuntime::default();

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
    let runtime = TestRuntime::default();

    assert!(matches!(
        runtime.try_global::<Theme>(),
        Err(GlobalAccessError::NotFound),
    ));
}

#[test]
fn shared_global_borrows_can_nest() {
    let mut runtime = TestRuntime::default();

    runtime.set_global(Theme { value: 12 }).unwrap();

    let first = runtime.global::<Theme>();

    let second = runtime.global::<Theme>();

    assert_eq!(first.value, 12,);

    assert_eq!(second.value, 12,);
}

#[test]
fn mutable_global_borrow_conflicts_with_shared_borrow() {
    let mut runtime = TestRuntime::default();

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
fn mutating_global_requests_rebuild() {
    let mut runtime = TestRuntime::default();

    runtime.set_global(Theme { value: 10 }).unwrap();

    runtime.take_invalidation();

    {
        let mut theme = runtime.global_mut::<Theme>();

        theme.value = 99;
    }

    assert_eq!(runtime.invalidation(), Invalidation::Rebuild,);

    assert_eq!(runtime.global::<Theme>().value, 99,);
}

#[test]
fn global_capacity_is_independent_from_entity_capacity() {
    type RuntimeWithOneEntity = Runtime<1024, 1, 1024, 8, 32, 256, 8, 128, 2>;

    struct App;

    impl Render for App {
        fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div()
        }
    }

    let mut runtime = RuntimeWithOneEntity::default();

    runtime.set_global(Theme { value: 1 }).unwrap();

    runtime.set_global(Locale { value: 2 }).unwrap();

    /*
     * If globals consumed entity slots, these two globals would have exhausted
     * a runtime configured with only one entity slot.
     */
    runtime.create_root(|_| App).unwrap();

    runtime.rebuild().unwrap();

    assert_eq!(runtime.global_count(), 2,);

    assert_eq!(runtime.global::<Locale>().value, 2,);
}

#[test]
#[cfg(not(feature = "alloc"))]
fn global_slot_capacity_is_enforced() {
    type OneGlobalRuntime = Runtime<1024, 4, 1024, 8, 32, 256, 8, 128, 1>;

    let mut runtime = OneGlobalRuntime::default();

    runtime.set_global(Theme { value: 1 }).unwrap();

    assert_eq!(
        runtime.set_global(Locale { value: 2 },),
        Err(GlobalSetError::SlotsFull),
    );
}

#[test]
#[cfg(not(feature = "alloc"))]
fn global_byte_capacity_is_enforced() {
    #[derive(Debug)]
    struct LargeGlobal {
        _bytes: [u8; 64],
    }

    impl Global for LargeGlobal {}

    type TinyGlobalRuntime = Runtime<1024, 4, 1024, 8, 32, 256, 8, 16, 4>;

    let mut runtime = TinyGlobalRuntime::default();

    assert_eq!(
        runtime.set_global(LargeGlobal { _bytes: [0; 64] },),
        Err(GlobalSetError::StorageFull),
    );
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
        let mut runtime = TestRuntime::default();

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
