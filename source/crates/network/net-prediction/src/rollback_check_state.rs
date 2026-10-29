use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use campfire_net_core::NetTick;
use campfire_net_timeline::LocalTimelineShift;

use crate::prediction_despawned::PredictionDespawned;

/// What a rollback is for, which sets what it does to the locally predicted entities a rollback
/// would otherwise despawn or protect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Reflect)]
pub(crate) enum RollbackKind {
    /// A mismatch, or the policy's rollback on each new confirmed state.
    #[default]
    Policy,
    /// A timeline switch: a replay from confirmed data that keeps spawn protections.
    Switch,
    /// An app's one-shot reconcile, as a late-join snapshot: it ends every spawn protection.
    Snapshot,
}

/// The state the rollback check keeps from one frame to the next.
#[derive(Resource, Clone, Debug, Default, Reflect)]
pub struct RollbackCheckState {
    /// Latest completed server mutate tick consumed by the rollback check.
    ///
    /// Replicon advances `ServerMutateTicks` during receive; this advances only after
    /// `RollbackPlugin::check_rollback` handles that frontier. We retain it separately to:
    /// - avoid rescanning entities while the completed frontier is unchanged;
    /// - provide a safe watermark for pruning diff history.
    ///
    /// This tick is always in the past or present relative to the local simulation tick when it is
    /// stored. It can lag behind Replicon's globally latest completed confirmed tick when that
    /// confirmed tick is still in the local simulation's future, or when an unresolved predicted
    /// diff makes a newer confirmed tick unsafe to process.
    last_processed_confirmed_tick: Option<NetTick>,

    /// Set to true if we received any replication message this frame.
    /// Used to trigger `RollbackMode::Always`.
    pub(crate) received_messages_this_frame: bool,

    /// NetTick at which an external caller has requested a one-shot rollback.
    ///
    /// Consumed by `RollbackPlugin::check_rollback` regardless of the `rollback_setup.mode`
    /// setting — this is an explicit request, not a mismatch-triggered one.
    /// Set via [`RollbackCheckState::request_forced_rollback`]. Cleared
    /// when consumed.
    pub(crate) forced_rollback_tick: Option<NetTick>,

    /// What the requested one-shot rollback is for, when one is requested.
    pub(crate) forced_rollback_kind: RollbackKind,

    /// What the current rollback is for.
    pub(crate) rollback_kind: RollbackKind,

    /// Locally predicted entities a rollback despawns when they spawned after its tick, with
    /// their spawn tick, in tick order.
    pub(crate) local_despawn: Vec<(NetTick, Entity)>,

    /// Locally predicted entities protected from rollbacks until their tick, in tick order.
    pub(crate) local_skip_despawn: Vec<(NetTick, Entity)>,
}

impl RollbackCheckState {
    /// Request a one-shot rollback from `tick`, regardless of the
    /// `rollback_setup.mode` mode.
    ///
    /// Intended for scenarios where an external system (e.g. a snapshot an app
    /// applies to a late-joining client) has deposited confirmed state at a specific tick and
    /// needs the simulation to re-run from there. Subsequent calls within the
    /// same frame take the earliest tick.
    ///
    /// It ends every spawn protection of `LocallyPredicted`:
    /// the deposited state is authoritative for every entity.
    pub fn request_forced_rollback(&mut self, tick: NetTick) {
        self.request_rollback(tick, RollbackKind::Snapshot);
    }

    /// Requests the one-shot rollback of a timeline switch from `tick`, which keeps spawn
    /// protections.
    pub(crate) fn request_switch_rollback(&mut self, tick: NetTick) {
        self.request_rollback(tick, RollbackKind::Switch);
    }

    /// The earliest tick requested this frame wins, and a snapshot's kind outranks a switch's.
    fn request_rollback(&mut self, tick: NetTick, kind: RollbackKind) {
        if self
            .forced_rollback_tick
            .is_none_or(|existing| tick < existing)
        {
            self.forced_rollback_tick = Some(tick);
        }
        self.forced_rollback_kind = self.forced_rollback_kind.max(kind);
    }

