use std::slice;

use bevy_ecs::schedule::InternedSystemSet;

use crate::declare::module_set::ModuleSet;

/// The sets one module set runs after and before, in the order its `ModuleSet::order` names
/// them.
#[derive(Debug, Default)]
pub(crate) struct SetOrder {
    edges: Vec<SetEdge>,
}

/// One set a module set runs after or before.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SetEdge {
    pub(crate) set: InternedSystemSet,
    pub(crate) side: Side,
}

/// Which side of the other set a module set runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    After,
    Before,
}

impl SetOrder {
    /// It runs after `set`.
    pub(crate) fn after<S: ModuleSet>(&mut self, set: S) -> &mut SetOrder {
        self.push(set, Side::After)
    }

    /// It runs before `set`.
    pub(crate) fn before<S: ModuleSet>(&mut self, set: S) -> &mut SetOrder {
        self.push(set, Side::Before)
    }

    fn push<S: ModuleSet>(&mut self, set: S, side: Side) -> &mut SetOrder {
        self.edges.push(SetEdge {
            set: set.intern(),
            side,
        });
        self
    }

    pub(crate) fn clear(&mut self) {
        self.edges.clear();
    }

    pub(crate) fn iter(&self) -> slice::Iter<'_, SetEdge> {
        self.edges.iter()
    }
}

impl<'a> IntoIterator for &'a SetOrder {
    type Item = &'a SetEdge;
    type IntoIter = slice::Iter<'a, SetEdge>;

    fn into_iter(self) -> slice::Iter<'a, SetEdge> {
        self.iter()
    }
}
