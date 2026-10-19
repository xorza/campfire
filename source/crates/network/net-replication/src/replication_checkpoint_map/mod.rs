use crate::checkpoint::decode_authoritative_tick;
use crate::completed_checkpoint::CompletedCheckpoint;
use bevy_ecs::prelude::On;
use bevy_ecs::prelude::ResMut;
use bevy_ecs::prelude::Resource;
use bevy_replicon::client::UserdataReceived;
use bevy_replicon::client::confirm_history::ConfirmHistory;
use bevy_replicon::client::server_mutate_ticks::ServerMutateTicks;
use bevy_replicon::prelude::RepliconTick;
use campfire_net_core::NetTick;
use core::sync::atomic::{AtomicU64, Ordering};
use std::collections::VecDeque;
use tracing::error;

/// How many checkpoints the map keeps: the window of recent rollback and prediction.
pub(crate) const MAX_STORED_CHECKPOINTS: usize = 256;

/// Receiver-side translation table from Replicon checkpoint ticks to authoritative server ticks.
///
/// This resource is populated when Replicon triggers
/// [`UserdataReceived`](bevy_replicon::client::UserdataReceived). For each payload on Replicon's
/// update/mutation channels, the bridge:
///
/// 1. reads the [`NetTick`] from the Replicon userdata bytes
/// 2. uses Replicon's `message_tick` as the corresponding [`RepliconTick`]
/// 3. records the association here before Replicon applies the message
///
/// Prediction and rollback code should use this resource whenever they need to interpret
/// [`ConfirmHistory`](bevy_replicon::client::confirm_history::ConfirmHistory) or
/// [`ServerMutateTicks`] in
/// simulation time.
///
/// This mapping is intentionally bounded because it is only needed for recent rollback /
/// prediction windows.
#[derive(Resource, Debug)]
pub struct ReplicationCheckpointMap {
    /// Each retained checkpoint and its authoritative tick, sorted by Replicon tick, oldest
    /// first: the mutations arrive in any order, and only the newest window is kept.
    entries: VecDeque<(RepliconTick, NetTick)>,
    last_confirmed_checkpoint: Option<CompletedCheckpoint>,
    faults: CheckpointFaults,
    /// [`CheckpointFaults::unmapped_messages`], counted by a receive that reads the map shared,
    /// as a replicated entity's write gives it the world only to read.
    unmapped_messages: AtomicU64,
}

impl Default for ReplicationCheckpointMap {
    fn default() -> Self {
        Self {
            entries: VecDeque::with_capacity(MAX_STORED_CHECKPOINTS),
            last_confirmed_checkpoint: None,
            faults: CheckpointFaults::default(),
            unmapped_messages: AtomicU64::new(0),
        }
    }
}

/// How often the checkpoints a peer sent could not be used, over the app's life: a peer's data
/// is not trusted, so each is logged and counted, never asserted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointFaults {
    /// Checkpoint userdata that did not decode to a tick.
    pub invalid_userdata: u64,
    /// Completed mutate ticks that no recorded checkpoint maps.
    pub unmapped_confirms: u64,
    /// Received message ticks that no recorded checkpoint maps.
    pub unmapped_messages: u64,
}

impl ReplicationCheckpointMap {
    /// Record the authoritative server simulation tick for a Replicon checkpoint.
    ///
    /// Multiple Replicon ticks may legitimately map to the same authoritative [`NetTick`]. That
    /// happens when several replication sends are produced for the same simulation step.
    ///
    /// A checkpoint older than every retained one, once the map is full, is not kept: the window
    /// holds the newest checkpoints.
    pub fn record(&mut self, replicon_tick: RepliconTick, authoritative_tick: NetTick) {
        let mut index = match self.search(replicon_tick) {
            Ok(index) => {
                self.entries[index].1 = authoritative_tick;
                return;
            }
            Err(index) => index,
        };
        if self.entries.len() == MAX_STORED_CHECKPOINTS {
            if index == 0 {
                return;
            }
            self.entries.pop_front();
            index -= 1;
        }
        self.entries
            .insert(index, (replicon_tick, authoritative_tick));
    }

    /// The authoritative tick a retained checkpoint maps to.
    pub(crate) fn get(&self, replicon_tick: RepliconTick) -> Option<NetTick> {
        self.search(replicon_tick)
            .ok()
            .map(|index| self.entries[index].1)
    }

    /// The authoritative tick of a received message's checkpoint: the tick to record its values
    /// at. A message that no retained checkpoint maps is counted as a fault, as the mapping comes
    /// from the peer.
    pub fn message_tick(&self, replicon_tick: RepliconTick) -> Option<NetTick> {
        let tick = self.get(replicon_tick);
        if tick.is_none() {
            self.unmapped_messages.fetch_add(1, Ordering::Relaxed);
        }
        tick
    }

    /// The wrapping order is transitive within `RepliconTick::MAX_NEWER_DISTANCE`, far more
    /// ticks than the window of recent checkpoints spans.
    fn search(&self, replicon_tick: RepliconTick) -> Result<usize, usize> {
        self.entries
            .binary_search_by(|&(retained, _)| retained.wrapping_cmp(replicon_tick))
    }

