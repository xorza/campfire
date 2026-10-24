use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// A part of the install the import drops, by its name in the install, and why: a command
/// button, by its own; a supply module, by its object's; a weapon, by its own.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Dropped<C> {
    pub name: String,
    pub cause: C,
}

/// What a stage drops, each part once for each cause it drops by, in the order it first drops
/// it, as the stage reads the install.
#[derive(Debug)]
pub(crate) struct Losses<C> {
    dropped: Vec<Dropped<C>>,
    seen: BTreeSet<Dropped<C>>,
}

impl<C: Ord + Clone> Losses<C> {
    /// Drops the part `name` for `cause`, unless it dropped it for that cause before.
    pub(crate) fn drop(&mut self, name: &str, cause: C) {
        let dropped = Dropped {
            name: name.to_owned(),
            cause,
        };
        if !self.seen.contains(&dropped) {
            self.seen.insert(dropped.clone());
            self.dropped.push(dropped);
        }
    }
}

impl<C> Default for Losses<C> {
    fn default() -> Self {
        Self {
            dropped: Vec::new(),
            seen: BTreeSet::new(),
        }
    }
}

impl<C> IntoIterator for Losses<C> {
    type Item = Dropped<C>;
    type IntoIter = std::vec::IntoIter<Dropped<C>>;

    fn into_iter(self) -> Self::IntoIter {
        self.dropped.into_iter()
    }
}

impl<'a, C> IntoIterator for &'a Losses<C> {
    type Item = &'a Dropped<C>;
    type IntoIter = std::slice::Iter<'a, Dropped<C>>;

    fn into_iter(self) -> Self::IntoIter {
        self.dropped.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_part_is_dropped_once_for_each_cause_in_the_order_first_dropped() {
        let mut losses = Losses::default();
        for (name, cause) in [("b", 1), ("a", 2), ("b", 1), ("b", 2), ("a", 2)] {
            losses.drop(name, cause);
        }
        let dropped: Vec<(&str, i32)> = (&losses)
            .into_iter()
            .map(|dropped| (dropped.name.as_str(), dropped.cause))
            .collect();
        assert_eq!(dropped, [("b", 1), ("a", 2), ("b", 2)]);
        assert_eq!(losses.into_iter().count(), 3);
    }
}
