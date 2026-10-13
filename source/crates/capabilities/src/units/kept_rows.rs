use std::mem;
use std::ops::Range;

/// A column's rows of the running read, and those of the read before it, from which it copies
/// the row of each unit whose parts did not change.
#[derive(Debug, Default)]
pub(crate) struct KeptRows<R> {
    now: R,
    kept: R,
}

/// The rows of one read of a column.
pub(crate) trait ColumnRows: Default + PartialEq {
    fn clear(&mut self);

    /// Adds the rows `rows` of `from`, a run that is not empty.
    fn push_from(&mut self, from: &Self, rows: Range<usize>);

    fn len(&self) -> usize;
}

impl<R: ColumnRows> KeptRows<R> {
    /// Starts a read: the rows become those it keeps from, and it holds none.
    pub(crate) fn begin(&mut self) {
        mem::swap(&mut self.now, &mut self.kept);
        self.now.clear();
    }

    /// Adds the rows `rows` of the read before, unchanged.
    pub(crate) fn keep(&mut self, rows: Range<usize>) {
        self.now.push_from(&self.kept, rows);
    }

    /// The rows of the running read.
    pub(crate) const fn now(&self) -> &R {
        &self.now
    }

    /// The rows of the running read, to add to.
    pub(crate) const fn now_mut(&mut self) -> &mut R {
        &mut self.now
    }

    pub(crate) fn len(&self) -> usize {
        self.now.len()
    }

    /// Whether the rows of the running read equal those of the read before.
    pub(crate) fn same_as_kept(&self) -> bool {
        self.now == self.kept
    }
}

/// Where the runs of rows copied from one buffer to another move: those that started at `from`
/// there start at `to` here.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RunMove {
    from: u32,
    to: u32,
}

impl RunMove {
    /// The move of runs that started at `from` to the end of a buffer of `to` items.
    pub(crate) fn new(from: u32, to: usize) -> RunMove {
        RunMove {
            from,
            to: u32::try_from(to).expect("a view's runs fit u32"),
        }
    }

    /// Where `index` lands.
    pub(crate) const fn at(self, index: u32) -> u32 {
        index - self.from + self.to
    }

    /// Where `run` lands.
    pub(crate) const fn of(self, run: &Range<u32>) -> Range<u32> {
        self.at(run.start)..self.at(run.end)
    }
}

/// Rows of one value each, as a column with no runs holds them.
impl<T: Clone + PartialEq> ColumnRows for Vec<T> {
    fn clear(&mut self) {
        Vec::clear(self);
    }

    fn push_from(&mut self, from: &Self, rows: Range<usize>) {
        self.extend_from_slice(&from[rows]);
    }

    fn len(&self) -> usize {
        Vec::len(self)
    }
}
