use core::{
    any::TypeId,
    cell::{Cell, RefCell},
};

use crate::{EntityId, storage::VecStorage};

/// state a render can read
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Entity(EntityId),
    Global(TypeId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dependency {
    reader: EntityId,
    source: Source,
}

/// the entities and globals each entity read during its last render, so a change
/// renders again only the entities that read it
pub trait DependencyStore {
    /// records that `reader`'s render read `source`
    fn record(&self, reader: EntityId, source: Source);

    /// forgets what `reader` read, before it renders again or after it leaves the frame
    fn forget(&self, reader: EntityId);

    /// calls `f` with every entity that read `source`
    fn for_each_reader(&self, source: Source, f: &mut dyn FnMut(EntityId));

    /// whether a read could not be recorded for lack of space. Until the next full
    /// rebuild clears the table, a change must render every entity
    fn is_incomplete(&self) -> bool;

    fn clear(&self);
}

/// records nothing, for contexts outside a runtime
pub(crate) struct NoDependencies;

impl DependencyStore for NoDependencies {
    fn record(&self, _: EntityId, _: Source) {}

    fn forget(&self, _: EntityId) {}

    fn for_each_reader(&self, _: Source, _: &mut dyn FnMut(EntityId)) {}

    fn is_incomplete(&self) -> bool {
        false
    }

    fn clear(&self) {}
}

pub(crate) struct Dependencies<V: VecStorage<Dependency>> {
    edges: RefCell<V>,
    incomplete: Cell<bool>,
}

impl<V: VecStorage<Dependency>> Default for Dependencies<V> {
    fn default() -> Self {
        Self {
            edges: RefCell::new(V::default()),
            incomplete: Cell::new(false),
        }
    }
}

impl<V: VecStorage<Dependency>> DependencyStore for Dependencies<V> {
    fn record(&self, reader: EntityId, source: Source) {
        if source == Source::Entity(reader) {
            return;
        }

        let dependency = Dependency { reader, source };
        let mut edges = self.edges.borrow_mut();
        if edges.contains(&dependency) {
            return;
        }

        if edges.try_push(dependency).is_err() {
            self.incomplete.set(true);
        }
    }

    fn forget(&self, reader: EntityId) {
        let mut edges = self.edges.borrow_mut();
        let mut kept = 0;

        for index in 0..edges.len() {
            if edges[index].reader != reader {
                edges[kept] = edges[index];
                kept += 1;
            }
        }

        edges.truncate(kept);
    }

    fn for_each_reader(&self, source: Source, f: &mut dyn FnMut(EntityId)) {
        for dependency in self.edges.borrow().iter() {
            if dependency.source == source {
                f(dependency.reader);
            }
        }
    }

    fn is_incomplete(&self) -> bool {
        self.incomplete.get()
    }

    fn clear(&self) {
        self.edges.borrow_mut().clear();
        self.incomplete.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(slot: u16) -> EntityId {
        EntityId::new(slot, 0)
    }

    fn readers(dependencies: &dyn DependencyStore, source: Source) -> heapless::Vec<EntityId, 8> {
        let mut readers = heapless::Vec::new();
        dependencies.for_each_reader(source, &mut |reader| readers.push(reader).unwrap());
        readers
    }

    #[test]
    fn records_each_read_once() {
        let dependencies = Dependencies::<heapless::Vec<Dependency, 8>>::default();

        dependencies.record(entity(0), Source::Entity(entity(1)));
        dependencies.record(entity(0), Source::Entity(entity(1)));
        dependencies.record(entity(2), Source::Entity(entity(1)));
        // reading itself is not a dependency
        dependencies.record(entity(1), Source::Entity(entity(1)));

        assert_eq!(
            readers(&dependencies, Source::Entity(entity(1))),
            [entity(0), entity(2)]
        );
    }

    #[test]
    fn forgetting_a_reader_keeps_the_others() {
        let dependencies = Dependencies::<heapless::Vec<Dependency, 8>>::default();
        let theme = Source::Global(TypeId::of::<u8>());

        dependencies.record(entity(0), theme);
        dependencies.record(entity(1), theme);
        dependencies.record(entity(0), Source::Entity(entity(1)));
        dependencies.forget(entity(0));

        assert_eq!(readers(&dependencies, theme), [entity(1)]);
        assert!(readers(&dependencies, Source::Entity(entity(1))).is_empty());
    }

    #[test]
    fn a_full_table_is_incomplete_until_cleared() {
        let dependencies = Dependencies::<heapless::Vec<Dependency, 1>>::default();

        dependencies.record(entity(0), Source::Entity(entity(1)));
        assert!(!dependencies.is_incomplete());

        dependencies.record(entity(0), Source::Entity(entity(2)));
        assert!(dependencies.is_incomplete());

        dependencies.clear();
        assert!(!dependencies.is_incomplete());
    }
}
