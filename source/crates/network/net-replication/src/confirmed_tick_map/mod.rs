use crate::authoritative_tick::decode_authoritative_tick;
use crate::confirmed_tick::ConfirmedTick;
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

/// How many confirmed ticks the map keeps: the window of recent rollback and prediction.
pub(crate) const MAX_CONFIRMED_TICKS: usize = 256;

/// Receiver-side translation table from Replicon confirmed tick ticks to authoritative server
/// ticks.
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
pub struct ConfirmedTickMap {
    /// Each retained confirmed tick and its authoritative tick, sorted by Replicon tick, oldest
    /// first: the mutations arrive in any order, and only the newest window is kept.
    entries: VecDeque<(RepliconTick, NetTick)>,
    last_confirmed: Option<ConfirmedTick>,
    faults: ConfirmedTickFaults,
    /// [`ConfirmedTickFaults::unmapped_messages`], counted by a receive that reads the map shared,
    /// as a replicated entity's write gives it the world only to read.
    unmapped_messages: AtomicU64,
}

impl Default for ConfirmedTickMap {
    fn default() -> Self {
        Self {
            entries: VecDeque::with_capacity(MAX_CONFIRMED_TICKS),
            last_confirmed: None,
            faults: ConfirmedTickFaults::default(),
            unmapped_messages: AtomicU64::new(0),
        }
    }
}

/// How often the confirmed ticks a peer sent could not be used, over the app's life: a peer's data
/// is not trusted, so each is logged and counted, never asserted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ConfirmedTickFaults {
    /// Authoritative tick userdata that did not decode to a tick.
    pub invalid_userdata: u64,
    /// Completed mutate ticks that no recorded confirmed tick maps.
    pub unmapped_confirms: u64,
    /// Received message ticks that no recorded confirmed tick maps.
    pub unmapped_messages: u64,
}

impl ConfirmedTickMap {
    /// Record the authoritative server simulation tick for a Replicon confirmed tick.
    ///
    /// Multiple Replicon ticks may legitimately map to the same authoritative [`NetTick`]. That
    /// happens when several replication sends are produced for the same simulation step.
    ///
    /// A confirmed tick older than every retained one, once the map is full, is not kept: the
    /// window holds the newest confirmed ticks.
    pub(crate) fn record(&mut self, replicon_tick: RepliconTick, authoritative_tick: NetTick) {
        let mut index = match self.search(replicon_tick) {
            Ok(index) => {
                self.entries[index].1 = authoritative_tick;
                return;
            }
            Err(index) => index,
        };
        if self.entries.len() == MAX_CONFIRMED_TICKS {
            if index == 0 {
                return;
            }
            self.entries.pop_front();
            index -= 1;
        }
        self.entries
            .insert(index, (replicon_tick, authoritative_tick));
    }

    /// The authoritative tick a retained confirmed tick maps to.
    pub(crate) fn get(&self, replicon_tick: RepliconTick) -> Option<NetTick> {
        self.search(replicon_tick)
            .ok()
            .map(|index| self.entries[index].1)
    }

    /// The authoritative tick of a received message's confirmed tick: the tick to record its values
    /// at. A message that no retained confirmed tick maps is counted as a fault, as the mapping
    /// comes from the peer.
    pub fn message_tick(&self, replicon_tick: RepliconTick) -> Option<NetTick> {
        let tick = self.get(replicon_tick);
        if tick.is_none() {
            self.unmapped_messages.fetch_add(1, Ordering::Relaxed);
        }
        tick
    }

    /// The wrapping order is transitive within `RepliconTick::MAX_NEWER_DISTANCE`, far more
    /// ticks than the window of recent confirmed ticks spans.
    fn search(&self, replicon_tick: RepliconTick) -> Result<usize, usize> {
        self.entries
            .binary_search_by(|&(retained, _)| retained.wrapping_cmp(replicon_tick))
    }

    /// An authoritative tick at which the state an entity's latest confirmation carried held: the
    /// confirmation's own tick, or, for a confirmation older than every retained confirmed tick,
    /// the oldest retained tick, since nothing for the entity arrived through every later confirmed
    /// tick. `None` when no confirmed tick maps a confirmation within the retained range.
    pub fn confirmed_state_tick(&self, confirm_history: &ConfirmHistory) -> Option<NetTick> {
        let replicon_tick = confirm_history.last_tick();
        match self.search(replicon_tick) {
            Ok(index) => Some(self.entries[index].1),
            Err(0) => self.entries.front().map(|&(_, tick)| tick),
            Err(_) => None,
        }
    }

    /// The authoritative tick of the newest message the client received, its updates' or its
    /// mutations', complete or not: the tick of the newest state a watching client holds and
    /// draws, which a paused server's first update alone gives, as no mutation follows it.
    pub fn newest_tick(&self) -> Option<NetTick> {
        self.entries.back().map(|&(_, tick)| tick)
    }

    /// Latest authoritative network tick for which Replicon completed all mutate messages.
    pub fn last_confirmed_tick(&self) -> Option<NetTick> {
        self.last_confirmed
            .map(|confirmed_tick| confirmed_tick.tick)
    }

