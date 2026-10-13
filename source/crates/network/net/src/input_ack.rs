use campfire_common::Tick;
use campfire_protocol::Applied;
use serde::{Deserialize, Serialize};

/// Tells a player where the inputs of a packet the server logged take effect, as Gambetta's
/// client-side prediction has the server acknowledge each input it processed: from the input of
/// seq `first` on, the tick each applies in, or none for one logged late or early, which never
/// takes effect. The client replays each in that tick, so a rollback predicts its own inputs
/// where the server applied them, not where it stamped them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct InputAck {
    pub(crate) first: u64,
    pub(crate) applied: Vec<Option<Tick>>,
}

impl InputAck {
    /// The acknowledgment of the inputs from seq `first` on, as the log placed them.
    pub(crate) fn of(first: u64, applied: &[Applied]) -> InputAck {
        let applied = applied
            .iter()
            .map(|applied| match *applied {
                Applied::At(tick) => Some(tick),
                Applied::Late | Applied::Early => None,
            })
            .collect();
        InputAck { first, applied }
    }
}
