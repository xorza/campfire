use std::ops::Range;
use std::{fmt, mem};

/// A column's rows of the running read, and those of the read before it, from which it copies
/// the row of each unit whose parts did not change.
#[derive(Debug, Default)]
pub struct KeptRows<R> {
    now: R,
    kept: R,
}

/// The rows of one read of a column.
pub trait ColumnRows: Default + PartialEq + fmt::Debug {
    fn clear(&mut self);

    /// Adds the rows `rows` of `from`, a run that is not empty.
    fn push_from(&mut self, from: &Self, rows: Range<usize>);

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A column's kept rows, of whatever type its rows are, as the view starts, keeps, counts and
/// compares them.
pub trait AnyKeptRows: fmt::Debug {
    /// Starts a read: the rows become those it keeps from, and it holds none.
    fn begin(&mut self);

    /// Adds the rows `rows` of the read before, unchanged.
    fn keep(&mut self, rows: Range<usize>);

    /// How many rows the running read holds.
    fn rows(&self) -> usize;

    /// Whether the rows of the running read equal those of the read before.
    fn same_as_kept(&self) -> bool;
}

impl<R: ColumnRows> AnyKeptRows for KeptRows<R> {
    fn begin(&mut self) {
        mem::swap(&mut self.now, &mut self.kept);
        self.now.clear();
    }

    fn keep(&mut self, rows: Range<usize>) {
        self.now.push_from(&self.kept, rows);
    }

    fn rows(&self) -> usize {
        self.now.len()
    }

    fn same_as_kept(&self) -> bool {
        self.now == self.kept
    }
}

impl<R: ColumnRows> KeptRows<R> {
    /// The rows of the running read.
    pub const fn now(&self) -> &R {
        &self.now
    }

    /// The rows of the running read, to add to.
    pub const fn now_mut(&mut self) -> &mut R {
        &mut self.now
    }
}

/// Where the runs of rows copied from one buffer to another move: those that started at `from`
/// there start at `to` here.
#[derive(Debug, Clone, Copy)]
pub struct RunMove {
    from: u32,
    to: u32,
}

impl RunMove {
    /// The move of runs that started at `from` to the end of a buffer of `to` items.
    pub fn new(from: u32, to: usize) -> Self {
        Self {
            from,
            to: u32::try_from(to).expect("a view's runs fit u32"),
        }
    }

    /// Where `index` lands.
    pub const fn at(self, index: u32) -> u32 {
        index - self.from + self.to
    }

    /// Where `run` lands.
    pub const fn of(self, run: &Range<u32>) -> Range<u32> {
        self.at(run.start)..self.at(run.end)
    }
}

/// Rows of one value each, as a column with no runs holds them.
impl<T: Clone + PartialEq + fmt::Debug> ColumnRows for Vec<T> {
    fn clear(&mut self) {
        Self::clear(self);
    }

    fn push_from(&mut self, from: &Self, rows: Range<usize>) {
        self.extend_from_slice(&from[rows]);
    }

    fn len(&self) -> usize {
        Self::len(self)
    }
}
