use std::cmp::Ordering;
use std::ops::Range;

/// Names by index, every name in one buffer: what a book names its entries by, such as its
/// teams, its paths or its tags.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NameList {
    text: String,
    /// Where each name ends in `text`.
    ends: Vec<u32>,
}

impl NameList {
    /// Adds `name` after the others, and gives its index.
    pub(crate) fn push(&mut self, name: &str) -> usize {
        self.text.push_str(name);
        self.ends
            .push(u32::try_from(self.text.len()).expect("names fit u32"));
        self.ends.len() - 1
    }

    /// The name at `index`.
    pub(crate) fn get(&self, index: usize) -> Option<&str> {
        let end = *self.ends.get(index)? as usize;
        let start = index
            .checked_sub(1)
            .map_or(0, |before| self.ends[before] as usize);
        Some(&self.text[start..end])
    }

    /// The index of the first name that is `name`.
    pub(crate) fn named(&self, name: &str) -> Option<usize> {
        self.iter().position(|held| held == name)
    }

    /// The index of `name` among `within`, names sorted in that range, from its start.
    pub(crate) fn sorted_named(&self, within: Range<usize>, name: &str) -> Option<usize> {
        let (mut low, mut high) = (within.start, within.end);
        while low < high {
            let middle = low + (high - low) / 2;
            let held = self.get(middle).expect("a range within the names");
            match held.cmp(name) {
                Ordering::Less => low = middle + 1,
                Ordering::Greater => high = middle,
                Ordering::Equal => return Some(middle - within.start),
            }
        }
        None
    }

    pub(crate) const fn len(&self) -> usize {
        self.ends.len()
    }

    /// Every name, by index.
    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &str> {
        (0..self.len()).map(|at| self.get(at).expect("an index below the count"))
    }
}

impl<'a> FromIterator<&'a str> for NameList {
    fn from_iter<I: IntoIterator<Item = &'a str>>(names: I) -> NameList {
        let mut list = NameList::default();
        for name in names {
            list.push(name);
        }
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_keep_their_indices_and_runs_search_only_themselves() {
        let list: NameList = ["b", "", "a", "c", "b"].into_iter().collect();
        assert_eq!(list.len(), 5);
        assert_eq!(list.iter().collect::<Vec<_>>(), ["b", "", "a", "c", "b"]);
        assert_eq!(
            [0, 1, 4, 5].map(|at| list.get(at)),
            [Some("b"), Some(""), Some("b"), None]
        );
        // The first of two alike, and an empty name, which takes no byte.
        assert_eq!(
            ["b", "", "c", "d"].map(|name| list.named(name)),
            [Some(0), Some(1), Some(3), None]
        );
        // The sorted run of indices 2 to 4, "a", "c": each from the run's start; "b" at index 0
        // and 4 lies outside it.
        assert_eq!(
            ["a", "c", "b", ""].map(|name| list.sorted_named(2..4, name)),
            [Some(0), Some(1), None, None]
        );
        assert_eq!(list.sorted_named(2..2, "a"), None);
        assert_eq!(NameList::default().iter().count(), 0);
    }
}
