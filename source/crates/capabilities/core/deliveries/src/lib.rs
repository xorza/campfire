#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use campfire_core::{
    DashDelivery, Declarations, InstallCx, Module, ModuleSet, Phase, Scratch, SetPlace,
};
use campfire_math::Vec3;
use campfire_sim::{Capability, Position, StableId};
use campfire_values::Hit;

use crate::deliveries_api::DeliveriesApi;
use crate::delivery_hooks::DeliveryHooks;

mod delivered;
mod deliverers;
mod deliveries_api;
mod delivering;
mod delivery_hooks;
mod delivery_spawner;
mod teleported;

pub use crate::delivered::{Delivered, Reached};
pub use crate::deliverers::{Deliverers, DeliveryKind};
pub use crate::delivering::{Delivering, Launcher};
pub use crate::delivery_spawner::DeliverySpawner;
pub use crate::teleported::Teleported;

/// The hits and ends of this tick's deliveries whose action's hooks run, in the order they
/// happened; and the delivery units that ended, which despawn once the hooks ran, so a hook still
/// reads its delivery. Not state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub struct Deliveries {
    pub delivered: Vec<Delivered>,
    pub ended: Vec<Entity>,
}

campfire_common::enum_all! {
    /// The steps of `deliveries`.
    #[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) enum DeliverySet {
        /// What the deliveries reached runs their actions' hooks, and the deliveries that ended
        /// run theirs.
        Deliver,
    }
    /// Every step.
    pub const ALL;
}

impl ModuleSet for DeliverySet {
    const CAPABILITY: Option<Capability> = None;
    const ALL: &'static [DeliverySet] = &DeliverySet::ALL;

    fn place(self) -> SetPlace {
        SetPlace::Phase(match self {
            DeliverySet::Deliver => Phase::Deliver,
        })
    }
}

impl Module for Deliveries {
    const CAPABILITY: Option<Capability> = None;
    const NEEDS: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<DeliverySet>();
        d.scratch::<Deliveries>();
        d.api(DeliveriesApi::register);
    }

    /// Adds the deliveries' hooks to a match whose modules call for them, as a module that sets a
    /// delivery out or calls one does: in `Phase::Deliver`, after the deliveries move.
    fn install(cx: &mut InstallCx<'_>) {
        if !cx.calls_for::<Deliverers>() {
            return;
        }
        if cx.scripts() {
            cx.schedule
                .add_systems(DeliveryHooks::deliver.in_set(DeliverySet::Deliver));
        } else {
            cx.schedule
                .add_systems(DeliveryHooks::end.in_set(DeliverySet::Deliver));
        }
    }
}

impl Deliveries {
    /// Records the end of a dash that delivers `dash`, aimed at `target` if a unit, whose unit
    /// stands at `pos` after a last step in `direction`: an end with no delivery unit, after
    /// the meters the dash went, whose hooks are its action's.
    pub fn end_dash(
        &mut self,
        dash: DashDelivery,
        target: Option<StableId>,
        pos: Position,
        direction: Option<Vec3>,
    ) {
        let by = Delivering {
            source: dash.source,
            action: dash.action,
            rank: dash.rank,
            start: dash.start,
            launch: None,
        };
        let hit = Hit {
            delivery: None,
            target,
            pos,
            distance: dash.dashed,
            direction,
        };
        self.delivered.push(Delivered {
            by,
            reach: Reached::End,
            hit,
        });
    }
}

impl Scratch for Deliveries {
    fn is_clear(&self) -> bool {
        self.delivered.is_empty() && self.ended.is_empty()
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::system::RunSystemOnce;
    use bevy_ecs::world::World;
    use campfire_core::UnitType;
    use campfire_sim::{Position, StableId};

    use crate::Deliveries;
    use crate::delivery_spawner::DeliverySpawner;

    impl Deliveries {
        /// Spawns a delivery of `unit_type` for `source` at `pos`, with `parts`, which make it
        /// one, as an action's delivery spawns: its stable id.
        pub fn spawn_of<B: Bundle>(
            world: &mut World,
            source: StableId,
            unit_type: UnitType,
            pos: impl Into<Position>,
            parts: B,
        ) -> StableId {
            let pos = pos.into();
            let mut parts = Some(parts);
            let spawn = move |mut spawner: DeliverySpawner<'_, '_>| {
                let parts = parts.take().expect("one spawn");
                spawner.spawn(source, pos, unit_type, None, |_| parts)
            };
            world
                .run_system_once(spawn)
                .expect("the spawn runs")
                .expect("its source stands")
        }
    }
}
