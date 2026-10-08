use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::world::World;
use campfire_common::Ticks;
use campfire_script::rhai::Dynamic;
use campfire_sim::{SimSet, StateRegistry, TickRate};

use crate::abilities::cast_spends::CastSpends;
use crate::abilities::casts::Casts;
use crate::abilities::channels::Channels;
use crate::abilities::toggles::Toggles;
use crate::actions::ActionsSet;
use crate::actions::action_target::ActionTarget;
use crate::actions::slot_kind::SlotKind;
use crate::combat::CombatSet;
use crate::deliveries::Deliveries;
use crate::items::inventory::Inventory;
use crate::scripts::ctx::Ctx;
use crate::stats::StatsSet;
use crate::units::block::Block;
use crate::units::unit_tags::UnitTags;
use crate::units::view::View;

pub(crate) mod abilities_api;
pub(crate) mod abilities_effect;
pub(crate) mod cast_spends;
pub(crate) mod casts;
pub(crate) mod channels;
pub(crate) mod toggles;

/// The `abilities` capability: abilities in slots, cast through their checks, with the effect a
/// script describes.
#[derive(Debug)]
pub struct Abilities;

/// The systems of `abilities`, for the capabilities built on it to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AbilitiesSet {
    /// In `SimSet::Inputs`, before the holds follow: toggles pay their seconds or turn off.
    Toggles,
}

impl Abilities {
    /// `target` as a script reads it: a living unit's handle, a point, or `()` for none or a
    /// unit no longer living.
    fn target(view: &View, target: ActionTarget) -> Dynamic {
        match target {
            ActionTarget::None => Dynamic::UNIT,
            ActionTarget::Unit(id) => view
                .living(id)
                .and_then(|_| view.unit(id))
                .map_or(Dynamic::UNIT, Dynamic::from),
            ActionTarget::Point(at) => Dynamic::from(at),
        }
    }

    /// Whether a unit with `tags`, under a forced move when `forced`, carrying `inventory`, is
    /// kept from the action of a slot of `kind`: from using it, for one of its inventory's, else,
    /// and with no slot, from casting it.
    fn blocked(
        tags: Option<&UnitTags>,
        forced: bool,
        inventory: Option<&Inventory>,
        kind: Option<SlotKind>,
    ) -> bool {
        let group = kind.map_or(Block::Cast, |kind| Inventory::group(inventory, kind));
        UnitTags::blocks(tags, forced, group)
    }

    /// A second at `rate`, the time a toggle pays its cost each.
    fn second(rate: TickRate) -> Ticks {
        rate.ticks(1000)
            .expect("a second counts in ticks at every rate")
    }

    /// Adds abilities to a match, on the core `Units` installs: in Hit, after attacks strike and
    /// before the tick's projectiles launch, due casts resolve: the delivery, the cost, the
    /// cooldown and the script's effects apply together, or none of them. A dash an instant cast
    /// starts is its delivery, whose end runs with the deliveries' hooks. A cast resolves in the
    /// script host; without the core's scripts, as on a client, a due cast of a unit it predicts
    /// only cools down, as the server's does.
    pub fn install(world: &mut World, schedule: &mut Schedule, _: &mut StateRegistry) {
        Deliveries::install(world, schedule);
        world.init_resource::<CastSpends>();
        schedule.add_systems(
            Casts::start_casts
                .in_set(SimSet::Act)
                .in_set(ActionsSet::Start),
        );
        if !world.contains_non_send::<Ctx>() {
            schedule.add_systems(
                (Casts::predict_casts, Channels::predict_channels)
                    .chain()
                    .in_set(SimSet::Hit)
                    .after(CombatSet::Fire)
                    .before(CombatSet::Launch),
            );
            return;
        }
        schedule.add_systems((
            (Casts::resolve_casts, Channels::run_channels)
                .chain()
                .in_set(SimSet::Hit)
                .after(CombatSet::Fire)
                .before(CombatSet::Launch),
            Toggles::run_toggles
                .in_set(SimSet::Inputs)
                .in_set(AbilitiesSet::Toggles)
                .after(StatsSet::Regenerate)
                .after(CombatSet::Respawn)
                .before(ActionsSet::HoldAtInputs),
        ));
    }
}

#[cfg(test)]
mod tests;
