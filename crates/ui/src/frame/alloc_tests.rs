use core::any::TypeId;

use super::*;
use crate::{callback::CallbackId, *};

#[test]
fn growing_frame_preserves_text_links_bindings_and_caches() {
    let mut frame = FrameArena::<1, 1>::default();
    let root = frame.push_div(Style::default()).unwrap();
    let event = TypeId::of::<ActivateEvent>();
    let mut previous = None;

    for index in 0..128 {
        let node = frame.push_text("مرحبا", TextStyle::default()).unwrap();
        frame.append_child(root, node);
        frame
            .bind_event(node, event, CallbackId::new(index, 0))
            .unwrap();
        frame.cache_measurement(node, Size::ZERO, Size::ZERO);

        if let Some(previous) = previous {
            assert_eq!(frame.node(previous).next_sibling, Some(node));
        }
        previous = Some(node);
    }

    assert_eq!(frame.node_count(), 129);
    assert_eq!(frame.text_bytes_used(), 128 * "مرحبا".len());
    assert_eq!(frame.node(root).last_child, previous);

    let mut current = frame.node(root).first_child;
    for index in 0..128 {
        let node = current.unwrap();
        assert_eq!(frame.node(node).parent, Some(root));

        let NodeKind::Text { text } = frame.node(node).kind else {
            panic!("expected text")
        };

        assert_eq!(frame.text(text), "مرحبا");
        assert_eq!(frame.cached_measurement(node, Size::ZERO), Some(Size::ZERO));
        assert_eq!(
            frame.event_callbacks(node, event).next(),
            Some(CallbackId::new(index, 0))
        );

        current = frame.node(node).next_sibling;
    }

    assert_eq!(current, None);
}

#[test]
fn clear_reuses_capacity_and_explicit_shrink_preserves_live_text() {
    let mut frame = FrameArena::<0, 0>::default();
    for _ in 0..128 {
        frame.push_text("reusable", TextStyle::default()).unwrap();
    }

    let capacities = (frame.node_capacity(), frame.text_capacity());
    let pointers = (
        frame.nodes.as_ptr(),
        frame.text.as_ptr(),
        frame.node_cache.as_ptr(),
    );

    frame.clear();
    for _ in 0..128 {
        frame.push_text("reusable", TextStyle::default()).unwrap();
    }

    assert_eq!((frame.node_capacity(), frame.text_capacity()), capacities);
    assert_eq!(
        (
            frame.nodes.as_ptr(),
            frame.text.as_ptr(),
            frame.node_cache.as_ptr()
        ),
        pointers
    );

    frame.clear();
    let node = frame.push_text("small", TextStyle::default()).unwrap();

    assert_eq!(frame.cached_measurement(node, Size::ZERO), None);

    frame.shrink_to_fit();

    assert!(frame.node_capacity() < capacities.0);
    assert!(frame.text_capacity() < capacities.1);

    let NodeKind::Text { text } = frame.node(node).kind else {
        panic!("expected text")
    };
    assert_eq!(frame.text(text), "small");

    frame.clear();
    frame.shrink_to_fit();

    assert_eq!(frame.node_capacity(), 0);
    assert_eq!(frame.text_capacity(), 0);

    frame
        .push_text("grows again", TextStyle::default())
        .unwrap();
}

#[test]
fn reservation_failure_keeps_nodes_caches_and_text_consistent() {
    let mut nodes = FrameArena::<{ usize::MAX }, 0>::default();

    assert_eq!(
        nodes.push_text("rollback", TextStyle::default()),
        Err(MountError::AllocationFailed)
    );
    assert_eq!(nodes.node_count(), 0);
    assert_eq!(nodes.text_bytes_used(), 0);
    assert_eq!(nodes.node_cache.len(), 0);

    let mut text = FrameArena::<0, { usize::MAX }>::default();
    let root = text.push_div(Style::default()).unwrap();

    assert_eq!(
        text.push_text("failure", TextStyle::default()),
        Err(MountError::AllocationFailed)
    );
    assert_eq!(text.node_count(), 1);
    assert_eq!(text.node_cache.len(), 1);
    assert_eq!(text.text_bytes_used(), 0);
    assert_eq!(text.node(root).first_child, None);
}

#[test]
fn growing_storage_respects_node_and_binding_id_limits() {
    let mut frame = FrameArena::<0, 0>::default();
    let root = frame.push_div(Style::default()).unwrap();

    for _ in 1..=u16::MAX {
        frame.push_div(Style::default()).unwrap();
    }

    assert_eq!(
        frame.push_text("rollback", TextStyle::default()),
        Err(MountError::NodesFull)
    );
    assert_eq!(frame.node_count(), usize::from(u16::MAX) + 1);
    assert_eq!(frame.text_bytes_used(), 0);
    assert_eq!(frame.node_cache.len(), frame.node_count());

    let event = TypeId::of::<ActivateEvent>();
    let callback = CallbackId::new(0, 0);

    for _ in 0..=u16::MAX {
        frame.bind_event(root, event, callback).unwrap();
    }

    assert_eq!(
        frame.bind_event(root, event, callback),
        Err(MountError::EventBindingsFull)
    );
    assert_eq!(
        frame.event_callbacks(root, event).count(),
        usize::from(u16::MAX) + 1
    );
}
