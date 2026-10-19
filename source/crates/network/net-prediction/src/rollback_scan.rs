use bevy_ecs::prelude::*;
use campfire_net_core::NetTick;
use tracing::{trace, warn};

use crate::rollback_check_state::RollbackKind;

/// What the stages of one frame's rollback check share, as each predicted component's survey and
/// scan is its own system: the checkpoint they look at, what they report, and the rollback the
/// frame asks for.
#[derive(Resource, Debug, Default)]
pub(crate) struct RollbackScan {
    /// The local tick, once the timeline is synced: no rollback is asked for before.
    pub(crate) tick: Option<NetTick>,
    /// The newest completed checkpoint at or before the local tick.
    pub(crate) candidate: Option<NetTick>,
    /// Whether this frame's policy looks at the candidate, so the surveys report on it.
    pub(crate) surveys: bool,
    /// Whether the policy needs to know that any entity is a check target.
    pub(crate) counts_targets: bool,
    /// Whether any entity is a rollback-check target.
    pub(crate) has_check_entities: bool,
    /// The earliest unresolved predicted diff at or before the candidate.
    pub(crate) earliest_pending: Option<NetTick>,
    /// The completed checkpoint the components compare at, when the policy asks for it.
    pub(crate) scan_tick: Option<NetTick>,
    /// The first entity found mispredicted at the scan tick.
    pub(crate) mismatch: Option<Entity>,
    /// The rollback this frame asks for, and what it is for.
    pub(crate) requested: Option<NetTick>,
    pub(crate) kind: RollbackKind,
}

impl RollbackScan {
    /// The candidate checkpoint, when the surveys report on it.
    pub(crate) const fn surveyed_candidate(&self) -> Option<NetTick> {
        if self.surveys { self.candidate } else { None }
    }

    /// Keeps the earliest of the pending diff ticks reported.
    pub(crate) fn report_pending(&mut self, tick: NetTick) {
        self.earliest_pending = Some(
            self.earliest_pending
                .map_or(tick, |earliest| earliest.min(tick)),
        );
    }

    /// Asks for a rollback to `rollback_tick`, for `kind`, when it is at most `max_rollback_ticks`
    /// before the local tick and not after it; one out of those bounds asks for none.
    pub(crate) fn request(
        &mut self,
        rollback_tick: NetTick,
        max_rollback_ticks: u16,
        kind: RollbackKind,
    ) {
        let tick = self
            .tick
            .expect("a rollback is asked for once the timeline is synced");
        let delta = tick - rollback_tick;
        if delta < 0 || delta > i32::from(max_rollback_ticks) {
            warn!(
                ?rollback_tick,
                ?tick,
                "Trying to do a rollback of {delta:?} ticks. The max is {max_rollback_ticks:?}! Aborting"
            );
            trace!(
                target: "campfire_net_debug::prediction",
                kind = "rollback_rejected",
                schedule = "PreUpdate",
                sample_point = "PreUpdate",
                local_tick = tick.0,
                rollback_tick = rollback_tick.0,
                rollback_delta = delta,
                max_rollback_ticks,
                "rollback request rejected"
            );
            self.requested = None;
            return;
        }
        self.requested = Some(rollback_tick);
        self.kind = kind;
        trace!(
            target: "campfire_net_debug::prediction",
            kind = "rollback_requested",
            schedule = "PreUpdate",
            sample_point = "PreUpdate",
            local_tick = tick.0,
            rollback_tick = rollback_tick.0,
            rollback_delta = delta,
            "rollback requested"
        );
    }
}
