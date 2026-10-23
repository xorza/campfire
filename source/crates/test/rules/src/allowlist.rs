use std::cell::Cell;
use std::fmt::Debug;

/// A rule's exempt entries, each of which must still exempt something: the check asks it of each
/// candidate, which marks the entry that takes it, and `unused` names the entries none took, so a
/// stale one fails.
#[derive(Debug)]
pub(crate) struct Allowlist<T: 'static> {
    entries: &'static [T],
    used: Vec<Cell<bool>>,
}

impl<T: Debug> Allowlist<T> {
    pub(crate) fn new(entries: &'static [T]) -> Self {
        Self {
            entries,
            used: entries.iter().map(|_| Cell::new(false)).collect(),
        }
    }

    /// Whether an entry takes the candidate, as `takes` says of each, the first that does marked
    /// used.
    pub(crate) fn allows(&self, takes: impl Fn(&T) -> bool) -> bool {
        let Some(at) = self.entries.iter().position(takes) else {
            return false;
        };
        self.used[at].set(true);
        true
    }

    /// The entries no candidate took, in their order.
    pub(crate) fn unused(&self) -> Vec<&'static T> {
        self.entries
            .iter()
            .zip(&self.used)
            .filter(|(_, used)| !used.get())
            .map(|(entry, _)| entry)
            .collect()
    }
}

#[test]
fn an_entry_no_candidate_takes_is_unused() {
    const NAMES: [&str; 3] = ["a", "b", "c"];
    let names = Allowlist::new(&NAMES);
    assert!(names.allows(|name| *name == "b"));
    assert!(!names.allows(|name| *name == "d"));
    assert!(names.allows(|name| *name == "b"));
    assert_eq!(names.unused(), [&"a", &"c"]);
}
