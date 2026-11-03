/// A set of a model's named nodes, by their places among the names its look shows and hides, as
/// `LookNodes::names` lists them, at most `NodeMask::MOST`: held inline, so a shot that hides a
/// clip's round sets a bit and names no node (02-engine-core.md, A model shows its clip's rounds).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NodeMask([u64; WORDS]);

/// The words a mask holds its bits in.
const WORDS: usize = 4;

impl NodeMask {
    /// The most names a model's look shows and hides, past which its load names no more.
    pub(crate) const MOST: usize = WORDS * 64;

    /// The mask of the nodes at `places`, each below `MOST`, as a model's load places them.
    pub(crate) fn of(places: impl IntoIterator<Item = usize>) -> Self {
        let mut mask = Self::default();
        for at in places {
            assert!(
                at < Self::MOST,
                "a node's place {at} past the most a mask holds"
            );
            mask.0[at / 64] |= 1 << (at % 64);
        }
        mask
    }

    /// Whether it holds the node at `at`.
    pub(crate) const fn contains(self, at: usize) -> bool {
        at < Self::MOST && self.0[at / 64] & (1 << (at % 64)) != 0
    }

    /// The nodes of it or of `other`.
    #[must_use]
    pub(crate) fn union(self, other: Self) -> Self {
        Self(std::array::from_fn(|word| self.0[word] | other.0[word]))
    }

    /// The nodes of it that `other` does not hold.
    #[must_use]
    pub(crate) fn minus(self, other: Self) -> Self {
        Self(std::array::from_fn(|word| self.0[word] & !other.0[word]))
    }

    /// The nodes one of it and `other` holds and the other does not.
    #[must_use]
    pub(crate) fn changed(self, other: Self) -> Self {
        Self(std::array::from_fn(|word| self.0[word] ^ other.0[word]))
    }

    /// Its nodes' places, in order, each word's set bits from its lowest.
    pub(crate) fn iter(self) -> impl Iterator<Item = usize> {
        (0..self.0.len()).flat_map(move |word| {
            let mut bits = self.0[word];
            std::iter::from_fn(move || {
                let low = bits.trailing_zeros();
                (bits != 0).then(|| {
                    bits &= bits - 1;
                    word * 64 + usize::try_from(low).expect("a bit's place fits usize")
                })
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mask_holds_its_places_and_joins_takes_and_compares_another() {
        let a = NodeMask::of([0, 3, 64, 255]);
        let b = NodeMask::of([3, 100]);
        assert_eq!(a.iter().collect::<Vec<_>>(), [0, 3, 64, 255]);
        assert!(a.contains(255) && !a.contains(1) && !a.contains(NodeMask::MOST));
        assert_eq!(a.union(b).iter().collect::<Vec<_>>(), [0, 3, 64, 100, 255]);
        assert_eq!(a.minus(b).iter().collect::<Vec<_>>(), [0, 64, 255]);
        assert_eq!(a.changed(b).iter().collect::<Vec<_>>(), [0, 64, 100, 255]);
        assert_eq!(NodeMask::default().iter().count(), 0);
    }
}
