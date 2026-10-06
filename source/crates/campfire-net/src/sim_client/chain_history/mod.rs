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

    /// The seq of the input after the last it holds.
    pub(crate) fn next_seq(&self) -> u64 {
        self.first_seq + u64::try_from(self.heads.len()).expect("a history fits u64")
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
mod tests;
