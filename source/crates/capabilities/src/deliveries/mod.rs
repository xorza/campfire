use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use campfire_math::Vec3;
use campfire_sim::{Capability, Position, StableId};

use crate::declare::declarations::Declarations;
use crate::declare::install_cx::InstallCx;
use crate::declare::module::Module;
use crate::declare::module_set::ModuleSet;
use crate::declare::phase::Phase;
use crate::declare::scratch::Scratch;
use crate::declare::set_place::SetPlace;
use crate::deliveries::delivered::{Delivered, Reached};
use crate::deliveries::deliverers::Deliverers;
use crate::deliveries::deliveries_api::DeliveriesApi;
use crate::deliveries::delivering::Delivering;
use crate::deliveries::delivery_hooks::DeliveryHooks;
use crate::units::forced_move::DashDelivery;
use crate::values::hit::Hit;

pub(crate) mod delivered;
pub(crate) mod deliverers;
pub(crate) mod deliveries_api;
pub(crate) mod delivering;
pub(crate) mod delivery_hooks;
pub(crate) mod delivery_spawner;
pub(crate) mod teleported;

/// The hits and ends of this tick's deliveries whose action's hooks run, in the order they
/// happened; and the delivery units that ended, which despawn once the hooks ran, so a hook still
/// reads its delivery. Not state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub(crate) struct Deliveries {
    pub(crate) delivered: Vec<Delivered>,
    pub(crate) ended: Vec<Entity>,
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
    pub(crate) const ALL;
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
    pub(crate) fn end_dash(
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

#[cfg(test)]
pub(crate) mod test_world {
    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::system::RunSystemOnce;
    use bevy_ecs::world::World;
    use campfire_sim::{Position, StableId};

    use crate::deliveries::Deliveries;
    use crate::deliveries::delivery_spawner::DeliverySpawner;
    use crate::units::unit_type::UnitType;

    impl Deliveries {
        /// Spawns a delivery of `unit_type` for `source` at `pos`, with `parts`, which make it
        /// one, as an action's delivery spawns: its stable id.
        pub(crate) fn spawn_of<B: Bundle>(
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
