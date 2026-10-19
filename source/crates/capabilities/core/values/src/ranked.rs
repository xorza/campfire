use std::slice;

use serde::{Deserialize, Serialize};

use crate::rank::Rank;

/// One value for every rank, or one per rank.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Ranked<T> {
    One(T),
    PerRank(Vec<T>),
}

impl<T> Ranked<T> {
    /// How many ranks it has values for; `None` for one value, which fits any rank.
    pub const fn ranks(&self) -> Option<usize> {
        match self {
            Self::One(_) => None,
            Self::PerRank(values) => Some(values.len()),
        }
    }

    /// Every value it holds, one or one per rank.
    pub fn values(&self) -> &[T] {
        match self {
            Self::One(value) => slice::from_ref(value),
            Self::PerRank(values) => values,
        }
    }

    /// The value at `rank`; `None` past the last rank.
    pub fn get(&self, rank: Rank) -> Option<&T> {
        match self {
            Self::One(value) => Some(value),
            Self::PerRank(values) => values.get(rank.index()),
        }
    }
}

impl<T> From<T> for Ranked<T> {
    fn from(value: T) -> Self {
        Self::One(value)
    }
}

/// One value for each rank of `values`, in order.
impl<T> FromIterator<T> for Ranked<T> {
    fn from_iter<I: IntoIterator<Item = T>>(values: I) -> Self {
        Self::PerRank(values.into_iter().collect())
    }
}

impl<T, const N: usize> From<[T; N]> for Ranked<T> {
    fn from(values: [T; N]) -> Self {
        values.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_is_one_and_a_list_one_a_rank() {
        assert_eq!(Ranked::from(4_u8), Ranked::One(4));
        assert_eq!(Ranked::from([1_u8, 2]), Ranked::PerRank(vec![1, 2]));
    }
}
