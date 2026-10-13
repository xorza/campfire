use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use campfire_common::Ticks;
use campfire_script::rhai::Dynamic;
use campfire_sim::{Capability, SimPlace, SimSet, TickRate};

use crate::abilities::abilities_api::AbilitiesApi;
use crate::abilities::cast_spends::CastSpends;
use crate::abilities::casts::Casts;
use crate::abilities::channels::Channels;
use crate::abilities::toggles::Toggles;
use crate::actions::ActionsSet;
use crate::actions::action_target::ActionTarget;
use crate::actions::slot_kind::SlotKind;
use crate::combat::CombatSet;
use crate::declare::declarations::Declarations;
use crate::declare::install_cx::InstallCx;
use crate::declare::module::Module;
use crate::declare::module_set::ModuleSet;
use crate::declare::set_order::SetOrder;
use crate::deliveries::deliverers::Deliverers;
use crate::items::inventory::Inventory;
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

campfire_common::enum_all! {
    /// The steps of `abilities`.
    #[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) enum AbilitiesSet {
        /// Before the holds follow: toggles pay their seconds or turn off.
        Toggles,
        /// After attacks strike and before the tick's projectiles launch: due casts resolve.
        Resolve,
    }
    /// Every step.
    pub(crate) const ALL;
}

impl ModuleSet for AbilitiesSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Abilities);
    const ALL: &'static [AbilitiesSet] = &AbilitiesSet::ALL;

    fn place(self) -> SimPlace {
        match self {
            AbilitiesSet::Toggles => SimPlace::Stage(SimSet::Inputs),
            AbilitiesSet::Resolve => SimPlace::Stage(SimSet::Hit),
        }
    }

    fn order(self, o: &mut SetOrder) {
        match self {
            AbilitiesSet::Toggles => {
                o.after(StatsSet::Regenerate)
                    .after(CombatSet::Respawn)
                    .before(ActionsSet::HoldAtInputs);
            }
            AbilitiesSet::Resolve => {
                o.after(ActionsSet::Fire).before(ActionsSet::Launch);
            }
        }
    }
}

