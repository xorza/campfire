use campfire_common::Tick;
use campfire_protocol::Applied;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// Tells a player where the inputs of a packet the server logged take effect, as Gambetta's
/// client-side prediction has the server acknowledge each input it processed: from the input of
/// seq `first` on, the tick each applies in, or none for one logged early, which never
/// takes effect. The client replays each in that tick, so a rollback predicts its own inputs
/// where the server applied them, not where it stamped them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct InputAck {
    pub first: u64,
    pub applied: Vec<Option<Tick>>,
}

/// What a peer sends is untrusted, so an ack whose seqs pass a `u64`, or whose ticks go back,
/// fails to decode: the client's list of its inputs keeps their ticks in order.
impl<'de> Deserialize<'de> for InputAck {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            first: u64,
            applied: Vec<Option<Tick>>,
        }
        let Fields { first, applied } = Fields::deserialize(deserializer)?;
        let len = u64::try_from(applied.len()).map_err(D::Error::custom)?;
        if first.checked_add(len).is_none() {
            return Err(D::Error::custom("an ack's seqs within a u64"));
        }
        if !applied.iter().flatten().is_sorted() {
            return Err(D::Error::custom("an ack's ticks in order"));
        }
        Ok(Self { first, applied })
    }
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

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;

    use super::*;

    #[test]
    fn an_ack_decodes_only_of_seqs_a_u64_holds_and_ticks_in_order() {
        let ack = |first, ticks: &[Option<u64>]| InputAck {
            first,
            applied: ticks.iter().map(|tick| tick.map(Tick::new)).collect(),
        };
        let decoded = |ack: &InputAck| Binary::decode::<InputAck>(&Binary::encode(ack)).ok();
        // Ticks that never decrease, an early input of none among them, read back.
        let kept = ack(7, &[Some(3), None, Some(3), Some(5)]);
        assert_eq!(decoded(&kept), Some(kept.clone()));
        // A tick before the one ahead of it, and seqs past the last a `u64` numbers, are refused;
        // the last seq itself is not.
        assert_eq!(decoded(&ack(7, &[Some(5), Some(4)])), None);
        assert_eq!(decoded(&ack(u64::MAX, &[Some(1)])), None);
        let last = ack(u64::MAX - 1, &[Some(1)]);
        assert_eq!(decoded(&last), Some(last.clone()));
    }
}
