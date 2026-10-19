use bevy::prelude::*;
use campfire_net_connection::Connected;
use campfire_net_messages::{MessageReceiver, MessageSender};

use crate::scenario::guarantee::Guarantee;
use campfire_net_messages::internals::SeqMessage;
use campfire_net_transport::internals::{
    Channel1, ChannelOrderedReliable, ChannelSequencedReliable, ChannelSequencedUnreliable,
    ChannelUnorderedReliable,
};

/// The guarantee of each channel the scenarios send on, by the `channel` index of a
/// [`SeqMessage`].
pub(crate) const CHANNELS: [(&str, Guarantee); 5] = [
    ("UnorderedUnreliable", Guarantee::AtMostOnce),
    ("SequencedUnreliable", Guarantee::Increasing),
    ("UnorderedReliable", Guarantee::ExactlyOnce),
    ("SequencedReliable", Guarantee::IncreasingWithNewest),
    ("OrderedReliable", Guarantee::ExactlyOnceInOrder),
];

/// One end's numbered messages: while `sending`, one on each channel every frame, and what
/// arrived from the other end.
#[derive(Component, Debug, Default)]
pub(crate) struct MessageLog {
    pub(crate) sending: bool,
    pub(crate) sent: [Vec<u32>; 5],
    pub(crate) received: [Vec<u32>; 5],
}

impl MessageLog {
    pub(crate) fn send(
        mut ends: Query<'_, '_, (&mut Self, &mut MessageSender<SeqMessage>), With<Connected>>,
    ) {
        for (mut log, mut sender) in &mut ends {
            if !log.sending {
                continue;
            }
            for (channel, sent) in log.sent.iter_mut().enumerate() {
                let seq = u32::try_from(sent.len()).unwrap();
                let message = SeqMessage {
                    channel: u8::try_from(channel).unwrap(),
                    seq,
                };
                match channel {
                    0 => sender.send::<Channel1>(message),
                    1 => sender.send::<ChannelSequencedUnreliable>(message),
                    2 => sender.send::<ChannelUnorderedReliable>(message),
                    3 => sender.send::<ChannelSequencedReliable>(message),
                    4 => sender.send::<ChannelOrderedReliable>(message),
                    _ => unreachable!("CHANNELS holds five channels"),
                }
                sent.push(seq);
            }
        }
    }

    pub(crate) fn receive(mut ends: Query<'_, '_, (&mut Self, &mut MessageReceiver<SeqMessage>)>) {
        for (mut log, mut receiver) in &mut ends {
            for message in receiver.receive() {
                log.received[usize::from(message.channel)].push(message.seq);
            }
        }
    }

    /// Checks what `peer` received from this end against each channel's guarantee.
    pub(crate) fn check_sent_to(&self, peer: &Self, failures: &mut Vec<String>, label: &str) {
        for (channel, (name, guarantee)) in CHANNELS.iter().enumerate() {
            if let Err(failure) = guarantee.check(&self.sent[channel], &peer.received[channel]) {
                failures.push(format!("{label} {name}: {failure}"));
            }
        }
    }
}
