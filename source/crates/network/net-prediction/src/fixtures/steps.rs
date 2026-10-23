//! Rollback steps that the prediction tests share.

use bevy_ecs::world::World;
use bevy_replicon::client::server_mutate_ticks::ServerMutateTicks;
use bevy_replicon::prelude::RepliconTick;
use campfire_net_core::NetTick;
use campfire_net_replication::ConfirmedTickMap;
use campfire_net_stepper::Stepper;
use campfire_net_timeline::Rollback;

use crate::fixtures::prediction_stack::PredictionStack;
use crate::rollback_check_state::RollbackCheckState;

pub(crate) fn record_completed_mutate_tick_for_rollback_check(world: &mut World, tick: NetTick) {
    let replicon_tick = RepliconTick::new(tick.0);
    // The server may have completed the tick already: a second confirmation of it is more
    // messages than the tick had.
    let mut mutate_ticks = world.resource_mut::<ServerMutateTicks>();
    if !mutate_ticks.contains(replicon_tick) {
        mutate_ticks.confirm(replicon_tick, 1);
    }
    let mut confirmed_ticks = world.resource_mut::<ConfirmedTickMap>();
    confirmed_ticks.bridge_record(replicon_tick, tick);
    confirmed_ticks.bridge_record_last_confirmed_tick(replicon_tick);
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