    /// Latest confirmed tick for which Replicon completed all mutate messages.
    pub const fn last_confirmed(&self) -> Option<ConfirmedTick> {
        self.last_confirmed
    }

    /// The newest completed confirmed tick that is not ahead of `limit`.
    ///
    /// The common case uses the cached latest completed confirmed tick. When that confirmed tick is
    /// ahead of the local timeline, inspect the retained Replicon-to-network mappings and keep only
    /// ticks that [`ServerMutateTicks`] reports as complete. Multiple Replicon ticks can map to the
    /// same network tick; ties prefer the most recent Replicon tick.
    pub fn latest_completed_at_or_before(
        &self,
        server_mutate_ticks: &ServerMutateTicks,
        limit: NetTick,
    ) -> Option<ConfirmedTick> {
        if let Some(confirmed_tick) = self.last_confirmed
            && confirmed_tick.tick <= limit
        {
            return Some(confirmed_tick);
        }

        self.latest_completed_matching(server_mutate_ticks, |tick| tick <= limit)
    }

    /// The newest completed confirmed tick strictly before `limit`.
    ///
    /// This is used when an unresolved diff at `limit` makes that tick and every later tick
    /// unusable as a rollback confirmed tick. The returned confirmed tick is selected only from
    /// mappings that [`ServerMutateTicks`] reports as globally complete.
    pub fn latest_completed_before(
        &self,
        server_mutate_ticks: &ServerMutateTicks,
        limit: NetTick,
    ) -> Option<ConfirmedTick> {
        if let Some(confirmed_tick) = self.last_confirmed
            && confirmed_tick.tick < limit
        {
            return Some(confirmed_tick);
        }

        self.latest_completed_matching(server_mutate_ticks, |tick| tick < limit)
    }

    fn latest_completed_matching(
        &self,
        server_mutate_ticks: &ServerMutateTicks,
        is_within_bound: impl Fn(NetTick) -> bool,
    ) -> Option<ConfirmedTick> {
        let mut best = None;
        for &(replicon_tick, tick) in &self.entries {
            if !server_mutate_ticks.contains(replicon_tick) || !is_within_bound(tick) {
                continue;
            }
            let candidate = ConfirmedTick {
                replicon_tick,
                tick,
            };
            if best.is_none_or(|existing| candidate.is_newer_than(existing)) {
                best = Some(candidate);
            }
        }
        best
    }

    /// Resolve a completed Replicon mutate confirmed tick and update the latest confirmed confirmed
    /// tick.
    ///
    /// A confirmed tick advances the cache when its network tick is newer. If multiple Replicon
    /// confirmed ticks map to the same network tick, the newer Replicon confirmed tick wins.
    /// The resolved confirmed tick, even when an already-cached confirmed tick remains
    /// newer, or `None` if the mapping has not been received yet.
    pub(crate) fn record_last_confirmed_tick(
        &mut self,
        replicon_tick: RepliconTick,
    ) -> Option<ConfirmedTick> {
        let Some(tick) = self.get(replicon_tick) else {
            self.faults.unmapped_confirms += 1;
            return None;
        };
        let confirmed_tick = ConfirmedTick {
            replicon_tick,
            tick,
        };
        if self
            .last_confirmed
            .is_none_or(|existing| confirmed_tick.is_newer_than(existing))
        {
            self.last_confirmed = Some(confirmed_tick);
        }
        Some(confirmed_tick)
    }

    /// Forgets the link's confirmed ticks; the fault counts last.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.last_confirmed = None;
    }

    pub fn faults(&self) -> ConfirmedTickFaults {
        ConfirmedTickFaults {
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
        mut confirmed_ticks: ResMut<'_, Self>,
    ) {
        match decode_authoritative_tick(received.bytes.as_ref()) {
            Ok(authoritative_tick) => {
                confirmed_ticks.record(received.message_tick, authoritative_tick);
            }
            Err(error_kind) => {
                error!(
                    ?error_kind,
                    replicon_tick = ?received.message_tick,
                    userdata_len = received.bytes.len(),
                    "dropping invalid authoritative tick userdata"
                );
                confirmed_ticks.count_invalid_userdata();
            }
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_replicon::prelude::RepliconTick;
    use campfire_net_core::NetTick;

    use crate::confirmed_tick::ConfirmedTick;
    use crate::confirmed_tick_map::ConfirmedTickMap;

    /// The bridge's writes, for another crate's test that stands in for it.
    impl ConfirmedTickMap {
        /// Maps `replicon_tick` to `authoritative_tick`, as the bridge's `record` does.
        pub fn bridge_record(&mut self, replicon_tick: RepliconTick, authoritative_tick: NetTick) {
            self.record(replicon_tick, authoritative_tick);
        }

        /// Completes `replicon_tick`, as the bridge's `record_last_confirmed_tick` does.
        pub fn bridge_record_last_confirmed_tick(
            &mut self,
            replicon_tick: RepliconTick,
        ) -> Option<ConfirmedTick> {
            self.record_last_confirmed_tick(replicon_tick)
        }
    }
}

#[cfg(test)]
mod tests;
