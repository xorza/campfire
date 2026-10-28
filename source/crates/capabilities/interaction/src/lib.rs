// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

use bevy_ecs::schedule::SystemSet;
use bevy_ecs::world::World;
use campfire_actions::{ActionSlots, SlotClaims};
use campfire_common::EnumAll;
use campfire_declarations::{ById, Declarations, InstallCx, Module, ModuleSystems, SetPlace};
use campfire_sim::{Breach, Capability, EntityIndex, SimEdgeSystems, SimSystems, StableId};
use campfire_stats::StatsSystems;
use campfire_units::{BodyUnit, SpawnCallbacks};
use campfire_values::{EngineTag, TagProperties, UnitType};

use crate::containers::Containers;
use crate::use_orders::UseOrders;

mod containers;
mod enter_action_data;
mod enter_data;
mod enter_spec;
mod entering;
mod error;
mod interaction_api;
mod interaction_books;
mod interaction_effect;
mod interaction_rules;
mod rider_size;
mod riders;
mod rides_data;
mod riding;
mod use_action_data;
mod use_data;
mod use_orders;
mod use_slot;
mod use_spec;
mod uses_data;
mod using;

pub use crate::enter_action_data::EnterActionData;
pub use crate::enter_data::{EnterData, FireData, PayloadData, RiderCount};
pub use crate::enter_spec::{EnterSpec, FireSpec, RiderPayload};
pub use crate::error::{EnterError, UseError};
pub use crate::interaction_api::InteractionApi;
pub use crate::rider_size::RiderSize;
pub use crate::riders::{HeldFrom, Rider, Riders};
pub use crate::rides_data::RidesData;
pub use crate::riding::Riding;
pub use crate::use_action_data::UseActionData;
pub use crate::use_data::UseData;
pub use crate::use_slot::UseSlot;
pub use crate::use_spec::UseSpec;
pub use crate::uses_data::UsesData;
pub use crate::using::{UnitUse, Using};

/// The `interaction` capability: units that use objects, each object's use an action of the
/// `use` kind, and units that ride in containers, each container's entry an action of the `enter`
/// kind, which an order fills a user's use slot with and the abilities' pipeline runs: either
/// needs `orders` and `abilities` beside it, which the load checks.
#[derive(Debug)]
pub struct Interaction;

/// The steps of `interaction`, each the server's.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum InteractionSystems {
    /// After the actions ran: each flying container a unit comes to, or a rider of which waits to
    /// leave, lands, and climbs again once none does.
    Land,
    /// After the units scanned: each firing rider of a container of fire points whose target
    /// changed moves to the free point nearest it.
    Aim,
    /// After the units moved: each container's riders stand where it does, or at their fire
    /// points.
    Carry,
    /// After the hits and before the damage: each container holds its modifier on its riders.
    Hold,
    /// After the units died: the riders of each container that fell leave, hurt, and those of
    /// each container whose life fell to its `full` share start to leave.
    Fall,
    /// After the containers fell: the riders whose exits came leave their containers.
    Exit,
}

impl InteractionSystems {
    /// The gap after the tick's damage and deaths.
    const RESOLVED: SimEdgeSystems = SimEdgeSystems::After(SimSystems::Resolve);
}

impl ModuleSystems for InteractionSystems {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        match self {
            Self::Aim => SetPlace::gap(SimEdgeSystems::After(SimSystems::Think)),
            Self::Land => SetPlace::gap(SimEdgeSystems::After(SimSystems::Act)),
            Self::Carry => SetPlace::gap(SimEdgeSystems::After(SimSystems::Collide)),
            // Before the tick's damage, so the stats' hold of Resolve takes it beside the other
            // holds, in an order of its own.
            Self::Hold => SetPlace::gap(SimEdgeSystems::After(SimSystems::Hit)),
            // Before the units' stats derive again, so a rider out derives with them.
            Self::Fall => {
                SetPlace::gap(Self::RESOLVED).before(StatsSystems::Refresh(Self::RESOLVED))
            }
            Self::Exit => SetPlace::gap(Self::RESOLVED)
                .after(Self::Fall)
                .before(StatsSystems::Refresh(Self::RESOLVED)),
        }
    }
}

