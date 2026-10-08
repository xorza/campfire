use std::slice;

use serde::Deserialize;

use crate::values::rank::Rank;

/// One value for every rank, or one per rank.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Ranked<T> {
    One(T),
    PerRank(Vec<T>),
}

impl<T> Ranked<T> {
    /// How many ranks it has values for; `None` for one value, which fits any rank.
    pub const fn ranks(&self) -> Option<usize> {
        match self {
            Ranked::One(_) => None,
            Ranked::PerRank(values) => Some(values.len()),
        }
    }

    /// Every value it holds, one or one per rank.
    pub fn values(&self) -> &[T] {
        match self {
            Ranked::One(value) => slice::from_ref(value),
            Ranked::PerRank(values) => values,
        }
    }

    /// The value at `rank`; `None` past the last rank.
    pub fn get(&self, rank: Rank) -> Option<&T> {
        match self {
            Ranked::One(value) => Some(value),
            Ranked::PerRank(values) => values.get(rank.index()),
        }
    }
}
