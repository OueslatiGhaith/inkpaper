//! entities given up with [`Context::release`](crate::Context::release).
//!
//! a release only queues the entity: it may be borrowed, rendering, or still mounted in
//! the frame. The runtime drops it at the first safe point where it is none of these,
//! at the end of the outermost runtime call or of a rebuild.

use core::{cell::Cell, cell::RefCell};

use crate::{
    EntityId,
    pool::{OutOfMemory, PoolRef, PoolVec, Table},
};

/// why an entity can't be released
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ReleaseError {
    /// the entity was released already, or never existed
    InvalidEntity,
    /// the runtime renders the entity as its root
    Root,
    /// the pool had no room to queue the release
    OutOfMemory(OutOfMemory),
}

/// the entities waiting to be dropped
pub(crate) trait ReleaseStore {
    /// queues `entity`, once
    fn push(&self, entity: EntityId) -> Result<(), ReleaseError>;
}

/// holds nothing, for contexts outside a runtime. Every release fails
pub(crate) struct NoReleases;

impl ReleaseStore for NoReleases {
    fn push(&self, _: EntityId) -> Result<(), ReleaseError> {
        Err(ReleaseError::OutOfMemory(OutOfMemory {
            table: Table::Entities,
            requested: core::mem::size_of::<EntityId>(),
        }))
    }
}

/// the runtime's queued releases. They count as entity memory
pub(crate) struct ReleaseQueue {
    pending: RefCell<PoolVec<EntityId>>,
    /// the root entity, which can't be released
    root: Cell<Option<EntityId>>,
}

impl ReleaseQueue {
    pub(crate) fn new_in(pool: PoolRef) -> Self {
        Self {
            pending: RefCell::new(PoolVec::new_in(pool, Table::Entities)),
            root: Cell::new(None),
        }
    }

    pub(crate) fn set_root(&self, root: EntityId) {
        self.root.set(Some(root));
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pending.borrow().is_empty()
    }

    /// calls `drop` with each queued entity, and keeps those it returns false for
    pub(crate) fn drain(&self, mut drop: impl FnMut(EntityId) -> bool) {
        let mut index = 0;

        loop {
            // no guard is held while `drop` runs the entity's destructor
            let Some(entity) = self.pending.borrow().get(index).copied() else {
                return;
            };

            if drop(entity) {
                let mut pending = self.pending.borrow_mut();
                let last = pending.len() - 1;
                pending.swap(index, last);
                pending.truncate(last);
            } else {
                index += 1;
            }
        }
    }
}

impl ReleaseStore for ReleaseQueue {
    fn push(&self, entity: EntityId) -> Result<(), ReleaseError> {
        if self.root.get() == Some(entity) {
            return Err(ReleaseError::Root);
        }

        let mut pending = self.pending.borrow_mut();
        if pending.contains(&entity) {
            return Ok(());
        }

        pending.try_push(entity).map_err(ReleaseError::OutOfMemory)
    }
}
