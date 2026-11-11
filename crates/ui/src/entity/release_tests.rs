use core::cell::Cell;
use std::rc::Rc;

use crate::pool::testing::pool;
use crate::*;

/// counts its drops
struct Counted(Rc<Cell<u32>>);

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

struct Panel {
    _drops: Counted,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Context<'_, Self>) -> impl IntoElement {
        div().w(px(10)).h(px(10))
    }
}

struct Root {
    panel: Option<Entity<Panel>>,
}

impl Root {
    fn close_panel(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(panel) = self.panel.take() {
            cx.release(panel);
            cx.notify();
        }
    }
}

impl Render for Root {
    fn render(&mut self, _: &mut Context<'_, Self>) -> impl IntoElement {
        div().children(self.panel)
    }
}

struct Setup {
    runtime: Runtime,
    root: Entity<Root>,
    panel: Entity<Panel>,
    drops: Rc<Cell<u32>>,
}

fn setup() -> Setup {
    let drops = Rc::new(Cell::new(0));
    let mut runtime = Runtime::new(pool());

    let root = runtime
        .create_root({
            let drops = drops.clone();

            move |cx| Root {
                panel: Some(cx.new(|_| Panel {
                    _drops: Counted(drops),
                })),
            }
        })
        .unwrap();
    runtime.set_partial_rebuilds(true);
    runtime.rebuild().unwrap();

    let panel = runtime.update(root, |root, _| root.panel).unwrap().unwrap();

    Setup {
        runtime,
        root,
        panel,
        drops,
    }
}

#[test]
fn an_unmounted_entity_is_dropped_when_the_releasing_call_returns() {
    let Setup { runtime, root, .. } = setup();
    let drops = Rc::new(Cell::new(0));
    let model = runtime.create(|_| Counted(drops.clone())).unwrap();

    runtime
        .update(root, |_, cx| {
            cx.release(model);

            // still there until the call returns
            assert_eq!(drops.get(), 0);
        })
        .unwrap();

    assert_eq!(drops.get(), 1);
    assert_eq!(
        runtime.update(model, |_, _| ()),
        Err(EntityAccessError::InvalidEntity)
    );
}

#[test]
fn the_host_drops_an_unmounted_entity_at_once() {
    let Setup { runtime, .. } = setup();
    let drops = Rc::new(Cell::new(0));
    let model = runtime.create(|_| Counted(drops.clone())).unwrap();

    runtime.release(model);

    assert_eq!(drops.get(), 1);
}

#[test]
fn an_entity_can_release_itself_from_its_own_update() {
    let Setup { runtime, .. } = setup();
    let drops = Rc::new(Cell::new(0));
    let model = runtime.create(|_| Counted(drops.clone())).unwrap();

    runtime
        .update(model, |_, cx| cx.release(cx.entity()))
        .unwrap();

    assert_eq!(drops.get(), 1);
}

#[test]
fn a_mounted_view_is_dropped_after_the_rebuild_that_unmounts_it() {
    let Setup {
        mut runtime,
        root,
        panel,
        drops,
    } = setup();

    runtime.update(root, Root::close_panel).unwrap();

    // still mounted, so it waits for the rebuild
    assert_eq!(drops.get(), 0);
    assert_eq!(
        runtime.update(panel, |_, _| ()),
        Ok(()),
        "the handle works until the entity drops"
    );

    runtime.rebuild().unwrap();

    assert_eq!(drops.get(), 1);
    assert_eq!(
        runtime.update(panel, |_, _| ()),
        Err(EntityAccessError::InvalidEntity)
    );
}

#[test]
fn a_view_its_parent_keeps_mounted_is_not_dropped() {
    let Setup {
        mut runtime,
        root,
        panel,
        drops,
    } = setup();

    runtime.update(root, |_, cx| cx.release(panel)).unwrap();
    runtime.update(root, |_, cx| cx.notify()).unwrap();
    runtime.rebuild().unwrap();

    assert_eq!(drops.get(), 0);
}

#[test]
fn the_root_and_released_entities_cannot_be_released() {
    let Setup { runtime, root, .. } = setup();
    let model = runtime.create(|_| ()).unwrap();

    assert_eq!(runtime.try_release(root), Err(ReleaseError::Root));

    runtime.release(model);

    assert_eq!(runtime.try_release(model), Err(ReleaseError::InvalidEntity));
}

#[test]
fn a_released_slot_goes_to_the_next_entity() {
    let Setup { runtime, .. } = setup();
    let old = runtime.create(|_| 1_u32).unwrap();
    runtime.release(old);

    let new = runtime.create(|_| 2_u32).unwrap();

    assert_ne!(new, old);
    assert_eq!(
        runtime.update(old, |_, _| ()),
        Err(EntityAccessError::InvalidEntity)
    );
    assert_eq!(runtime.update(new, |value, _| *value), Ok(2));
}

#[derive(Debug, Clone, Copy)]
struct Ping;

struct Source;

impl EventEmitter<Ping> for Source {}

struct Model;

impl EventEmitter<Ping> for Model {}

impl Model {
    fn on_ping(&mut self, _: Entity<Source>, _: &Ping, _: &mut Context<'_, Self>) {}

    fn clicked(&mut self, _: &ActivateEvent, _: &mut Context<'_, Self>) {}
}

struct Watcher;

impl Watcher {
    fn on_ping(&mut self, _: Entity<Model>, _: &Ping, _: &mut Context<'_, Self>) {}
}

#[test]
fn releasing_an_entity_forgets_its_subscriptions() {
    let Setup { runtime, .. } = setup();
    let source = runtime.create(|_| Source).unwrap();
    let watcher = runtime.create(|_| Watcher).unwrap();
    let before = runtime.subscription_count();

    let model = runtime
        .create(|cx| {
            cx.subscribe(source, Model::on_ping);
            Model
        })
        .unwrap();
    runtime
        .update(watcher, |_, cx| cx.subscribe(model, Watcher::on_ping))
        .unwrap();
    assert_eq!(runtime.subscription_count(), before + 2);

    runtime.release(model);

    assert_eq!(runtime.subscription_count(), before);
}

/// registers a listener on the model while rendering, so the reader owns a callback
/// that runs on the model
struct Reader {
    model: Entity<Model>,
}

impl Render for Reader {
    fn render(&mut self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        // a read that ends at once, so the update below can borrow the model
        self.model.read_with(cx, |_| ());
        let clicked = self.model.update(cx, |_, cx| cx.listener(Model::clicked));

        div().id("model").on_activate(clicked)
    }
}

#[test]
fn releasing_an_entity_forgets_its_callbacks_and_reads() {
    let mut runtime = Runtime::<()>::new(pool());
    let model = runtime.create(|_| Model).unwrap();
    runtime.create_root(|_| Reader { model }).unwrap();
    runtime.rebuild().unwrap();

    // a listener the model registered for itself
    runtime
        .update(model, |_, cx| {
            let _ = cx.listener(Model::clicked);
        })
        .unwrap();

    assert_eq!(runtime.callback_count(), 2);
    assert_eq!(runtime.dependency_count(), 1);

    runtime.release(model);

    assert_eq!(runtime.callback_count(), 0);
    assert_eq!(runtime.dependency_count(), 0);
}
