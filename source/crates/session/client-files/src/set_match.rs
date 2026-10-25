/// How a set of a state's conditions matches a unit's: how many of them the unit holds, and how
/// many it lacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetMatch {
    pub common: u32,
    pub lacked: u32,
}

impl SetMatch {
    /// The state a unit draws of states whose sets match it as `states` gives, each state's sets
    /// in order: the one of the set of the most conditions in common with the unit's, of a tie the
    /// fewest the unit lacks, of a tie the first, as Zero Hour's
    /// `SparseMatchFinder::findBestInfoSlow` finds it (02-engine-core.md, A look is states of
    /// conditions); the first for no set.
    pub fn best<S>(states: impl IntoIterator<Item = S>) -> usize
    where
        S: IntoIterator<Item = Self>,
    {
        let mut best = 0;
        let (mut most, mut fewest) = (0, u32::MAX);
        for (at, sets) in states.into_iter().enumerate() {
            for Self { common, lacked } in sets {
                if common > most || (common >= most && lacked < fewest) {
                    (best, most, fewest) = (at, common, lacked);
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_best_state_shares_the_most_conditions_then_lacks_the_fewest_then_comes_first() {
        let set = |common, lacked| SetMatch { common, lacked };
        // The second state's second set shares 2, past the first's 1.
        assert_eq!(
            SetMatch::best([vec![set(1, 0)], vec![set(0, 0), set(2, 1)]]),
            1
        );
        // Of 1 in common each, the one that lacks none; of a full tie, the first.
        assert_eq!(SetMatch::best([vec![set(1, 2)], vec![set(1, 0)]]), 1);
        assert_eq!(SetMatch::best([vec![set(1, 1)], vec![set(1, 1)]]), 0);
        // None in common: the fewest lacked; no set at all, the first.
        assert_eq!(SetMatch::best([vec![set(0, 1)], vec![set(0, 0)]]), 1);
        assert_eq!(SetMatch::best(Vec::<Vec<SetMatch>>::new()), 0);
    }
}
