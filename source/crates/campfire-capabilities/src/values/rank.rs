use std::num::NonZeroU8;

use serde::{Deserialize, Serialize};

/// A rank of an action, from 1: what a slot holds its action at once it is learned, and what
/// its values, its params and a modifier it applies are read at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Rank(NonZeroU8);

impl Rank {
    pub const FIRST: Rank = Rank(NonZeroU8::MIN);

    /// Rank `rank`; `None` for 0, which is no rank.
    pub const fn new(rank: u8) -> Option<Rank> {
        match NonZeroU8::new(rank) {
            Some(rank) => Some(Rank(rank)),
            None => None,
        }
    }

    pub const fn get(self) -> u8 {
        self.0.get()
    }

    /// Its place among an action's ranks, from 0 for the first.
    pub const fn index(self) -> usize {
        (self.0.get() - 1) as usize
    }

    /// The rank above it; `None` past the last a `u8` counts.
    pub const fn next(self) -> Option<Rank> {
        match self.0.checked_add(1) {
            Some(next) => Some(Rank(next)),
            None => None,
        }
    }

    /// The count of a slot's rank, 0 before it is learned, as scripts and players read it.
    pub fn count(rank: Option<Rank>) -> u8 {
        rank.map_or(0, Rank::get)
    }
}

/// The first, the rank a call of no action, as the mode's, reads its params at.
impl Default for Rank {
    fn default() -> Rank {
        Rank::FIRST
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rank_counts_from_1() {
        assert_eq!(Rank::new(0), None);
        let third = Rank::new(3).unwrap();
        assert_eq!((third.get(), third.index()), (3, 2));
        assert_eq!((Rank::FIRST.get(), Rank::FIRST.index()), (1, 0));
        assert_eq!(third.next(), Rank::new(4));
        assert_eq!(Rank::new(u8::MAX).unwrap().next(), None);
        assert_eq!((Rank::count(None), Rank::count(Some(third))), (0, 3));
        // A rank decodes from 1 up; 0 is none.
        let decode = |rank: u8| postcard::from_bytes::<Rank>(&[rank]).ok();
        assert_eq!((decode(0), decode(3)), (None, Some(third)));
    }
}
