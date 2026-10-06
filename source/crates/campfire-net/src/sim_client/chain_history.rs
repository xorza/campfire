use campfire_protocol::{InputChain, InputHash};

/// The heads of the player's chain from a point on, by seq: where the chain stood then, and the
/// head after each input since. A client that joins again finds where the log's copy of its chain
/// stands in its own, and cuts the inputs after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChainHistory {
    /// The seq of the first input after `base`.
    first_seq: u64,
    base: InputHash,
    heads: Vec<InputHash>,
}

impl ChainHistory {
    /// The history of `chain` from where it stands.
    pub(crate) fn of(chain: &InputChain) -> ChainHistory {
        ChainHistory {
            first_seq: chain.next_seq(),
            base: chain.head(),
            heads: Vec::new(),
        }
    }

    /// Notes the head after the chain's next input.
    pub(crate) fn push(&mut self, head: InputHash) {
        self.heads.push(head);
    }

    /// The head after the chain's first `next_seq` inputs; none outside what it holds.
    pub(crate) fn head_at(&self, next_seq: u64) -> Option<InputHash> {
        let after = usize::try_from(next_seq.checked_sub(self.first_seq)?).ok()?;
        match after {
            0 => Some(self.base),
            _ => self.heads.get(after - 1).copied(),
        }
    }

    /// Forgets the heads before the one after the chain's first `next_seq` inputs, which it
    /// must hold: a receipt confirms the server holds those inputs durably.
    pub(crate) fn forget_before(&mut self, next_seq: u64) {
        let base = self
            .head_at(next_seq)
            .expect("a receipt within the history");
        let dropped = usize::try_from(next_seq - self.first_seq).expect("a history fits memory");
        self.heads.drain(..dropped);
        self.first_seq = next_seq;
        self.base = base;
    }

    /// Drops the heads after the chain's first `next_seq` inputs, which it must hold; how many it
    /// drops.
    pub(crate) fn cut(&mut self, next_seq: u64) -> u64 {
        assert!(self.head_at(next_seq).is_some(), "a cut within the history");
        let keep = usize::try_from(next_seq - self.first_seq).expect("a history fits memory");
        let dropped = self.heads.len() - keep;
        self.heads.truncate(keep);
        u64::try_from(dropped).expect("a count fits u64")
    }
}

#[cfg(test)]
mod tests {
    use campfire_common::{PlayerSlot, Tick};

    use super::*;

    #[test]
    fn a_history_gives_each_head_it_holds_and_cuts_to_one() {
        // From a chain that holds 2 inputs already: seq 2 links to its head, and the history
        // notes the heads after seq 2, 3 and 4.
        let mut chain = InputChain::new(PlayerSlot::new(1), InputHash::new([1; 32]));
        chain.extend(Tick::new(0), b"a");
        chain.extend(Tick::new(0), b"b");
        let mut history = ChainHistory::of(&chain);
        let mut heads = vec![chain.head()];
        for payload in [b"c", b"d", b"e"] {
            chain.extend(Tick::new(1), payload);
            history.push(chain.head());
            heads.push(chain.head());
        }
        assert_eq!(
            [1, 2, 3, 4, 5, 6].map(|next_seq| history.head_at(next_seq)),
            [
                None,
                Some(heads[0]),
                Some(heads[1]),
                Some(heads[2]),
                Some(heads[3]),
                None
            ]
        );
        // The log holds 3 of the 5: the two after it go.
        assert_eq!(history.cut(3), 2);
        assert_eq!(history.head_at(3), Some(heads[1]));
        assert_eq!(history.head_at(4), None);
        assert_eq!(history.cut(2), 1);
        assert_eq!(history.cut(2), 0);
        // Two inputs again, then a receipt for the first, seq 2: the history holds from 3 on.
        history.push(heads[1]);
        history.push(heads[2]);
        history.forget_before(3);
        assert_eq!(history.head_at(2), None);
        assert_eq!(history.head_at(3), Some(heads[1]));
        assert_eq!(history.head_at(4), Some(heads[2]));
        assert_eq!(history.head_at(5), None);
    }
}
