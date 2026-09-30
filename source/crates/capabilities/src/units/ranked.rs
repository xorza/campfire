use std::slice;

use serde::Deserialize;

/// One value for every rank, or one per rank.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Ranked<T> {
    One(T),
    PerRank(Vec<T>),
}

impl<T> Ranked<T> {
    /// How many ranks it has values for; `None` for one value, which fits any rank.
    pub fn ranks(&self) -> Option<usize> {
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
}

impl<T: Copy> Ranked<T> {
    /// The value at `rank`, from 1; `None` past the last rank.
    pub fn at(&self, rank: u8) -> Option<T> {
        match self {
            Ranked::One(value) => Some(*value),
            Ranked::PerRank(values) => values.get(usize::from(rank.checked_sub(1)?)).copied(),
        }
    }
}
