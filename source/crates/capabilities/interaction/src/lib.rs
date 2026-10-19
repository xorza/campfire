// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

use bevy_ecs::world::World;
use campfire_actions::{ActionSlots, SlotClaims};
use campfire_declare::{ByType, Declarations, InstallCx, Module, NoSet};
use campfire_sim::{Breach, Capability};
use campfire_values::UnitType;

use crate::use_orders::UseOrders;

mod error;
mod interaction_api;
mod interaction_books;
mod interaction_rules;
mod use_action_data;
mod use_data;
mod use_orders;
mod use_slot;
mod use_spec;
mod uses_data;
mod using;

pub use crate::error::UseError;
pub use crate::interaction_api::InteractionApi;
pub use crate::use_action_data::UseActionData;
pub use crate::use_data::UseData;
pub use crate::use_slot::UseSlot;
pub use crate::use_spec::UseSpec;
pub use crate::uses_data::UsesData;
pub use crate::using::{UnitUse, Using};

/// The `interaction` capability: units that use objects, each object's use an action of the
/// `use` kind, which a user's use slot runs through the abilities' pipeline: a `use` action needs
/// `abilities` beside it, which the load checks.
#[derive(Debug)]
pub struct Interaction;

impl Interaction {
    /// A unit of a type with a use slot holds one slot of its kind, as it spawns with, which a use
    /// fills: a restored user without one would fill a slot of another kind, or one past its last.
    fn users_hold_their_slot(world: &World) -> Result<(), Breach> {
        let Some(use_slots) = world.get_resource::<ByType<UseSlot>>() else {
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

impl Module for Interaction {
    const CAPABILITY: Option<Capability> = Some(Capability::Interaction);
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];
    type Set = NoSet;

    fn declare<D: Declarations>(d: &mut D) {
        d.listed::<UseActionData>();
        d.section::<UseData>();
        d.section::<UsesData>();
        d.book::<ByType<UseSpec>>(UseSpec::book);
        d.book::<ByType<UseSlot>>(UseSlot::book);
        d.extend::<ByType<ActionSlots>>(Self::use_slots);
        d.extend::<SlotClaims>(Self::claim_slots);
        d.rule(Self::rule);
        d.check::<ActionSlots>(Self::users_hold_their_slot);
        d.api(InteractionApi::register);
        d.observer(UseOrders::ordered);
    }

    /// Adds nothing to the schedule: a `use` order fills a use slot as it is given, and the
    /// abilities' casts run it.
    fn install(_: &mut InstallCx<'_, Self::Set>) {}
}