impl Interaction {
    /// The engine tag of a container that holds a rider, which its looks test and a scan's
    /// filter may select (04-capabilities/interaction.md, A container holding riders holds a tag).
    pub const HOLDING: EngineTag = EngineTag::new(12, "holding", TagProperties::NONE);

    /// A unit of a type with a use slot holds one slot of its kind, as it spawns with, which a use
    /// fills: a restored user without one would fill a slot of another kind, or one past its last.
    fn users_hold_their_slot(world: &World) -> Result<(), Breach> {
        let Some(use_slots) = world.get_resource::<ById<UnitType, UseSlot>>() else {
            return Ok(());
        };
        Breach::first(world, |entity| {
            let Some(use_slot) = world
                .get::<UnitType>(entity)
                .and_then(|&unit_type| use_slots.get(unit_type))
            else {
                return false;
            };
            let held = world.get::<ActionSlots>(entity).map_or(0, |slots| {
                slots
                    .iter()
                    .filter(|slot| slot.kind == use_slot.kind())
                    .count()
            });
            held != 1
        })
    }
}

impl Interaction {
    /// Each rider is one of its container's riders, so it leaves as they do: a restored rider its
    /// container does not hold would stay out of the world.
    fn riders_are_held(world: &World) -> Result<(), Breach> {
        let index = world.resource::<EntityIndex>();
        Breach::first(world, |entity| {
            let (Some(riding), Some(&id)) =
                (world.get::<Riding>(entity), world.get::<StableId>(entity))
            else {
                return false;
            };
            let container = index.get(riding.container());
            let riders = container.and_then(|container| world.get::<Riders>(container));
            !riders.is_some_and(|riders| riders.holds(id))
        })
    }
}

impl Module for Interaction {
    const CAPABILITY: Option<Capability> = Some(Capability::Interaction);
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[Capability::Stats];
    type Systems = InteractionSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.listed::<UseActionData>();
        d.listed::<EnterActionData>();
        d.section::<UseData>();
        d.section::<UsesData>();
        d.section::<EnterData>();
        d.section::<RidesData>();
        d.book::<ById<UnitType, UseSpec>>(UseSpec::book);
        d.book::<ById<UnitType, UseSlot>>(UseSlot::book);
        d.book::<ById<UnitType, EnterSpec>>(EnterSpec::book);
        d.book::<ById<UnitType, RiderSize>>(RiderSize::book);
        d.engine_tags(&[Self::HOLDING]);
        d.component::<Riding>();
        d.state::<BodyUnit, Riding>();
        d.check::<Riding>(Self::riders_are_held);
        d.component::<Riders>();
        d.timed::<Riders>();
        d.part::<BodyUnit, Riders>(|at| at.of_type::<EnterSpec>().map(|_| Riders::default()));
        d.callback::<SpawnCallbacks>(Capability::Interaction, Containers::fill);
        d.observer(Containers::ordered);
        d.extend::<ById<UnitType, ActionSlots>>(Self::use_slots);
        d.extend::<SlotClaims>(Self::claim_slots);
        d.rule(Self::rule);
        d.check::<ActionSlots>(Self::users_hold_their_slot);
        d.api(InteractionApi::register);
        d.observer(UseOrders::ordered);
    }

    /// Adds the containers' passes, on the server: in the gap after Think, each firing rider whose
    /// target changed takes the free fire point nearest it; in the gap after Act, each flier a unit
    /// comes to, or a rider of which waits to leave, lands, and climbs once none does; in the gap
    /// after Collide, each container's riders stand where it does, or at their points; in the gap
    /// after Hit, each container holds its modifier on its riders, which stats hold in Resolve; in
    /// the gap after Resolve, the riders of the containers that fell leave, hurt, those of the
    /// containers whose life fell to their `full` share start to leave, and then the riders whose
    /// exits came leave. A `use` or an `enter` order fills a use slot as it is given, and the
    /// abilities' casts run it.
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        cx.server(InteractionSystems::Aim, Containers::aim);
        cx.server(InteractionSystems::Land, Containers::land);
        cx.server(InteractionSystems::Carry, Containers::carry);
        cx.server(InteractionSystems::Hold, Containers::hold);
        cx.server(InteractionSystems::Fall, Containers::fall);
        cx.server(InteractionSystems::Exit, Containers::let_out);
    }
}