    /// An authoritative tick at which the state an entity's latest confirmation carried held: the
    /// confirmation's own tick, or, for a confirmation older than every retained checkpoint, the
    /// oldest retained tick, since nothing for the entity arrived through every later checkpoint.
    /// `None` when no checkpoint maps a confirmation within the retained range.
    pub fn confirmed_state_tick(&self, confirm_history: &ConfirmHistory) -> Option<NetTick> {
        let replicon_tick = confirm_history.last_tick();
        match self.search(replicon_tick) {
            Ok(index) => Some(self.entries[index].1),
            Err(0) => self.entries.front().map(|&(_, tick)| tick),
            Err(_) => None,
        }
    }

    /// Latest authoritative network tick for which Replicon completed all mutate messages.
    pub fn last_confirmed_tick(&self) -> Option<NetTick> {
        self.last_confirmed_checkpoint
            .map(|checkpoint| checkpoint.tick)
    }

    /// Latest checkpoint for which Replicon completed all mutate messages.
    pub const fn last_confirmed_checkpoint(&self) -> Option<CompletedCheckpoint> {
        self.last_confirmed_checkpoint
    }

    /// Return the newest completed checkpoint that is not ahead of `limit`.
    ///
    /// The common case uses the cached latest completed checkpoint. When that checkpoint is ahead
    /// of the local timeline, inspect the retained Replicon-to-network mappings and keep only
    /// ticks that [`ServerMutateTicks`] reports as complete. Multiple Replicon ticks can map to the
    /// same network tick; ties prefer the most recent Replicon tick.
    pub fn latest_completed_at_or_before(
        &self,
        server_mutate_ticks: &ServerMutateTicks,
        limit: NetTick,
    ) -> Option<CompletedCheckpoint> {
        if let Some(checkpoint) = self.last_confirmed_checkpoint
            && checkpoint.tick <= limit
        {
            return Some(checkpoint);
        }

        self.latest_completed_matching(server_mutate_ticks, |tick| tick <= limit)
    }

    /// Return the newest completed checkpoint strictly before `limit`.
    ///
    /// This is used when an unresolved diff at `limit` makes that tick and every later tick
    /// unusable as a rollback checkpoint. The returned checkpoint is selected only from mappings
    /// that [`ServerMutateTicks`] reports as globally complete.
    pub fn latest_completed_before(
        &self,
        server_mutate_ticks: &ServerMutateTicks,
        limit: NetTick,
    ) -> Option<CompletedCheckpoint> {
        if let Some(checkpoint) = self.last_confirmed_checkpoint
            && checkpoint.tick < limit
        {
            return Some(checkpoint);
        }

        self.latest_completed_matching(server_mutate_ticks, |tick| tick < limit)
    }

    fn latest_completed_matching(
        &self,
        server_mutate_ticks: &ServerMutateTicks,
        is_within_bound: impl Fn(NetTick) -> bool,
    ) -> Option<CompletedCheckpoint> {
        let mut best = None;
        for &(replicon_tick, tick) in &self.entries {
            if !server_mutate_ticks.contains(replicon_tick) || !is_within_bound(tick) {
                continue;
            }
            let candidate = CompletedCheckpoint {
                replicon_tick,
                tick,
            };
            if best.is_none_or(|existing: CompletedCheckpoint| {
                candidate.tick > existing.tick
                    || (candidate.tick == existing.tick
                        && replicon_tick.is_newer(existing.replicon_tick))
            }) {
                best = Some(candidate);
            }
        }
        best
    }

    /// Resolve a completed Replicon mutate checkpoint and update the latest confirmed checkpoint.
    ///
    /// A checkpoint advances the cache when its network tick is newer. If multiple Replicon
    /// checkpoints map to the same network tick, the newer Replicon checkpoint wins. Returns the
    /// resolved checkpoint, even when an already-cached checkpoint remains newer, or `None` if the
    /// mapping has not been received yet.
    pub fn record_last_confirmed_checkpoint(
        &mut self,
        replicon_tick: RepliconTick,
    ) -> Option<CompletedCheckpoint> {
        let Some(tick) = self.get(replicon_tick) else {
            self.faults.unmapped_confirms += 1;
            return None;
        };
        let checkpoint = CompletedCheckpoint {
            replicon_tick,
            tick,
        };
        if self.last_confirmed_checkpoint.is_none_or(|existing| {
            checkpoint.tick > existing.tick
                || (checkpoint.tick == existing.tick
                    && checkpoint.replicon_tick.is_newer(existing.replicon_tick))
        }) {
            self.last_confirmed_checkpoint = Some(checkpoint);
        }
        Some(checkpoint)
    }

    /// Forgets the session's checkpoints; the fault counts last.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.last_confirmed_checkpoint = None;
    }

    pub fn faults(&self) -> CheckpointFaults {
        CheckpointFaults {
            unmapped_messages: self.unmapped_messages.load(Ordering::Relaxed),
            ..self.faults
        }
    }

    pub(crate) const fn count_invalid_userdata(&mut self) {
        self.faults.invalid_userdata += 1;
    }

    /// Record the authoritative network tick carried by Replicon userdata.
    pub(crate) fn record_authoritative_tick(
        received: On<'_, '_, UserdataReceived>,
        mut checkpoints: ResMut<'_, Self>,
    ) {
        match decode_authoritative_tick(received.bytes.as_ref()) {
            Ok(authoritative_tick) => checkpoints.record(received.message_tick, authoritative_tick),
            Err(error_kind) => {
                error!(
                    ?error_kind,
                    replicon_tick = ?received.message_tick,
                    userdata_len = received.bytes.len(),
                    "dropping invalid replicon checkpoint userdata"
                );
                checkpoints.count_invalid_userdata();
            }
        }
    }
}

#[cfg(test)]
mod tests;
