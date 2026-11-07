use super::*;
use crate::{
    Offset,
    interaction::scroll::ScrollStateTable,
    pool::testing::{pool_ref, pool_ref_of},
    px,
};

#[test]
fn growing_tables_preserve_offsets_and_reused_slots_reset_them() {
    let parent = IdentityParent::Entity(EntityId::new(0, 0));
    let mut states = ElementStateTable::new_in(pool_ref());
    let mut scroll = ScrollStateTable::new_in(pool_ref());

    let old = states.resolve(parent, ElementId::Value(0), 1).unwrap();
    scroll.prepare(states.slot_count()).unwrap();

    let offset = Offset::new(px(0), px(17));
    scroll.set_offset(old, offset);

    for id in 1..128 {
        states.resolve(parent, ElementId::Value(id), 1).unwrap();
    }

    scroll.prepare(states.slot_count()).unwrap();
    states.sweep(1);

    assert_eq!(states.len(), 128);
    assert!(states.contains(old));
    assert_eq!(scroll.offset(old), offset);

    states.sweep(2);

    let replacement = states.resolve(parent, ElementId::Value(0), 3).unwrap();

    assert_eq!(replacement.slot(), old.slot());
    assert_ne!(replacement.generation(), old.generation());
    assert!(!states.contains(old));
    assert_eq!(scroll.offset(replacement), Offset::ZERO);

    scroll.set_offset(replacement, Offset::new(px(0), px(3)));

    assert_eq!(scroll.offset(old), Offset::ZERO);
}

#[test]
fn failed_scroll_reservation_can_abort_new_identities_without_losing_old_state() {
    let parent = IdentityParent::Entity(EntityId::new(0, 0));
    let mut states = ElementStateTable::new_in(pool_ref());
    let mut scroll = ScrollStateTable::new_in(pool_ref());

    let old = states.resolve(parent, ElementId::Value(0), 1).unwrap();
    states.sweep(1);
    scroll.prepare(states.slot_count()).unwrap();

    let offset = Offset::new(px(0), px(17));
    scroll.set_offset(old, offset);

    assert_eq!(states.resolve(parent, ElementId::Value(0), 2).unwrap(), old);

    let new = states.resolve(parent, ElementId::Value(1), 2).unwrap();

    assert_eq!(
        scroll.prepare(usize::MAX),
        Err(IdentityError::OutOfMemory(OutOfMemory {
            table: Table::ElementStates,
            requested: usize::MAX,
        }))
    );

    states.abort_frame(2);

    assert!(states.contains(old));
    assert!(!states.contains(new));
    assert_eq!(states.entry(old).unwrap().last_seen_frame, 1);
    assert_eq!(scroll.offset(old), offset);

    let retry = states.resolve(parent, ElementId::Value(1), 3).unwrap();
    scroll.prepare(states.slot_count()).unwrap();

    assert_ne!(retry, new);
    assert_eq!(scroll.offset(retry), Offset::ZERO);
}

#[test]
fn running_out_of_ids_reports_full_states() {
    let parent = IdentityParent::Entity(EntityId::new(0, 0));
    // room for every id
    let mut full = ElementStateTable::new_in(pool_ref_of(16 * 1024 * 1024));

    full.resolve(parent, ElementId::Value(0), 1).unwrap();
    full.slots
        .try_resize(usize::from(u16::MAX) + 1, full.slots[0])
        .unwrap();

    assert_eq!(
        full.resolve(parent, ElementId::Value(1), 1),
        Err(IdentityError::StatesFull)
    );
}
