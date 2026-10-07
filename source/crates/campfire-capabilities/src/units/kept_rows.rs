use std::mem;

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

    /// Adds row `row` of `from`.
    fn push_from(&mut self, from: &Self, row: usize);

    fn len(&self) -> usize;
}

impl<R: ColumnRows> KeptRows<R> {
    /// Starts a read: the rows become those it keeps from, and it holds none.
    pub(crate) fn begin(&mut self) {
        mem::swap(&mut self.now, &mut self.kept);
        self.now.clear();
    }

    /// Adds row `row` of the read before, unchanged.
    pub(crate) fn keep(&mut self, row: usize) {
        self.now.push_from(&self.kept, row);
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

/// Rows of one value each, as a column with no runs holds them.
impl<T: Clone + PartialEq> ColumnRows for Vec<T> {
    fn clear(&mut self) {
        Vec::clear(self);
    }

    fn push_from(&mut self, from: &Self, row: usize) {
        self.push(from[row].clone());
    }

    fn len(&self) -> usize {
        Vec::len(self)
    }
}
