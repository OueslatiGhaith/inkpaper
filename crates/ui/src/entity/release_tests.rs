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
