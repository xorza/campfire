use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Local, SystemParam};
use campfire_net_messages::MultiMessageSender;
use campfire_protocol::Applied;

use crate::net_protocol::MatchChannel;
use campfire_host::InputAck;

/// What a server acknowledges each player's inputs through: one ack, which each acknowledgment
/// fills again, sent at once to its link, so an ack allocates nothing once its buffer has grown to
/// the longest input message.
#[derive(SystemParam, Debug)]
pub(crate) struct AckSender<'w, 's> {
    sender: MultiMessageSender<'w, 's>,
    ack: Local<'s, InputAck>,
}

impl AckSender<'_, '_> {
    /// Acknowledges to `link` the inputs from seq `first` on, as the log placed them.
    pub(crate) fn send(&mut self, link: Entity, first: u64, applied: &[Applied]) {
        self.ack.fill(first, applied);
        self.sender
            .send::<InputAck, MatchChannel>(&self.ack, [link])
            .expect("an ack is no longer than the input message it acknowledges");
    }
}
