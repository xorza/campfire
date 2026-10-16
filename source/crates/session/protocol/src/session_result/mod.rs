use campfire_codec::Binary;
use campfire_common::{StateHash, Tick};
use serde::{Deserialize, Serialize};

use crate::signed::{InSession, SignatureDomain, Signed};

/// How a session ended, which the server signs as the last record of its log: the tick the
/// session stopped before, how it ended, and the state hash after the tick before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionResult {
    pub tick: Tick,
    pub outcome: Outcome,
    pub state_hash: StateHash,
}

/// How a session ended: its match won by the team of index `team` or drawn, as the mode ended
/// it, or aborted before the mode ended it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Won { team: u8 },
    Draw,
    Aborted,
}

/// `domain ‖ session id ‖ postcard of the result`, as the server key signs a result.
impl Signed for InSession<'_, SessionResult> {
    const DOMAIN: SignatureDomain = SignatureDomain::new("campfire/result/v1");

    fn write_body(&self, message: &mut Vec<u8>) {
        message.extend_from_slice(self.session_id.as_bytes());
        Binary::encode_into(self.record, &mut *message);
    }
}

#[cfg(test)]
mod tests;