impl Module for Abilities {
    const CAPABILITY: Option<Capability> = Some(Capability::Abilities);
    const NEEDS: &'static [Capability] = &[Capability::Combat];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<AbilitiesSet>();
        d.calls::<Deliverers>();
        d.calls::<CastSpends>();
        d.api(AbilitiesApi::register);
    }

    /// Adds abilities to a match, on the core `Units` installs: in Hit, after attacks strike and
    /// before the tick's projectiles launch, due casts resolve: the delivery, the cost, the
    /// cooldown and the script's effects apply together, or none of them. A dash an instant cast
    /// starts is its delivery, whose end runs with the deliveries' hooks. A cast resolves in the
    /// script host; without the core's scripts, as on a client, a due cast of a unit it predicts
    /// only cools down, as the server's does.
    fn install(cx: &mut InstallCx<'_>) {
        let scripts = cx.scripts();
        let schedule = &mut *cx.schedule;
        schedule.add_systems(Casts::start_casts.in_set(ActionsSet::Start));
        if !scripts {
            schedule.add_systems(
                (Casts::predict_casts, Channels::predict_channels)
                    .chain()
                    .in_set(AbilitiesSet::Resolve),
            );
            return;
        }
        schedule.add_systems((
            (Casts::resolve_casts, Channels::run_channels)
                .chain()
                .in_set(AbilitiesSet::Resolve),
            Toggles::run_toggles.in_set(AbilitiesSet::Toggles),
        ));
    }
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
    const fn second(rate: TickRate) -> Ticks {
        rate.ticks(1000)
            .expect("a second counts in ticks at every rate")
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    use std::collections::BTreeMap;

    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::world::World;
    use campfire_common::PlayerSlot;
    use campfire_math::Num;
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{Position, SimWorld, StableId};

    use crate::abilities::Abilities;
    use crate::actions::Actions;
    use crate::actions::action_data::ActionData;
    use crate::actions::action_slots::{ActionSlot, ActionSlots};
    use crate::actions::action_target::ActionTarget;
    use crate::actions::in_progress::InProgress;
    use crate::actions::slot_kind::SlotKind;
    use crate::actions::slot_times::{SlotTime, SlotTimes, TimedSlots};
    use crate::combat::on_death::OnDeath;
    use crate::mode::Mode;
    use crate::scripts::hook::Hook;
    use crate::scripts::script_failures::ScriptFailures;
    use crate::scripts::script_failures::internals::FailedCall;
    use crate::stats::Stats;
    use crate::stats::life::Life;
    use crate::stats::modifier_clocks::ModifierClocks;
    use crate::stats::modifiers::Modifiers;
    use crate::stats::pool_id::PoolId;
    use crate::stats::pools::Pools;
    use crate::stats::stat_rule::StatRule;
    use crate::units::Units;
    use crate::units::action_id::ActionId;
    use crate::units::move_step::MoveStep;
    use crate::units::owner::Owner;
    use crate::units::team::Team;
    use crate::units::type_scope::TypeScope;
    use crate::units::unit_type::UnitType;
    use crate::units::unit_type_core::UnitTypeCore;
    use crate::values::rank::Rank;
    use crate::values::stat::Stat;

    /// Mana, the second of the match's pools, after the life pool.
    pub(crate) const MANA: PoolId = PoolId::new(1).unwrap();
    /// Rage, the third of the match's pools.
    pub(crate) const RAGE: PoolId = PoolId::new(2).unwrap();

    /// The stats of the scaling tests' mode, in its order: attack damage, an engine stat, first.
    pub(crate) fn scaling_stats() -> [Stat; 4] {
        let mut stats = ["attack_damage", "ability_power", "armor", "spell_vamp"]
            .map(|name| Stat::named(name).unwrap());
        stats.sort();
        stats
    }

    impl Abilities {
        /// Gives the match a stat book of the stats the scaling params name, with no unit type.
        pub(crate) fn load_stats(world: &mut World) {
            let rules = scaling_stats().map(|stat| (stat, StatRule::default()));
            Stats::load_rules(world, &BTreeMap::from(rules));
        }

        /// Loads `data` as the action `name` of package 0, of 5 ranks, with its script `source`.
        pub(crate) fn load(
            world: &mut World,
            name: &str,
            data: &ActionData,
            source: &str,
        ) -> ActionId {
            let script = Units::compile_hooked(world, source).unwrap();
            Actions::load(world, 0, name, data, Some(script), 5).unwrap()
        }

        /// Loads the mode's types the tests spawn: `combatant`, of 500 health that stays when it
        /// dies, with no slot; `caster`, a combatant of 100 mana and 20 rage; and `walking
        /// caster`, a caster that walks a meter a tick.
        pub(crate) fn load_types(world: &mut World) {
            let core = UnitTypeCore::default();
            let combatant = Units::load_type(world, TypeScope::Mode, "combatant", &core);
            Abilities::give_combatant(world, combatant);
            for name in ["caster", "walking caster"] {
                let caster = Units::load_type(world, TypeScope::Mode, name, &core);
                Abilities::give_caster(world, caster, 100, 20);
            }
            let walking = Units::type_named(world, "walking caster").unwrap();
            Units::give_type(world, walking, MoveStep::new(Num::ONE).unwrap());
        }

        /// Gives `unit_type` a combatant's books: 500 health that stays when it dies, and no
        /// slot.
        pub(crate) fn give_combatant(world: &mut World, unit_type: UnitType) {
            Units::give_type(world, unit_type, Life::full(Num::int(500)));
            Units::give_type(world, unit_type, OnDeath::Stay);
            Units::give_type(world, unit_type, ActionSlots::new([]));
        }

        /// Gives `unit_type` a combatant's books, and pools of `mana` and `rage`.
        pub(crate) fn give_caster(world: &mut World, unit_type: UnitType, mana: i64, rage: i64) {
            Abilities::give_combatant(world, unit_type);
            let pools = [(MANA, mana), (RAGE, rage)];
            let pools = Pools::new(pools.map(|(pool, max)| (pool, Num::int(max)))).unwrap();
            Units::give_type(world, unit_type, pools);
        }

        /// A `combatant` of `team` with `parts`.
        pub(crate) fn spawn_combatant(
            world: &mut World,
            pos: impl Into<Position>,
            team: Team,
            parts: impl Bundle,
        ) -> StableId {
            Mode::spawn_named(world, "combatant", pos, team, parts)
        }

        /// A `caster` of `team` with `parts`.
        pub(crate) fn spawn_caster(
            world: &mut World,
            pos: impl Into<Position>,
            team: Team,
            parts: impl Bundle,
        ) -> StableId {
            Mode::spawn_named(world, "caster", pos, team, parts)
        }

        /// A unit of `team` of the armed type `unit_type` that attacks `target`.
        pub(crate) fn attacker(
            world: &mut World,
            unit_type: UnitType,
            pos: impl Into<Position>,
            team: Team,
            target: StableId,
        ) -> StableId {
            let id = Mode::spawn(world, unit_type, pos, team, ());
            let mut slots = world.part_mut::<ActionSlots>(id);
            slots.set_attack_target(Some(target));
            id
        }

        /// Player 0's `caster` at the origin, on team 0, with `action` at `rank`.
        pub(crate) fn caster(world: &mut World, action: ActionId, rank: u8) -> StableId {
            Abilities::caster_of(world, "caster", action, rank)
        }

        /// Player 0's unit of the caster type `name` at the origin, on team 0, with `action` at
        /// `rank`.
        fn caster_of(world: &mut World, name: &str, action: ActionId, rank: u8) -> StableId {
            let parts = (
                Owner::new(PlayerSlot::new(0)),
                ActionSlots::new([(action, SlotKind::new(0), Rank::new(rank))]).timed(),
            );
            Mode::spawn_named(world, name, Position::ORIGIN, Team::new(0), parts)
        }

        pub(crate) fn cast(world: &mut World, id: StableId, target: ActionTarget) {
            Abilities::casts(world, &[(id, target)]);
        }

        /// Runs a tick in which each unit casts its first slot's ability at its target, as an order
        /// would make it.
        pub(crate) fn casts(world: &mut World, casts: &[(StableId, ActionTarget)]) {
            for &(id, target) in casts {
                let mut slots = world.part_mut::<ActionSlots>(id);
                slots.order(0, target);
            }
            world.step();
        }

        pub(crate) fn pool(world: &World, id: StableId) -> i64 {
            Abilities::pool_of(world, id, MANA)
        }

        pub(crate) fn pool_of(world: &World, id: StableId, pool: PoolId) -> i64 {
            world
                .part::<Pools>(id)
                .current(pool)
                .unwrap()
                .to_int()
                .expect("a whole amount")
        }

        pub(crate) fn slot(world: &World, id: StableId) -> ActionSlot {
            world.part::<ActionSlots>(id).slot(0).unwrap()
        }

        pub(crate) fn time(world: &World, id: StableId) -> SlotTime {
            world.part::<SlotTimes>(id).slot(0).unwrap()
        }

        pub(crate) fn casting(world: &World, id: StableId) -> Option<InProgress> {
            let entity = world.unit_entity(id);
            let slots = world.get::<ActionSlots>(entity).unwrap();
            slots
                .in_progress()
                .filter(|underway| matches!(underway, InProgress::Order { .. }))
        }

        pub(crate) fn failed_calls(world: &World) -> Vec<FailedCall> {
            world.non_send::<ScriptFailures>().calls()
        }

        /// Gives `id` the modifier `name` from itself.
        pub(crate) fn give(world: &mut World, id: StableId, name: &str) {
            Abilities::give_from(world, id, id, name);
        }

        /// Gives `id` the modifier `name` of package 0, from `source`.
        pub(crate) fn give_from(world: &mut World, id: StableId, source: StableId, name: &str) {
            let modifier = Stats::modifier(world, 0, name).unwrap();
            let from = Some((source, None, 1));
            Stats::give_modifier(world, id, modifier, from, false);
        }

        /// Each failed call of the tick: the unit it ran for and its hook.
        pub(crate) fn calls(world: &World) -> Vec<(StableId, Hook)> {
            let failures = Abilities::failed_calls(world).into_iter();
            failures
                .map(|failure| (failure.id.unwrap(), failure.hook))
                .collect()
        }

        /// Loads `data` as the weapon `name` of package 0, of one rank, with its effect lists.
        pub(crate) fn load_weapon(world: &mut World, name: &str, data: &ActionData) -> ActionId {
            let weapon = Actions::load(world, 0, name, data, None, 1).unwrap();
            Actions::load_effect_lists(world, weapon, 0, data);
            weapon
        }

        /// A `fighter`, a combatant's type, of team 0 at the origin, with `slots`.
        pub(crate) fn fighter(world: &mut World, fighter: UnitType, slots: TimedSlots) -> StableId {
            Mode::spawn(world, fighter, Position::ORIGIN, Team::new(0), slots)
        }

        /// The shield of `id`'s `mark` from `source`, if it carries one.
        pub(crate) fn mark(world: &World, id: StableId, source: StableId) -> Option<Num> {
            let mark = Stats::modifier(world, 0, "mark").unwrap();
            let entity = world.unit_entity(id);
            let modifiers = world.get::<Modifiers>(entity).unwrap();
            let clocks = world.get::<ModifierClocks>(entity).unwrap();
            modifiers.get(mark, Some(source))?;
            clocks.shield_of(modifiers, mark, Some(source))
        }

        /// Player 0's `walking caster` at the origin with `action`.
        pub(crate) fn walking_caster(world: &mut World, action: ActionId) -> StableId {
            Abilities::caster_of(world, "walking caster", action, 1)
        }
    }
}

#[cfg(test)]
mod tests;