    /// NetTick at which a one-shot rollback has been requested but not yet
    /// consumed by `RollbackPlugin::check_rollback`. While this is `Some`, prediction
    /// history buffers on the entities targeted by the rollback must not
    /// be mutated (e.g. checksum systems that use destructive reads must
    /// skip), otherwise `RollbackPlugin::prepare_rollback` won't find the restore value.
    pub const fn forced_rollback_tick(&self) -> Option<NetTick> {
        self.forced_rollback_tick
    }

    /// Moves the ticks of the locally predicted entities, which are the local timeline's, by the
    /// shift's whole ticks, and the tick of each prediction despawn with them, as the prediction
    /// histories move (`LocalTimelineShift`'s contract), so a rollback compares ticks of one
    /// labelling.
    pub(crate) fn shift_with_local_timeline(
        trigger: On<'_, '_, LocalTimelineShift>,
        mut state: ResMut<'_, Self>,
        mut despawned: Query<'_, '_, &mut PredictionDespawned>,
    ) {
        let state = &mut *state;
        let listed = state.local_despawn.iter_mut();
        for (tick, _) in listed.chain(state.local_skip_despawn.iter_mut()) {
            *tick = *tick + trigger.delta;
        }
        for mut despawned in &mut despawned {
            despawned.tick = despawned.tick + trigger.delta;
        }
    }

    /// Reset all connection-scoped rollback check state. The locally predicted entities are this
    /// app's own, and stay.
    pub(crate) fn reset_connection_state(&mut self) {
        self.last_processed_confirmed_tick = None;
        self.received_messages_this_frame = false;
        self.forced_rollback_tick = None;
        self.forced_rollback_kind = RollbackKind::default();
    }

    /// Reset the per-frame state tracking.
    pub(crate) const fn reset_frame_state(&mut self) {
        self.received_messages_this_frame = false;
    }

    /// The latest completed server mutate tick consumed by rollback checking.
    ///
    /// During Replicon's receive systems this still reflects the previous rollback-check frontier;
    /// it is advanced only after `RollbackPlugin::check_rollback` handles a completed confirmed
    /// tick that is at or before the local simulation tick. It is used to avoid rescanning an
    /// already-consumed completed tick. This is a processing watermark, not the target of a
    /// rollback currently in progress; that target is the
    /// [`Rollback`](campfire_net_timeline::Rollback) resource's tick.
    pub const fn last_processed_confirmed_tick(&self) -> Option<NetTick> {
        self.last_processed_confirmed_tick
    }

    /// Record that rollback checking consumed this completed server mutate tick.
    ///
    /// Call this only after `RollbackPlugin::check_rollback` consumes the confirmed tick: after the
    /// full state scan in `RollbackMode::Check`, or after scheduling its unconditional rollback in
    /// `RollbackMode::Always`. Do not advance it directly from `ServerMutateTicks`, while the
    /// completed tick is still in the client's future, or while a diff at or before this confirmed
    /// tick is unresolved.
    pub(crate) const fn set_last_processed_confirmed_tick(&mut self, tick: NetTick) {
        self.last_processed_confirmed_tick = Some(tick);
    }

    /// Check if the completed mutate tick has advanced since we last processed it.
    ///
    /// If this returns false, `RollbackPlugin::check_rollback` can skip the full state scan because
    /// the current completed confirmed tick was already handled on an earlier frame.
    pub(crate) fn has_confirmed_tick_advanced(&self, current_tick: NetTick) -> bool {
        match self.last_processed_confirmed_tick {
            None => true, // First time, always process
            Some(last) => current_tick > last,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The earliest requested tick wins, and a snapshot's kind outranks a switch's in either
    /// order.
    #[test]
    fn requests_merge_to_the_earliest_tick_and_the_strongest_kind() {
        let mut state = RollbackCheckState::default();
        state.request_switch_rollback(NetTick(10));
        state.request_forced_rollback(NetTick(12));
        assert_eq!(state.forced_rollback_tick(), Some(NetTick(10)));
        assert_eq!(state.forced_rollback_kind, RollbackKind::Snapshot);

        let mut state = RollbackCheckState::default();
        state.request_switch_rollback(NetTick(10));
        assert_eq!(state.forced_rollback_kind, RollbackKind::Switch);
    }
}
