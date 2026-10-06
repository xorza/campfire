use crate::input_hash::InputHash;

/// Where a player's chain stands after an input of theirs whose journal record is durable: the
/// id of the delegation whose key signed the chain head then, the input's seq, and the head after
/// it, as a receipt names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableHead {
    pub delegation: [u8; 32],
    pub seq: u64,
    pub head: InputHash,
}
