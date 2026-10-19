//! Maps Replicon delivery checkpoints onto network time.
//!
//! # Why this exists
//! Replicon and the stack use different notions of "tick":
//!
//! - [`RepliconTick`] is a replication checkpoint / message-ordering index. It is owned by
//!   Replicon and is what [`ConfirmHistory`] and [`ServerMutateTicks`] report.
//!   [`RepliconTick`] is incremented by one whenever the 'send' side of replication runs.
//! - [`NetTick`] is the stack's authoritative simulation tick. Prediction history, rollback,
//!   timeline resync, and mismatch detection must operate in this tick domain.
//!   [`NetTick`] is incremented by one whenever the FixedUpdate schedule runs.
//!
//! The bridge therefore carries an explicit mapping from each received Replicon checkpoint to the
//! authoritative server simulation.
//!
//! # How the mapping is carried
//! The stack stores the authoritative [`NetTick`] in Replicon's replication userdata:
//!
//! 1. On the server, `ServerRepliconPlugin::write_authoritative_tick_userdata` writes the
//!    current network tick into Replicon's `ReplicationUserdata` before Replicon builds
//!    replication messages.
//! 2. Replicon serializes those bytes into its update and mutation messages.
//! 3. On the client, Replicon triggers `UserdataReceived` before applying the message, and
//!    `ReplicationCheckpointMap::record_authoritative_tick` stores the `RepliconTick -> NetTick`
//!    association.
//!
//! # Which tick to use
//! - Use [`RepliconTick`] for Replicon protocol concerns only: packet ordering, completeness, and
//!   decoding [`ConfirmHistory`] / [`ServerMutateTicks`].
//! - Use [`NetTick`] for prediction history, rollback decisions, mismatch comparisons, and
//!   anything that interacts with the local timeline or timeline resync.
//! - When code receives a tick from Replicon state, resolve it through
//!   [`ReplicationCheckpointMap`] before using it in prediction or rollback logic.
//!
//! [`ConfirmHistory`]: bevy_replicon::client::confirm_history::ConfirmHistory
//! [`ServerMutateTicks`]: bevy_replicon::client::server_mutate_ticks::ServerMutateTicks
//! [`RepliconTick`]: bevy_replicon::prelude::RepliconTick
//! [`ReplicationCheckpointMap`]: crate::ReplicationCheckpointMap

use crate::checkpoint_error::CheckpointError;
use campfire_net_core::NetTick;

pub(crate) const CHECKPOINT_USERDATA_LEN: usize = size_of::<NetTick>();

pub(crate) const fn encode_authoritative_tick(
    authoritative_tick: NetTick,
) -> [u8; CHECKPOINT_USERDATA_LEN] {
    authoritative_tick.0.to_le_bytes()
}

pub(crate) const fn decode_authoritative_tick(bytes: &[u8]) -> Result<NetTick, CheckpointError> {
    if bytes.len() != CHECKPOINT_USERDATA_LEN {
        return Err(CheckpointError::InvalidUserdataLength(bytes.len()));
    }
    Ok(NetTick(u32::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3],
    ])))
}

#[cfg(test)]
mod tests;
