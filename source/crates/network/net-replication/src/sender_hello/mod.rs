use bevy_ecs::prelude::*;
use campfire_net_core::PositiveTickSpan;
use serde::{Deserialize, Serialize};

/// Event emitted when the receiver learns the sender's replication interval.
///
/// This is sent as part of the initial handshake so the receiver can align
/// its interpolation and timeline logic with the sender's send rate.
#[derive(Event, Debug, Serialize, Deserialize)]
pub struct SenderHello {
    /// How often the sender emits replication updates.
    pub send_interval: PositiveTickSpan,
    /// The link entity that sent this hello.
    pub link: Entity,
}

#[cfg(test)]
mod tests;
