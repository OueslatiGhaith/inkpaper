use std::vec::Vec;

use crate::pool::testing::{pool, pool_with_room};
use crate::*;

type TestRuntime = Runtime;

/// what a [`Child`] says happened
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ping {
    Value(u32),
    /// asks the subscriber to reset the child
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pong(u32);

/// emits [`Ping`]s, and [`Pong`]s when told to
struct Child {
    value: u32,
    /// a ping emitted while rendering
    ping_on_render: Option<u32>,
}

impl EventEmitter<Ping> for Child {}
impl EventEmitter<Pong> for Child {}

impl Child {
    const fn new() -> Self {
        Self {
            value: 0,
            ping_on_render: None,
        }
    }

    fn activate(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.value += 1;
        cx.emit(Ping::Reset);
    }
}

impl Render for Child {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        if let Some(value) = self.ping_on_render.take() {
            cx.emit(Ping::Value(value));
        }

        div()
            .id("child")
            .w(px(10))
            .h(px(10))
            .on_activate(cx.listener(Self::activate))
    }
}

/// subscribes to its first child's pings and pongs, and logs them
struct Parent {
    first: Entity<Child>,
    second: Entity<Child>,
    log: Vec<(u8, u32)>,
}

impl Parent {
    fn new(cx: &mut Context<'_, Self>) -> Self {
        let first = cx.new(|_| Child::new());
        let second = cx.new(|_| Child::new());

        cx.subscribe(first, Self::on_ping);
        cx.subscribe(first, Self::on_pong);

        Self {
            first,
            second,
            log: Vec::new(),
        }
    }

    fn on_ping(&mut self, child: Entity<Child>, ping: &Ping, cx: &mut Context<'_, Self>) {
        match *ping {
            Ping::Value(value) => self.log.push((b'p', value)),
            // the child that emitted is free to update again
            Ping::Reset => {
                let value = child.update(cx, |child, _| core::mem::take(&mut child.value));
                self.log.push((b'r', value));
            }
        }
    }

    fn on_pong(&mut self, _: Entity<Child>, pong: &Pong, _: &mut Context<'_, Self>) {
        self.log.push((b'o', pong.0));
    }
}

impl Render for Parent {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div().child(self.first)
    }
}

fn fixture() -> (TestRuntime, Entity<Parent>) {
    let mut runtime = TestRuntime::new(pool());
    let parent = runtime.create_root(Parent::new).unwrap();

    (runtime, parent)
}

fn log(runtime: &TestRuntime, parent: Entity<Parent>) -> Vec<(u8, u32)> {
    runtime
        .update(parent, |parent, _| parent.log.clone())
        .unwrap()
}

fn children(runtime: &TestRuntime, parent: Entity<Parent>) -> (Entity<Child>, Entity<Child>) {
    runtime
        .update(parent, |parent, _| (parent.first, parent.second))
        .unwrap()
}

#[test]
fn subscribers_get_events_once_the_emitting_call_returns() {
    let (runtime, parent) = fixture();
    let (first, _) = children(&runtime, parent);

    runtime
        .update(first, |_, cx| {
            cx.emit(Ping::Value(1));
            cx.emit(Pong(2));
            cx.emit(Ping::Value(3));

            // nothing is delivered while the emitter runs
            assert!(parent.read(cx, |parent| parent.log.is_empty()));
        })
        .unwrap();

    assert_eq!(log(&runtime, parent), [(b'p', 1), (b'o', 2), (b'p', 3)]);
}

#[test]
fn a_subscriber_can_update_the_entity_that_emitted() {
    let (runtime, parent) = fixture();
    let (first, _) = children(&runtime, parent);

    runtime
        .update(first, |child, cx| {
            child.value = 7;
            cx.emit(Ping::Reset);
        })
        .unwrap();

    assert_eq!(log(&runtime, parent), [(b'r', 7)]);
    assert_eq!(runtime.update(first, |child, _| child.value), Ok(0));
}

