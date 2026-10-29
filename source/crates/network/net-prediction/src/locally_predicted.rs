use crate::prediction_setup::PredictionSetup;
use crate::rollback_check_state::RollbackCheckState;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use campfire_net_replication::{PreSpawned, Predicted};
use campfire_net_timeline::LocalTimeline;
use core::fmt::Debug;

/// A locally predicted entity that this end alone holds: no server replicates it and no pre-spawn
/// waits for its match, so its own end decides when it goes (11-net-stack/replication.md, The
/// server alone ends a replicated entity's life on the receiver).
pub(crate) type LocalOnly = (
    With<LocallyPredicted>,
    Without<Predicted>,
    Without<PreSpawned>,
);

#[derive(Component, PartialEq, Eq, Debug, Clone, Copy)]
#[component(on_add = LocallyPredicted::on_add)]
/// A predicted entity whose state never triggers a rollback, as a projectile the client's own
/// input fires, which a rollback still returns to its predicted history.
pub struct LocallyPredicted {
    /// After spawning a LocallyPredicted entity, any rollback that happens shortly after might
    /// despawn the entity (since it didn't exist at the start of rollback) or remove its
    /// components.
    ///
    /// If the entity was spawned by the prediction itself (for instance by a 'Shoot' input), then
    /// we want the entity to be despawned as it will get re-created during rollback. But if the
    /// entity was spawned as a one-off event (for example replicated by the server upon
    /// connection), we don't want the entity to be affected by rollbacks for a short period after
    /// being spawned.
    ///
    /// This protects only the entity's lifecycle. It does not create missing component histories or
    /// global rollback resources, such as a physics engine's spatial-query history.
    pub skip_despawn: bool,
    /// For entities where skip_despawn is True, after how many ticks do we start enabling back
    /// rollbacks?
    pub enable_rollback_after: u8,
}

impl Default for LocallyPredicted {
    fn default() -> Self {
        Self {
            skip_despawn: false,
            enable_rollback_after: 20,
        }
    }
}

impl LocallyPredicted {
    /// Whether `entity` is one `LocalOnly` matches.
    pub(crate) fn is_local_only(entity: EntityRef<'_>) -> bool {
        entity.contains::<Self>()
            && !entity.contains::<Predicted>()
            && !entity.contains::<PreSpawned>()
    }

    fn on_add(mut world: DeferredWorld<'_>, context: HookContext) {
        let locally_predicted = *world.get::<Self>(context.entity).unwrap();
        let timeline = world.resource::<LocalTimeline>();
        let (tick, simulated) = (timeline.tick(), timeline.simulated_tick());
        if !world.contains_resource::<PredictionSetup>() {
            return;
        }
        let Some(mut check_state) = world.get_resource_mut::<RollbackCheckState>() else {
            return;
        };
        let (entries, tick) = if locally_predicted.skip_despawn {
            (
                &mut check_state.local_skip_despawn,
                tick + i32::from(locally_predicted.enable_rollback_after),
            )
        } else if let Some(simulated) = simulated {
            (&mut check_state.local_despawn, simulated)
        } else {
            // Only an entity spawned in `FixedMain` is spawned again by a rollback's replay, so
            // no rollback despawns one spawned outside it.
            return;
        };
        // Each list stays in tick order, which a rollback splits by tick.
        let at = entries.partition_point(|(entry_tick, _)| *entry_tick <= tick);
        entries.insert(at, (tick, context.entity));
    }
}
