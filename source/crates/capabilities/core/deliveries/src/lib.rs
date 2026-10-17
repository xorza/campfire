// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (09-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

use bevy_ecs::schedule::SystemSet;
use bevy_ecs::world::World;
use campfire_actions::{ActionBook, ActionsSet};
use campfire_common::EnumAll;
use campfire_declare::{Declarations, InstallCx, Module, ModuleSet, SetPlace};
use campfire_sim::{Breach, Capability};
use campfire_units::{BodyUnit, ForcedMove};

use crate::deliveries_api::DeliveriesApi;
use crate::delivery_hooks::DeliveryHooks;

mod dash_delivering;
mod delivered;
mod deliverers;
mod deliveries_api;
mod delivering;
mod delivery_hooks;
mod delivery_queue;
mod delivery_spawner;
mod teleported;

pub use crate::dash_delivering::DashDelivering;
pub use crate::delivered::{AttackShot, Delivered, DeliveredPayload};
pub use crate::deliverers::Deliverers;
pub use crate::delivering::{Delivering, Launcher};
pub use crate::delivery_queue::{DashEnd, DeliveryQueue};
pub use crate::delivery_spawner::{DeliverySpawner, NewDelivery};
pub use crate::teleported::Teleported;

/// The deliveries' module: the hooks that what they reached runs.
#[derive(Debug)]
pub struct Deliveries;

/// The steps of `deliveries`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum DeliverySet {
    /// What the deliveries reached runs their actions' hooks, and the deliveries that ended
    /// run theirs.
    Deliver,
}

impl ModuleSet for DeliverySet {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        match self {
            Self::Deliver => SetPlace::at(ActionsSet::Deliver),
        }
    }
}

impl Module for Deliveries {
    const CAPABILITY: Option<Capability> = None;
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];
    type Set = DeliverySet;

    fn declare<D: Declarations>(d: &mut D) {
        d.scratch::<DeliveryQueue>();
        d.component::<DashDelivering>();
        d.state::<BodyUnit, DashDelivering>();
        d.require::<DashDelivering, ForcedMove>();
        d.check::<DashDelivering>(Self::dashes_known);
        d.api(DeliveriesApi::register);
    }

    /// Adds the deliveries' hooks to a match whose modules call for them, as a module that sets a
    /// delivery out or calls one does: in `Phase::Deliver`, after the deliveries move.
    fn install(cx: &mut InstallCx<'_, Self::Set>) {
        if !cx.calls_for::<Deliverers>() {
            return;
        }
        cx.scripted(DeliverySet::Deliver, DeliveryHooks::deliver);
        cx.unscripted(DeliverySet::Deliver, DeliveryHooks::end);
    }
}

impl Deliveries {
    /// The restore check of each dash that delivers an action: its unit dashes, and its action
    /// at its rank is in the book, whose hooks the dash's end runs.
    fn dashes_known(world: &World) -> Result<(), Breach> {
        let book = world.resource::<ActionBook>();
        Breach::first(world, |entity| {
            let Some(dash) = world.get::<DashDelivering>(entity) else {
                return false;
            };
            let dashes = matches!(
                world.get::<ForcedMove>(entity),
                Some(ForcedMove::Dash { .. })
            );
            !dashes || !book.ranked(dash.by.action, dash.by.rank)
        })
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::system::RunSystemOnce;
    use bevy_ecs::world::World;
    use campfire_math::Vec3;
    use campfire_sim::StableId;
    use campfire_values::UnitType;

    use crate::Deliveries;
    use crate::delivery_spawner::{DeliverySpawner, NewDelivery};

    impl Deliveries {
        /// Spawns a delivery of `unit_type` for `source` at `pos`, with `parts`, which make it
        /// one, as an action's delivery spawns: its stable id.
        pub fn spawn_of<B: Bundle>(
            world: &mut World,
            source: StableId,
            unit_type: UnitType,
            pos: impl Into<Vec3>,
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
