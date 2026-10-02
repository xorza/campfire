use std::ops::Range;

/// Values by name in runs, one run for each owner, such as the params of a unit type or an
/// ability: every name and every value in one buffer each, and each run sorted by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NameTable<V> {
    names: Vec<Box<str>>,
    values: Vec<V>,
    /// Where each run starts in `names` and `values`, then where the last one ends.
    starts: Vec<u32>,
}

impl<V> NameTable<V> {
    /// Adds a run of `entries`, sorted by name, and gives its index: the number of runs before it.
    pub(crate) fn push<'a>(&mut self, entries: impl IntoIterator<Item = (&'a str, V)>) -> usize {
        let start = self.names.len();
        for (name, value) in entries {
            self.names.push(name.into());
            self.values.push(value);
        }
        debug_assert!(self.names[start..].is_sorted(), "a run is sorted by name");
        self.starts
            .push(u32::try_from(self.names.len()).expect("names fit u32"));
        self.starts.len() - 2
    }

    /// Whether it holds run `run`.
    pub(crate) const fn has_run(&self, run: usize) -> bool {
        run + 1 < self.starts.len()
    }

    /// The place of `name` in run `run`.
    pub(crate) fn named(&self, run: usize, name: &str) -> Option<usize> {
        let names = &self.names[self.run(run)];
        names.binary_search_by(|held| (**held).cmp(name)).ok()
    }

    pub(crate) fn get_named(&self, run: usize, name: &str) -> Option<&V> {
        let at = self.named(run, name)?;
        Some(&self.values[self.run(run)][at])
    }

    /// The values of run `run`, in the order of their names.
    pub(crate) fn values(&self, run: usize) -> &[V] {
        &self.values[self.run(run)]
    }

    fn run(&self, run: usize) -> Range<usize> {
        self.starts[run] as usize..self.starts[run + 1] as usize
    }
}

impl<V> Default for NameTable<V> {
    fn default() -> NameTable<V> {
        NameTable {
            names: Vec::new(),
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
        assert_eq!(table.push([("a", 1), ("c", 3)]), 0);
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
        .map(|(run, name)| table.get_named(run, name).copied());
        assert_eq!(
            found,
            [Some(1), None, Some(3), None, Some(10), Some(20), None]
        );
        assert_eq!(table.named(2, "b"), Some(1));
        assert_eq!(table.values(0), [1, 3]);
        assert!(table.values(1).is_empty());
    }
}
