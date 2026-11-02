//! Rollback steps that the prediction tests share.

use bevy_ecs::prelude::{Res, ResMut, Resource};
use bevy_ecs::world::World;
use bevy_replicon::client::server_mutate_ticks::ServerMutateTicks;
use bevy_replicon::prelude::RepliconTick;
use campfire_net_core::NetTick;
use campfire_net_replication::ConfirmedTickMap;
use campfire_net_stepper::Stepper;
use campfire_net_timeline::Rollback;

use crate::fixtures::prediction_stack::PredictionStack;
use crate::rollback_check_state::RollbackCheckState;

/// Completes the server's mutate tick `replicon_tick`, which confirms `tick`, as the rollback
/// check reads it.
pub(crate) fn record_completed_mutate_tick(
    world: &mut World,
    replicon_tick: RepliconTick,
    tick: NetTick,
) {
    // The server may have completed the tick already: a second confirmation of it is more
    // messages than the tick had.
    let mut mutate_ticks = world.resource_mut::<ServerMutateTicks>();
    if !mutate_ticks.contains(replicon_tick) {
        mutate_ticks.confirm(replicon_tick, 1);
    }
    let mut confirmed_ticks = world.resource_mut::<ConfirmedTickMap>();
    confirmed_ticks.bridge_complete(replicon_tick, tick);
}

/// Completes the server's mutate tick of `tick`'s number, which confirms `tick`.
pub(crate) fn record_completed_mutate_tick_for_rollback_check(world: &mut World, tick: NetTick) {
    record_completed_mutate_tick(world, RepliconTick::new(tick.0), tick);
}

/// Records a completed confirmed tick and requests the one-shot rollback, for tests of rollback
/// restoration and replay rather than of mismatch detection.
pub(crate) fn request_forced_rollback_at_completed_tick(
    stepper: &mut Stepper<PredictionStack>,
    tick: NetTick,
) {
    let world = stepper.client_app_mut(0).world_mut();
    record_completed_mutate_tick_for_rollback_check(world, tick);
    world
        .resource_mut::<RollbackCheckState>()
        .request_forced_rollback(tick);
}

pub(crate) fn trigger_state_rollback(stepper: &mut Stepper<PredictionStack>, tick: NetTick) {
    stepper.client_app_mut(0).insert_resource(Rollback(tick));
}

/// Whether a frame ran a rollback, as `record_rollback` sees it.
#[derive(Debug, Resource, Default)]
pub(crate) struct RollbackObserved(pub(crate) bool);

/// Records whether a rollback is under way.
pub(crate) fn record_rollback(
    rollback: Option<Res<'_, Rollback>>,
    mut observed: ResMut<'_, RollbackObserved>,
) {
    observed.0 |= rollback.is_some();
}
