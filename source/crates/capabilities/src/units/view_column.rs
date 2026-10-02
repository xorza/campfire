use std::any::Any;
use std::array;
use std::fmt;

use campfire_sim::Capability;

/// A capability's own part of the script view: what it reads of each unit, a row each in the
/// order the view reads the units, and what its getters read besides, such as its book. The
/// core names none of them.
pub(crate) trait ViewColumn: Any + fmt::Debug {
    /// Empties its rows for the next read, keeping its buffers.
    fn clear(&mut self);

    /// How many rows it holds: one for each unit the view read.
    fn rows(&self) -> usize;
}

/// The columns of the view, one for each capability that installs one.
#[derive(Debug)]
pub(crate) struct ViewColumns([Option<Box<dyn ViewColumn>>; Capability::ALL.len()]);

impl Default for ViewColumns {
    fn default() -> ViewColumns {
        ViewColumns(array::from_fn(|_| None))
    }
}

impl ViewColumns {
    /// Gives `capability` its column.
    pub(crate) fn add<C: ViewColumn>(&mut self, capability: Capability, column: C) {
        let slot = &mut self.0[capability as usize];
        debug_assert!(slot.is_none(), "a capability installs one column");
        *slot = Some(Box::new(column));
    }

    /// The column of `capability`, when it installed one of type `C`.
    pub(crate) fn get<C: ViewColumn>(&self, capability: Capability) -> Option<&C> {
        let column: &dyn Any = self.0[capability as usize].as_deref()?;
        column.downcast_ref()
    }

    /// The column of `capability`, to change, when it installed one of type `C`.
    pub(crate) fn get_mut<C: ViewColumn>(&mut self, capability: Capability) -> Option<&mut C> {
        let column: &mut dyn Any = self.0[capability as usize].as_deref_mut()?;
        column.downcast_mut()
    }

    /// Empties every column's rows for the next read.
    pub(crate) fn clear(&mut self) {
        for column in self.0.iter_mut().flatten() {
            column.clear();
        }
    }

    /// Whether every column holds `rows` rows.
    pub(crate) fn hold(&self, rows: usize) -> bool {
        self.0.iter().flatten().all(|column| column.rows() == rows)
    }
}