#[test]
fn only_the_subscribed_emitter_and_event_type_are_delivered() {
    let (runtime, parent) = fixture();
    let (_, second) = children(&runtime, parent);

    runtime
        .update(second, |_, cx| cx.emit(Ping::Value(1)))
        .unwrap();

    assert!(log(&runtime, parent).is_empty());
}

#[test]
fn nested_runtime_calls_deliver_when_the_outermost_returns() {
    let (runtime, parent) = fixture();
    let (first, second) = children(&runtime, parent);

    runtime
        .update(second, |_, _| {
            runtime
                .update(first, |_, cx| cx.emit(Ping::Value(1)))
                .unwrap();

            assert!(log(&runtime, parent).is_empty());
        })
        .unwrap();

    assert_eq!(log(&runtime, parent), [(b'p', 1)]);
}

#[test]
fn listeners_and_renders_deliver_what_they_emit() {
    let (mut runtime, parent) = fixture();
    let (first, _) = children(&runtime, parent);
    runtime.rebuild().unwrap();

    // a tap's listener
    assert!(runtime.focus_next());
    assert!(runtime.activate_focused().unwrap());
    assert_eq!(log(&runtime, parent), [(b'r', 1)]);

    // a render
    runtime
        .update(first, |child, cx| {
            child.ping_on_render = Some(5);
            cx.notify();
        })
        .unwrap();
    runtime.rebuild().unwrap();
    assert_eq!(log(&runtime, parent), [(b'r', 1), (b'p', 5)]);
}

/// collects the pongs of a [`Relayed`]
struct Collector {
    relay: Entity<Relayed>,
    got: Vec<u32>,
}

/// re-emits each ping of a child as a pong
struct Relayed {
    child: Entity<Child>,
}

impl EventEmitter<Pong> for Relayed {}

impl Relayed {
    fn on_ping(&mut self, _: Entity<Child>, ping: &Ping, cx: &mut Context<'_, Self>) {
        if let Ping::Value(value) = *ping {
            cx.emit(Pong(value * 10));
        }
    }
}

impl Collector {
    fn on_pong(&mut self, _: Entity<Relayed>, pong: &Pong, _: &mut Context<'_, Self>) {
        self.got.push(pong.0);
    }
}

#[test]
fn events_handlers_emit_are_delivered_too() {
    let runtime = TestRuntime::new(pool());
    let child = runtime.create(|_| Child::new()).unwrap();
    let relay = runtime
        .create(|cx| {
            cx.subscribe(child, Relayed::on_ping);
            Relayed { child }
        })
        .unwrap();
    let listener = runtime
        .create(|cx| {
            cx.subscribe(relay, Collector::on_pong);
            Collector {
                relay,
                got: Vec::new(),
            }
        })
        .unwrap();

    runtime
        .update(child, |_, cx| {
            cx.emit(Ping::Value(1));
            cx.emit(Ping::Value(2));
        })
        .unwrap();

    assert_eq!(
        runtime.update(listener, |listener, _| listener.got.clone()),
        Ok(std::vec![10, 20])
    );
    assert_eq!(
        runtime.update(listener, |listener, _| listener.relay),
        Ok(relay)
    );
    assert_eq!(runtime.update(relay, |relay, _| relay.child), Ok(child));
}

#[test]
fn a_full_queue_reports_it() {
    let mut runtime = TestRuntime::new(pool_with_room(2 * 1024));
    let parent = runtime.create_root(Parent::new).unwrap();
    let (first, _) = children(&runtime, parent);

    let results = runtime
        .update(first, |_, cx| {
            (0..256)
                .map(|value| cx.try_emit(Ping::Value(value)))
                .collect::<Vec<_>>()
        })
        .unwrap();
    let sent = results.iter().take_while(|result| result.is_ok()).count();

    assert!(sent > 0);
    assert!(results[sent..].iter().all(|result| matches!(
        result,
        Err(OutOfMemory {
            table: Table::Events,
            ..
        })
    )));
    // the ones that fit are delivered
    assert_eq!(log(&runtime, parent).len(), sent);
}
