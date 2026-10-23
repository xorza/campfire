use campfire_common::Tick;
use campfire_protocol::Applied;
use serde::{Deserialize, Serialize};

/// Tells a player where the inputs of a packet the server logged take effect, as Gambetta's
/// client-side prediction has the server acknowledge each input it processed: from the input of
/// seq `first` on, the tick each applies in, or none for one logged early, which never
/// takes effect. The client replays each in that tick, so a rollback predicts its own inputs
/// where the server applied them, not where it stamped them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputAck {
    pub first: u64,
    pub applied: Vec<Option<Tick>>,
}

impl InputAck {
    /// Makes it the acknowledgment of the inputs from seq `first` on, as the log placed them, in
    /// its own buffer, which a server keeps from ack to ack.
    pub fn fill(&mut self, first: u64, applied: &[Applied]) {
        self.first = first;
        self.applied.clear();
        self.applied
            .extend(applied.iter().map(|applied| match *applied {
                Applied::At(tick) => Some(tick),
                Applied::Early => None,
            }));
    }
}
