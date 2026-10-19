use crate::message_ref::MessageRef;
use crate::send_message_key::SendMessageKey;
use campfire_net_core::NetId;

/// A channel-owned message that is due in a flush, and how it competes: one for each message, a
/// fragmented one's included, whose fragments the packet builder takes in turn.
///
/// The channel keeps the message until
/// `ChannelSender::commit_send` runs after
/// its packet goes to the session. The packet builder reads it through [`CandidateMessages`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct SendCandidate {
    pub(crate) channel_id: NetId,
    pub(crate) key: SendMessageKey,
    pub(crate) rank: CandidateRank,
    /// A single message's bytes on the wire, which packing orders by; 0 for a fragmented one.
    pub(crate) bytes_len: u32,
}

/// How a candidate competes with the others of its flush.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CandidateRank {
    /// The message's priority scaled by its channel's.
    pub(crate) priority: f32,
    /// Its age among the messages of its channel and shape, independent of wrapping ids.
    pub(crate) send_order: u32,
}

impl SendCandidate {
    pub(crate) const fn is_single(&self) -> bool {
        self.key.is_single()
    }
}

/// Appends the candidates of one channel.
#[derive(Debug)]
pub(crate) struct CandidateSink<'a> {
    pub(crate) channel_id: NetId,
    pub(crate) channel_priority: f32,
    pub(crate) candidates: &'a mut Vec<SendCandidate>,
}

impl CandidateSink<'_> {
    /// Appends the message `key` names, with its `rank` scaled by the channel's priority. On a
    /// channel of infinite priority, a message that has not waited yet scores infinite too, not
    /// the `NaN` of `0 × ∞`, whose place in the order depends on its sign.
    pub(crate) fn push(
        &mut self,
        key: SendMessageKey,
        mut rank: CandidateRank,
        message: MessageRef<'_>,
    ) {
        let scaled = rank.priority * self.channel_priority;
        rank.priority = if scaled.is_nan() {
            self.channel_priority
        } else {
            scaled
        };
        let bytes_len = match message {
            MessageRef::Single(single) => {
                u32::try_from(single.bytes_len()).expect("a single message fits one packet")
            }
            MessageRef::Fragment(_) => 0,
        };
        self.candidates.push(SendCandidate {
            channel_id: self.channel_id,
            key,
            rank,
            bytes_len,
        });
    }
}

/// Where the packet builder reads the message of each candidate.
pub(crate) trait CandidateMessages {
    /// The single message or fragment `key` names on the channel `channel_id`.
    fn message(&self, channel_id: NetId, key: SendMessageKey) -> MessageRef<'_>;

    /// The key of the next due fragment of the message whose fragment `key` names, past it.
    fn next_due(&self, channel_id: NetId, key: SendMessageKey) -> Option<SendMessageKey>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::single_data::SingleData;
    use bytes::Bytes;

    fn pushed(channel_priority: f32, priority: f32) -> f32 {
        let mut candidates = Vec::new();
        let single = SingleData::new(None, Bytes::new());
        CandidateSink {
            channel_id: NetId::new(0),
            channel_priority,
            candidates: &mut candidates,
        }
        .push(
            SendMessageKey::UnreliableSingle(0),
            CandidateRank {
                priority,
                send_order: 0,
            },
            MessageRef::Single(&single),
        );
        candidates[0].rank.priority
    }

    /// The sink scales a message's priority by its channel's: 0.5 on a channel of 10 scores 5,
    /// and 2 scores 20. On an infinite channel a waiting message scores infinite, and so does
    /// one that has not waited yet, whose `0 × ∞` is `NaN`.
    #[test]
    fn the_sink_scales_priority_by_the_channel() {
        assert_eq!(pushed(10.0, 0.5), 5.0);
        assert_eq!(pushed(10.0, 2.0), 20.0);
        assert_eq!(pushed(f32::INFINITY, 0.5), f32::INFINITY);
        assert_eq!(pushed(f32::INFINITY, 0.0), f32::INFINITY);
    }

    /// A flush sorts its candidates, so each takes 24 bytes: the channel id, a key of a slot and
    /// a fragment, the priority and send order, and the length.
    #[test]
    fn a_candidate_takes_24_bytes() {
        assert_eq!(size_of::<SendCandidate>(), 24);
    }
}
