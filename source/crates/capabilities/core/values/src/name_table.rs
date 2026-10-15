use std::ops::Range;

use crate::names::{NameId, Names};

/// Values by name in runs, one run for each owner, such as the params of a unit type or an
/// ability: every name in one table, its scope the run, and every value in one buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameTable<V> {
    names: Names<Entry>,
    values: Vec<V>,
    /// Where each run starts in `values`, then where the last one ends.
    starts: Vec<u32>,
}

/// An entry of a `NameTable`, by its place among all the table's, in the scope of its run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Entry(u32);

impl NameId for Entry {
    type Scope = u32;
    const LIMIT: usize = u32::MAX as usize;

    fn at(index: usize) -> Entry {
        Entry(u32::try_from(index).expect("entries fit u32"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

impl<V> NameTable<V> {
    /// Adds a run of `entries`, each name once, and gives its index: the number of runs before it.
    pub fn push<'a>(&mut self, entries: impl IntoIterator<Item = (&'a str, V)>) -> usize {
        let run = self.starts.len() - 1;
        let scope = u32::try_from(run).expect("runs fit u32");
        for (name, value) in entries {
            self.names.add((scope, name));
            self.values.push(value);
        }
        self.starts
            .push(u32::try_from(self.values.len()).expect("entries fit u32"));
        run
    }

    /// How many runs it holds.
    pub const fn runs(&self) -> usize {
        self.starts.len() - 1
    }

    /// Whether it holds run `run`.
    pub const fn has_run(&self, run: usize) -> bool {
        run + 1 < self.starts.len()
    }

    /// The place of `name` in run `run`; none for a run it does not hold.
    pub fn named(&self, run: usize, name: &str) -> Option<usize> {
        let range = self.run(run)?;
        let scope = u32::try_from(run).ok()?;
        let entry = self.names.named((scope, name))?;
        Some(entry.index() - range.start)
    }

    /// The values of run `run`, in the order they were pushed; none for a run it does not hold.
    pub fn values(&self, run: usize) -> &[V] {
        self.run(run).map_or(&[], |range| &self.values[range])
    }

    /// Where run `run` lies in `values`, when it holds the run.
    fn run(&self, run: usize) -> Option<Range<usize>> {
        let end = *self.starts.get(run + 1)?;
        Some(self.starts[run] as usize..end as usize)
    }
}

impl<V> Default for NameTable<V> {
    fn default() -> NameTable<V> {
        NameTable {
            names: Names::default(),
            values: Vec::new(),
            starts: vec![0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_run_finds_only_its_own_names() {
        let mut table = NameTable::default();
        assert_eq!(table.push([("c", 3), ("a", 1)]), 0);
        assert_eq!(table.push([]), 1);
        assert_eq!(table.push([("a", 10), ("b", 20)]), 2);
        let found = [
            (0, "a"),
            (0, "b"),
            (0, "c"),
            (1, "a"),
            (2, "a"),
            (2, "b"),
            (2, "c"),
        ]
        .map(|(run, name)| table.named(run, name));
        assert_eq!(
            found,
            [Some(1), None, Some(0), None, Some(0), Some(1), None]
        );
        assert_eq!(table.values(0), [3, 1]);
        assert!(table.values(1).is_empty());
        assert!(table.has_run(2) && !table.has_run(3));
        // A run past the last holds nothing.
        assert!(table.values(3).is_empty());
        assert_eq!(table.named(3, "a"), None);
    }
}
