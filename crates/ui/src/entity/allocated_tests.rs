use super::*;
use std::{cell::Cell, rc::Rc, string::String};

#[test]
fn growth_preserves_addresses_during_shared_and_exclusive_borrows() {
    let arena = EntityArena::<0, 1>::default();
    let entity = arena.insert(7u32).unwrap();
    let original = arena.read(entity, |value| value as *const u32).unwrap();

    arena
        .read(entity, |value| {
            for index in 0..32 {
                arena.insert(index).unwrap();
            }

            assert_eq!(*value, 7);
            assert_eq!(value as *const u32, original);
            assert_eq!(
                arena.update(entity, |_| ()),
                Err(EntityAccessError::BorrowConflict)
            );
        })
        .unwrap();

    arena
        .update(entity, |value| {
            for index in 0..64 {
                arena.insert(index).unwrap();
            }

            assert_eq!(value as *const u32, original);
            assert_eq!(
                arena.read(entity, |_| ()),
                Err(EntityAccessError::BorrowConflict)
            );

            *value = 9;
        })
        .unwrap();

    assert_eq!(arena.read(entity, |value| *value), Ok(9));
}

#[test]
fn aligned_values_and_zsts_have_distinct_aligned_storage() {
    #[repr(align(128))]
    struct Aligned(u32);

    #[repr(align(64))]
    struct Marker;

    let arena = EntityArena::<0, 0>::default();
    let value = arena.insert(Aligned(42)).unwrap();

    arena
        .read(value, |value| {
            assert_eq!(value as *const Aligned as usize % 128, 0);
            assert_eq!(value.0, 42);
        })
        .unwrap();

    let first = arena.insert(Marker).unwrap();
    let second = arena.insert(Marker).unwrap();

    let first = arena.read(first, |value| value as *const Marker).unwrap();
    let second = arena.read(second, |value| value as *const Marker).unwrap();

    assert_ne!(first, second);
    assert_eq!(first as usize % 64, 0);
    assert_eq!(second as usize % 64, 0);
}

#[test]
fn abandoned_storage_is_released_without_dropping_an_uninitialized_value() {
    let arena = EntityArena::<0, 0>::default();
    let reservation = arena
        .reserve(
            Layout::new::<String>(),
            TypeId::of::<String>(),
            drop_value::<String>,
        )
        .unwrap();

    assert!(arena.used_bytes() > 0);

    arena.abandon(reservation.id);

    assert_eq!(arena.used_bytes(), 0);

    let abandoned = Entity::<String>::from_id(reservation.id);

    assert_eq!(
        arena.read(abandoned, |_| ()),
        Err(EntityAccessError::InvalidEntity)
    );

    let next = arena.insert(String::from("valid")).unwrap();

    assert_ne!(next.entity_id(), reservation.id);
}

#[test]
fn values_drop_once_in_reverse_reservation_order_even_after_growth() {
    struct Tracked {
        id: usize,
        next: Rc<Cell<usize>>,
    }

    impl Drop for Tracked {
        fn drop(&mut self) {
            assert_eq!(self.next.get(), self.id + 1);
            self.next.set(self.id);
        }
    }

    let next = Rc::new(Cell::new(32));

    {
        let arena = EntityArena::<0, 1>::default();

        for id in 0..32 {
            arena
                .insert(Tracked {
                    id,
                    next: next.clone(),
                })
                .unwrap();
        }

        assert_eq!(next.get(), 32);
    }

    assert_eq!(next.get(), 0);
}

#[test]
#[cfg_attr(miri, ignore)]
fn entity_id_exhaustion_does_not_wrap() {
    let arena = EntityArena::<0, 0>::default();

    for _ in 0..=u16::MAX {
        arena.insert(()).unwrap();
    }

    assert_eq!(arena.insert(()), Err(EntityAllocError::SlotsFull));
    assert_eq!(arena.len(), usize::from(u16::MAX) + 1);
}
