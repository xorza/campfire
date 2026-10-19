use std::slice;

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// The calls of a hook that wait to run, in order: a stage adds its tick's at the end and runs
/// the calls from the front; those whose call found the pool spent stay, and run first in a later
/// tick. The first may wait with some of the mode's scripts answered, and a later tick runs it in
/// the rest (03-game-scripting.md, An event waits in the scripts that did not answer it). State,
/// as a later tick runs them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingCalls<T> {
    calls: Vec<T>,
    /// How many of the mode's scripts answered the first call, which waits for the rest; none
    /// with no call.
    partial: u8,
}

impl<T> PendingCalls<T> {
    pub const fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }

    pub const fn len(&self) -> usize {
        self.calls.len()
    }

    /// The calls, first to last.
    pub fn iter(&self) -> slice::Iter<'_, T> {
        self.calls.iter()
    }

    /// How many of the mode's scripts answered the first call, which waits for the rest.
    pub const fn partial(&self) -> u8 {
        self.partial
    }

    /// Moves the calls of `later` to the end, leaving it empty; none of them ran in any script.
    pub fn append(&mut self, later: &mut Self) {
        debug_assert_eq!(later.partial, 0, "a later call ran in no script");
        self.calls.append(&mut later.calls);
    }

    /// Removes the first `count` calls, which ran, and records that `partial` of the mode's
    /// scripts answered the call then first, which waits for the rest.
    pub fn answered(&mut self, count: usize, partial: u8) {
        self.calls.drain(..count);
        debug_assert!(
            partial == 0 || !self.calls.is_empty(),
            "only a call that waits ran in part"
        );
        self.partial = partial;
    }

    pub fn clear(&mut self) {
        self.calls.clear();
        self.partial = 0;
    }
}

impl<T> Default for PendingCalls<T> {
    fn default() -> Self {
        Self {
            calls: Vec::new(),
            partial: 0,
        }
    }
}

impl<T> FromIterator<T> for PendingCalls<T> {
    fn from_iter<I: IntoIterator<Item = T>>(calls: I) -> Self {
        Self {
            calls: calls.into_iter().collect(),
            partial: 0,
        }
    }
}

/// Adds the calls at the end.
impl<T> Extend<T> for PendingCalls<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, calls: I) {
        self.calls.extend(calls);
    }
}

impl<T, const N: usize> From<[T; N]> for PendingCalls<T> {
    fn from(calls: [T; N]) -> Self {
        calls.into_iter().collect()
    }
}

impl<'a, T> IntoIterator for &'a PendingCalls<T> {
    type Item = &'a T;
    type IntoIter = slice::Iter<'a, T>;

    fn into_iter(self) -> slice::Iter<'a, T> {
        self.iter()
    }
}

/// A snapshot is untrusted, so a call that ran in part with no call to run fails to decode; a
/// count past the mode's scripts runs the call in none.
impl<'de, T: Deserialize<'de>> Deserialize<'de> for PendingCalls<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(bound = "T: Deserialize<'de>")]
        struct Fields<T> {
            calls: Vec<T>,
            partial: u8,
        }
        let Fields { calls, partial } = Fields::deserialize(deserializer)?;
        if partial > 0 && calls.is_empty() {
            return Err(D::Error::custom("a call that ran in part"));
        }
        Ok(Self { calls, partial })
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;

    use super::*;

    #[test]
    fn calls_run_from_the_front_and_later_ones_join_the_end() {
        let mut pending = PendingCalls::from([1, 2, 3]);
        pending.answered(2, 0);
        let mut later = PendingCalls::from([4]);
        pending.append(&mut later);
        pending.extend([5]);
        assert_eq!(pending.iter().copied().collect::<Vec<_>>(), [3, 4, 5]);
        assert_eq!((pending.len(), later.is_empty()), (3, true));
        // The call now first waits, answered by 2 of the mode's scripts; once it runs in the
        // rest, the next starts in none.
        pending.answered(0, 2);
        assert_eq!((pending.len(), pending.partial()), (3, 2));
        // As state, it is the list of its calls, then that count.
        let bytes = Binary::encode(&pending);
        let mut list = Binary::encode(&vec![3, 4, 5]);
        list.push(2);
        assert_eq!(bytes, list);
        assert_eq!(
            Binary::decode::<PendingCalls<i32>>(&bytes).unwrap(),
            pending
        );
        pending.answered(1, 0);
        assert_eq!((pending.len(), pending.partial()), (2, 0));
        pending.clear();
        assert!(pending.is_empty());
        // No call that waits with a count fails to decode.
        let mut none = Binary::encode(&Vec::<i32>::new());
        none.push(1);
        assert!(Binary::decode::<PendingCalls<i32>>(&none).is_err());
    }
}
