use crate::confirmed_tick_map::ConfirmedTickMap;
use crate::delayed_despawn::DelayedDespawn;
use crate::interpolated::Interpolated;
use crate::interpolated_despawn::InterpolatedDespawn;
use bevy_ecs::prelude::*;
use bevy_replicon::shared::replication::registry::ctx::DespawnCtx;
use campfire_net_despawn::DespawnReplicated;
use tracing::debug;

/// Replicon's despawn of a received entity. An interpolated entity, when interpolation delays
/// it, waits as [`DelayedDespawn`] for the interpolation timeline to reach the tick the server
/// despawned it at; one whose tick no confirmed tick maps is despawned at once.
pub(crate) fn receive(ctx: &DespawnCtx, mut entity: EntityWorldMut<'_>) {
    let delays = entity.contains::<Interpolated>()
        && entity
            .world()
            .get_resource::<InterpolatedDespawn>()
            .is_some_and(|policy| matches!(policy, InterpolatedDespawn::AtInterpolationTick));
    if delays {
        let tick = entity
            .world()
            .get_resource::<ConfirmedTickMap>()
            .and_then(|confirmed_ticks| confirmed_ticks.message_tick(ctx.message_tick));
        if let Some(tick) = tick {
            entity.insert(DelayedDespawn::new(tick));
            return;
        }
        debug!(
            entity = ?entity.id(),
            message_tick = ?ctx.message_tick,
            "despawning at once an interpolated entity whose despawn no confirmed tick maps"
        );
    }
    DespawnReplicated.apply(entity);
}

#[cfg(test)]
mod tests;
