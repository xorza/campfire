use std::any::{Any, TypeId};
use std::fmt;

/// A part of the script view that a capability, or the action pipeline, owns: what it reads of
/// each unit, a row each in the order the view reads the units, and what its getters read
/// besides, such as its book. The core names none of them.
pub(crate) trait ViewColumn: Any + fmt::Debug {
    /// Empties its rows for the next read, keeping its buffers.
    fn clear(&mut self);

    /// How many rows it holds: one for each unit the view read.
    fn rows(&self) -> usize;
}

/// The columns of the view, each of its own type, in the order they were added.
#[derive(Debug, Default)]
pub(crate) struct ViewColumns(Vec<Box<dyn ViewColumn>>);

impl ViewColumns {
    pub(crate) fn add<C: ViewColumn>(&mut self, column: C) {
        debug_assert!(self.get::<C>().is_none(), "one column of each type");
        self.0.push(Box::new(column));
    }

    /// The column of type `C`, when one was added.
    pub(crate) fn get<C: ViewColumn>(&self) -> Option<&C> {
        let column: &dyn Any = self
            .0
            .iter()
            .find(|column| (***column).type_id() == TypeId::of::<C>())?
            .as_ref();
        column.downcast_ref()
    }

    /// The column of type `C`, to change, when one was added.
    pub(crate) fn get_mut<C: ViewColumn>(&mut self) -> Option<&mut C> {
        let column: &mut dyn Any = self
            .0
            .iter_mut()
            .find(|column| (***column).type_id() == TypeId::of::<C>())?
            .as_mut();
        column.downcast_mut()
    }

    /// Empties every column's rows for the next read.
    pub(crate) fn clear(&mut self) {
        for column in &mut self.0 {
            column.clear();
        }
    }

    /// Whether every column holds `rows` rows.
    pub(crate) fn hold(&self, rows: usize) -> bool {
        self.0.iter().all(|column| column.rows() == rows)
    }
}
