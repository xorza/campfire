use std::any::Any;
use std::fmt;
use std::ops::Range;

use bevy_ecs::world::World;

use crate::scripts::typed_list::{TypedItem, TypedList};
use crate::scripts::view::kept_rows::AnyKeptRows;

/// A part of the script view that a capability, or the action pipeline, owns: what it reads of
/// each unit, a row each in the order the view reads the units, kept from the read before, and
/// what its getters read besides, such as its book. The core names none of them.
pub trait ViewColumn: Any + fmt::Debug {
    /// Its rows of the running read, and those of the read before.
    fn kept(&self) -> &dyn AnyKeptRows;

    fn kept_mut(&mut self) -> &mut dyn AnyKeptRows;

    /// Takes what its rows derive from besides the units' parts, as a read of `world` starts.
    /// Whether every row must be filled again, as that changed.
    fn begin(&mut self, world: &World) -> bool;
}

/// A column of any type, as the view holds it: what the view does with every column's rows.
trait AnyColumn: TypedItem + fmt::Debug {
    /// Takes what its rows derive from besides the units' parts, as a read of `world` starts.
    /// Whether every row must be filled again.
    fn begin(&mut self, world: &World) -> bool;

    /// Starts a read that changes rows: the rows it holds become those it keeps from, and it
    /// holds none.
    fn start_rows(&mut self);

    /// Adds the rows `rows` of the read before, unchanged.
    fn keep(&mut self, rows: Range<usize>);

    /// How many rows it holds: one for each unit the view read.
    fn rows(&self) -> usize;

    /// Whether the rows of the running read equal those of the read before.
    fn same_as_kept(&self) -> bool;
}

impl<C: ViewColumn> AnyColumn for C {
    fn begin(&mut self, world: &World) -> bool {
        ViewColumn::begin(self, world)
    }

    fn start_rows(&mut self) {
        self.kept_mut().begin();
    }

    fn keep(&mut self, rows: Range<usize>) {
        self.kept_mut().keep(rows);
    }

    fn rows(&self) -> usize {
        self.kept().rows()
    }

    fn same_as_kept(&self) -> bool {
        self.kept().same_as_kept()
    }
}

/// The columns of the view, each of its own type, in the order they were added.
#[derive(Debug, Default)]
pub struct ViewColumns(TypedList<dyn AnyColumn>);

impl ViewColumns {
    pub fn add<C: ViewColumn>(&mut self, column: C) {
        self.0.add(Box::new(column));
    }

    /// The place of the column of type `C`, when one was added.
    pub fn index_of<C: ViewColumn>(&self) -> Option<usize> {
        self.0.index_of::<C>()
    }

    /// The column of type `C`, when one was added.
    pub fn get<C: ViewColumn>(&self) -> Option<&C> {
        self.0.get()
    }

    /// The column of type `C`, to change, when one was added.
    pub fn get_mut<C: ViewColumn>(&mut self) -> Option<&mut C> {
        self.0.get_mut()
    }

    /// The column at `at`, of type `C`.
    pub fn at_mut<C: ViewColumn>(&mut self, at: usize) -> &mut C {
        self.0.typed_mut(at)
    }

    /// Takes what the column at `at` derives from besides the units' parts, as a read of `world`
    /// starts. Whether every row must be filled again.
    pub fn begin(&mut self, at: usize, world: &World) -> bool {
        self.0.at_mut(at).begin(world)
    }

    /// Starts a read that changes rows in the column at `at`: its rows become those it keeps
    /// from, and it holds none.
    pub fn start_rows(&mut self, at: usize) {
        self.0.at_mut(at).start_rows();
    }

    /// Adds the rows `rows` of the read before to the column at `at`, unchanged.
    pub fn keep(&mut self, at: usize, rows: Range<usize>) {
        self.0.at_mut(at).keep(rows);
    }

    /// Whether every column holds `rows` rows.
    pub fn hold(&self, rows: usize) -> bool {
        self.0.iter().all(|column| column.rows() == rows)
    }

    /// Whether every column's rows of the running read equal those of the read before.
    pub fn same_as_kept(&self) -> bool {
        self.0.iter().all(AnyColumn::same_as_kept)
    }
}
