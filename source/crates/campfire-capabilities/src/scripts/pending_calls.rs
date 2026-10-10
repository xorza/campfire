use std::slice;

use serde::{Deserialize, Serialize};

/// The calls of a hook that wait to run, in order: a stage adds its tick's at the end and runs
/// the calls from the front; those whose call found the pool spent stay, and run first in a later
/// tick. State, as a later tick runs them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct PendingCalls<T> {
    calls: Vec<T>,
}

impl<T> PendingCalls<T> {
    pub(crate) const fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }

    pub(crate) const fn len(&self) -> usize {
        self.calls.len()
    }

    /// The calls, first to last.
    pub(crate) fn iter(&self) -> slice::Iter<'_, T> {
        self.calls.iter()
    }

    /// Adds `calls` at the end.
    pub(crate) fn extend(&mut self, calls: impl IntoIterator<Item = T>) {
        self.calls.extend(calls);
    }

    /// Moves the calls of `later` to the end, leaving it empty.
    pub(crate) fn append(&mut self, later: &mut PendingCalls<T>) {
        self.calls.append(&mut later.calls);
    }

    /// Removes the first `count` calls, which ran.
    pub(crate) fn answered(&mut self, count: usize) {
        self.calls.drain(..count);
    }

    pub(crate) fn clear(&mut self) {
        self.calls.clear();
    }
}

impl<T> Default for PendingCalls<T> {
    fn default() -> PendingCalls<T> {
        PendingCalls { calls: Vec::new() }
    }
}

impl<T> From<Vec<T>> for PendingCalls<T> {
    fn from(calls: Vec<T>) -> PendingCalls<T> {
        PendingCalls { calls }
    }
}

#[cfg(test)]
mod tests {
    use campfire_common::Binary;

    use super::*;

    #[test]
    fn calls_run_from_the_front_and_later_ones_join_the_end() {
        let mut pending = PendingCalls::from(vec![1, 2, 3]);
        pending.answered(2);
        let mut later = PendingCalls::from(vec![4]);
        pending.append(&mut later);
        pending.extend([5]);
        assert_eq!(pending.iter().copied().collect::<Vec<_>>(), [3, 4, 5]);
        assert_eq!((pending.len(), later.is_empty()), (3, true));
        // As state, it is the list of its calls.
        let bytes = Binary::encode(&pending);
        assert_eq!(bytes, Binary::encode(&vec![3, 4, 5]));
        pending.clear();
        assert!(pending.is_empty());
    }
}
