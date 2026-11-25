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
pub(crate) struct LeaseOwner(pub(crate) u32);

#[derive(Debug, Clone, Copy)]
struct Lease {
    owner: LeaseOwner,
    entity: EntityId,
}

/// the error for a lease when a context has no lease table
pub(crate) fn no_lease_room() -> OutOfMemory {
    OutOfMemory {
        table: Table::Entities,
        requested: core::mem::size_of::<Lease>(),
    }
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

    /// an owner no live context uses
    pub(crate) fn next_owner(&self) -> LeaseOwner {
        let owner = self.next_owner.get();
        self.next_owner.set(owner.wrapping_add(1));

        LeaseOwner(owner)
    }

    /// whether `owner` holds a lease on `entity`
    pub(crate) fn holds(&self, owner: LeaseOwner, entity: EntityId) -> bool {
        self.leases
            .borrow()
            .iter()
            .any(|lease| lease.owner == owner && lease.entity == entity)
    }

    pub(crate) fn push(&self, owner: LeaseOwner, entity: EntityId) -> Result<(), OutOfMemory> {
        self.leases.borrow_mut().try_push(Lease { owner, entity })
    }

    /// removes the leases of `owner`, handing each entity to `release`
    pub(crate) fn release_owner(&self, owner: LeaseOwner, mut release: impl FnMut(EntityId)) {
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
