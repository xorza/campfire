use std::any::{Any, TypeId};
use std::fmt;

use bevy_ecs::world::World;

/// A part of the script view that a capability, or the action pipeline, owns: what it reads of
/// each unit, a row each in the order the view reads the units, and what its getters read
/// besides, such as its book. The core names none of them.
pub(crate) trait ViewColumn: Any + fmt::Debug {
    /// Starts a read of `world`: the rows it holds become those it keeps from, and it holds none.
    /// Whether every row must be filled again, as something its rows derive from besides the
    /// units' parts changed.
    fn begin(&mut self, world: &World) -> bool;

    /// Adds row `row` of the read before, unchanged.
    fn keep(&mut self, row: usize);

    /// How many rows it holds: one for each unit the view read.
    fn rows(&self) -> usize;

    /// Whether the rows of the running read equal those of the read before.
    fn same_as_kept(&self) -> bool;
}

/// The columns of the view, each of its own type, in the order they were added.
#[derive(Debug, Default)]
pub(crate) struct ViewColumns {
    /// The type of each column, to find one without a call through its table.
    types: Vec<TypeId>,
    columns: Vec<Box<dyn ViewColumn>>,
}

impl ViewColumns {
    pub(crate) fn add<C: ViewColumn>(&mut self, column: C) {
        debug_assert!(self.get::<C>().is_none(), "one column of each type");
        self.types.push(TypeId::of::<C>());
        self.columns.push(Box::new(column));
    }

    /// The place of the column of type `C`, when one was added.
    pub(crate) fn index_of<C: ViewColumn>(&self) -> Option<usize> {
        self.types.iter().position(|&ty| ty == TypeId::of::<C>())
    }

    /// The column of type `C`, when one was added.
    pub(crate) fn get<C: ViewColumn>(&self) -> Option<&C> {
        let column: &dyn Any = self.columns[self.index_of::<C>()?].as_ref();
        column.downcast_ref()
    }

    /// The column of type `C`, to change, when one was added.
    pub(crate) fn get_mut<C: ViewColumn>(&mut self) -> Option<&mut C> {
        let at = self.index_of::<C>()?;
        Some(self.at_mut(at))
    }

    /// The column at `at`, of type `C`.
    pub(crate) fn at_mut<C: ViewColumn>(&mut self, at: usize) -> &mut C {
        let column: &mut dyn Any = self.columns[at].as_mut();
        column
            .downcast_mut()
            .expect("the column at a place is of the type found there")
    }

    /// The column at `at`, of any type.
    pub(crate) fn any_mut(&mut self, at: usize) -> &mut dyn ViewColumn {
        self.columns[at].as_mut()
    }

    /// Whether every column holds `rows` rows.
    pub(crate) fn hold(&self, rows: usize) -> bool {
        self.columns.iter().all(|column| column.rows() == rows)
    }

    /// Whether every column's rows of the running read equal those of the read before.
    pub(crate) fn same_as_kept(&self) -> bool {
        self.columns.iter().all(|column| column.same_as_kept())
    }
}
