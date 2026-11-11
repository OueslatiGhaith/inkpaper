//! shared entity borrows handed out as references.
//!
//! [`Entity::read`](crate::Entity::read) returns a `&T` that lives as long as the
//! [`Context`](crate::Context) it was read through. The entity stays borrowed until
//! then: the context records a lease here and releases it when it drops. Leases are
//! kept per context, not on a stack, because a nested listener can read through an
//! outer context while an inner one is alive.

use core::cell::{Cell, RefCell};

use crate::{
    EntityId,
    pool::{OutOfMemory, PoolRef, PoolVec, Table},
};

/// which [`Context`](crate::Context) holds a lease
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LeaseOwner(u32);

#[derive(Debug, Clone, Copy)]
struct Lease {
    owner: LeaseOwner,
    entity: EntityId,
}

/// the leases of every live context
pub(crate) trait LeaseStore {
    /// an owner no live context uses
    fn next_owner(&self) -> LeaseOwner;

    /// whether `owner` holds a lease on `entity`
    fn holds(&self, owner: LeaseOwner, entity: EntityId) -> bool;

    fn push(&self, owner: LeaseOwner, entity: EntityId) -> Result<(), OutOfMemory>;

    /// removes the leases of `owner`, handing each entity to `release`
    fn release_owner(&self, owner: LeaseOwner, release: &mut dyn FnMut(EntityId));
}

/// holds nothing, for contexts outside a runtime. Every lease fails
pub(crate) struct NoLeases;

impl LeaseStore for NoLeases {
    fn next_owner(&self) -> LeaseOwner {
        LeaseOwner(0)
    }

    fn holds(&self, _: LeaseOwner, _: EntityId) -> bool {
        false
    }

    fn push(&self, _: LeaseOwner, _: EntityId) -> Result<(), OutOfMemory> {
        Err(OutOfMemory {
            table: Table::Entities,
            requested: core::mem::size_of::<Lease>(),
        })
    }

    fn release_owner(&self, _: LeaseOwner, _: &mut dyn FnMut(EntityId)) {}
}

/// the runtime's leases. They count as entity memory
pub(crate) struct LeaseTable {
    leases: RefCell<PoolVec<Lease>>,
    /// the owner given to the next context. Contexts live only during one runtime
    /// call, so an owner can't wrap around to one still alive
    next_owner: Cell<u32>,
}

impl LeaseTable {
    pub(crate) fn new_in(pool: PoolRef) -> Self {
        Self {
            leases: RefCell::new(PoolVec::new_in(pool, Table::Entities)),
            next_owner: Cell::new(0),
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.leases.borrow().len()
    }
}

impl LeaseStore for LeaseTable {
    fn next_owner(&self) -> LeaseOwner {
        let owner = self.next_owner.get();
        self.next_owner.set(owner.wrapping_add(1));

        LeaseOwner(owner)
    }

    fn holds(&self, owner: LeaseOwner, entity: EntityId) -> bool {
        self.leases
            .borrow()
            .iter()
            .any(|lease| lease.owner == owner && lease.entity == entity)
    }

    fn push(&self, owner: LeaseOwner, entity: EntityId) -> Result<(), OutOfMemory> {
        self.leases.borrow_mut().try_push(Lease { owner, entity })
    }

    fn release_owner(&self, owner: LeaseOwner, release: &mut dyn FnMut(EntityId)) {
        let mut leases = self.leases.borrow_mut();
        let mut index = 0;

        while let Some(lease) = leases.get(index).copied() {
            if lease.owner != owner {
                index += 1;
                continue;
            }

            let last = leases.len() - 1;
            leases.swap(index, last);
            leases.truncate(last);

            release(lease.entity);
        }
    }
}
