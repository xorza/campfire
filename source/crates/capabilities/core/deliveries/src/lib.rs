#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::World;
use campfire_actions::ActionBook;
use campfire_declare::{Declarations, InstallCx, Module, ModuleSet, Phase, Scratch, SetPlace};
use campfire_math::Vec3;
use campfire_sim::{Breach, Capability, Position, StableId};
use campfire_units::ForcedMove;
use campfire_values::{DashDelivery, Hit};

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
pub use crate::delivery_spawner::{DeliverySpawner, NewDelivery};
pub use crate::teleported::Teleported;

/// The hits and ends of this tick's deliveries whose action's hooks run, in the order they
/// happened; and the delivery units that ended, which despawn once the hooks ran, so a hook still
/// reads its delivery. Not state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub struct Deliveries {
    delivered: Vec<Delivered>,
    ended: Vec<Entity>,
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
    const USES: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<DeliverySet>();
        d.scratch::<Deliveries>();
        d.check::<ForcedMove>(Deliveries::dashes_known);
        d.api(DeliveriesApi::register);
    }

    /// Adds the deliveries' hooks to a match whose modules call for them, as a module that sets a
    /// delivery out or calls one does: in `Phase::Deliver`, after the deliveries move.
    fn install(cx: &mut InstallCx<'_>) {
        if !cx.calls_for::<Deliverers>() {
            return;
        }
        cx.scripted(DeliveryHooks::deliver.in_set(DeliverySet::Deliver));
        cx.unscripted(DeliveryHooks::end.in_set(DeliverySet::Deliver));
    }
}

impl Deliveries {
    /// Records `delivered`, a delivery that reached what it reached this tick.
    pub fn deliver(&mut self, delivered: Delivered) {
        self.delivered.push(delivered);
    }

    /// Records that the delivery unit `entity` ended this tick, so it despawns.
    pub fn end(&mut self, entity: Entity) {
        self.ended.push(entity);
    }

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

    /// The restore check of each dash that delivers an action: its action at its rank is in the
    /// book, whose hooks the dash's end runs.
    fn dashes_known(world: &World) -> Result<(), Breach> {
        let book = world.resource::<ActionBook>();
        Breach::first(world, |entity| {
            let Some(ForcedMove::Dash {
                delivers: Some(dash),
                ..
            }) = world.get::<ForcedMove>(entity)
            else {
                return false;
            };
            !book.ranked(dash.action, dash.rank)
        })
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
    use campfire_sim::{Position, StableId};
    use campfire_values::UnitType;

    use crate::Deliveries;
    use crate::delivered::Delivered;
    use crate::delivery_spawner::{DeliverySpawner, NewDelivery};

    impl Deliveries {
        /// The deliveries recorded this tick, to change as a test sets them.
        pub const fn delivered_mut(&mut self) -> &mut Vec<Delivered> {
            &mut self.delivered
        }

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
                let delivery = NewDelivery {
                    source,
                    unit_type,
                    pos,
                    id: None,
                };
                spawner.spawn(delivery, |_| parts)
            };
            world
                .run_system_once(spawn)
                .expect("the spawn runs")
                .expect("its source stands")
        }
    }
}
