#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use campfire_actions::{ActionTarget, CastSpends, SlotKind, UseGroup};
use campfire_common::Ticks;
use campfire_core::{
    Block, Declarations, InstallCx, Module, ModuleSet, Phase, ScriptValue, SetPlace, UnitTags, View,
};
use campfire_deliveries::Deliverers;
use campfire_sim::{Capability, TickRate};

mod abilities_api;
mod abilities_effect;
mod casts;
mod channels;
mod toggles;

pub use crate::abilities_api::AbilitiesApi;
pub use crate::casts::Casts;
pub use crate::channels::Channels;
pub use crate::toggles::Toggles;

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
        /// The due casts start.
        StartCasts,
        /// After attacks strike and before the tick's projectiles launch: due casts resolve.
        Resolve,
    }
    /// Every step.
    pub const ALL;
}

impl ModuleSet for AbilitiesSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Abilities);
    const ALL: &'static [AbilitiesSet] = &AbilitiesSet::ALL;

    fn place(self) -> SetPlace {
        SetPlace::Phase(match self {
            AbilitiesSet::Toggles => Phase::Toggles,
            AbilitiesSet::StartCasts => Phase::Start,
            AbilitiesSet::Resolve => Phase::CastResolve,
        })
    }
}

impl Module for Abilities {
    const CAPABILITY: Option<Capability> = Some(Capability::Abilities);
    const NEEDS: &'static [Capability] = &[Capability::Stats];
    const USES: &'static [Capability] = &[];

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
        schedule.add_systems(Casts::start_casts.in_set(AbilitiesSet::StartCasts));
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
    fn target(view: &View, target: ActionTarget) -> ScriptValue {
        match target {
            ActionTarget::None => ScriptValue::UNIT,
            ActionTarget::Unit(id) => view
                .living(id)
                .and_then(|_| view.unit(id))
                .map_or(ScriptValue::UNIT, ScriptValue::of),
            ActionTarget::Point(at) => ScriptValue::of(at),
        }
    }

    /// Whether a unit with `tags`, under a forced move when `forced`, of the use group `used`, is
    /// kept from the action of a slot of `kind`: from using it, for a slot of its use group, else,
    /// and with no slot, from casting it.
    fn blocked(
        tags: Option<&UnitTags>,
        forced: bool,
        used: Option<&UseGroup>,
        kind: Option<SlotKind>,
    ) -> bool {
        let group = kind.map_or(Block::Cast, |kind| UseGroup::group(used, kind));
        UnitTags::blocks(tags, forced, group)
    }

    /// A second at `rate`, the time a toggle pays its cost each.
    const fn second(rate: TickRate) -> Ticks {
        rate.ticks(1000)
            .expect("a second counts in ticks at every rate")
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use std::collections::BTreeMap;

    use bevy_ecs::world::World;
    use campfire_actions::{
        ActionSlot, ActionSlots, ActionTarget, InProgress, SlotTime, SlotTimes,
    };
    use campfire_core::internals::FailedCall;
    use campfire_core::{Hook, ScriptFailures};
    use campfire_math::Num;
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{SimWorld, StableId};
    use campfire_stats::{ModifierClocks, Modifiers, PoolId, Pools, StatRule, Stats};
    use campfire_values::Stat;

    use crate::Abilities;

    /// Mana, the second of the match's pools, after the life pool.
    pub const MANA: PoolId = PoolId::new(1).unwrap();

    /// Rage, the third of the match's pools.
    pub const RAGE: PoolId = PoolId::new(2).unwrap();

    /// The stats of the scaling tests' mode, in its order: attack damage, an engine stat, first.
    pub fn scaling_stats() -> [Stat; 4] {
        let mut stats = ["attack_damage", "ability_power", "armor", "spell_vamp"]
            .map(|name| Stat::named(name).unwrap());
        stats.sort();
        stats
    }

    impl Abilities {
        /// Gives the match a stat book of the stats the scaling params name, with no unit type.
        pub fn load_stats(world: &mut World) {
            let rules = scaling_stats().map(|stat| (stat, StatRule::default()));
            Stats::load_rules(world, &BTreeMap::from(rules));
        }

        pub fn cast(world: &mut World, id: StableId, target: ActionTarget) {
            Abilities::casts(world, &[(id, target)]);
        }

        /// Runs a tick in which each unit casts its first slot's ability at its target, as an order
        /// would make it.
        pub fn casts(world: &mut World, casts: &[(StableId, ActionTarget)]) {
            for &(id, target) in casts {
                let mut slots = world.part_mut::<ActionSlots>(id);
                slots.order(0, target);
            }
            world.step();
        }

        pub fn pool(world: &World, id: StableId) -> i64 {
            Abilities::pool_of(world, id, MANA)
        }

        pub fn pool_of(world: &World, id: StableId, pool: PoolId) -> i64 {
            world
                .part::<Pools>(id)
                .current(pool)
                .unwrap()
                .to_int()
                .expect("a whole amount")
        }

        pub fn slot(world: &World, id: StableId) -> ActionSlot {
            world.part::<ActionSlots>(id).slot(0).unwrap()
        }

        pub fn time(world: &World, id: StableId) -> SlotTime {
            world.part::<SlotTimes>(id).slot(0).unwrap()
        }

        pub fn casting(world: &World, id: StableId) -> Option<InProgress> {
            let entity = world.unit_entity(id);
            let slots = world.get::<ActionSlots>(entity).unwrap();
            slots
                .in_progress()
                .filter(|underway| matches!(underway, InProgress::Order { .. }))
        }

        pub fn failed_calls(world: &World) -> Vec<FailedCall> {
            world.non_send::<ScriptFailures>().calls()
        }

        /// Gives `id` the modifier `name` from itself.
        pub fn give(world: &mut World, id: StableId, name: &str) {
            Abilities::give_from(world, id, id, name);
        }

        /// Gives `id` the modifier `name` of package 0, from `source`.
        pub fn give_from(world: &mut World, id: StableId, source: StableId, name: &str) {
            let modifier = Stats::modifier(world, 0, name).unwrap();
            let from = Some((source, None, 1));
            Stats::give_modifier(world, id, modifier, from, false);
        }

        /// Each failed call of the tick: the unit it ran for and its hook.
        pub fn calls(world: &World) -> Vec<(StableId, Hook)> {
            let failures = Abilities::failed_calls(world).into_iter();
            failures
                .map(|failure| (failure.id.unwrap(), failure.hook))
                .collect()
        }

        /// The shield of `id`'s `mark` from `source`, if it carries one.
        pub fn mark(world: &World, id: StableId, source: StableId) -> Option<Num> {
            let mark = Stats::modifier(world, 0, "mark").unwrap();
            let entity = world.unit_entity(id);
            let modifiers = world.get::<Modifiers>(entity).unwrap();
            let clocks = world.get::<ModifierClocks>(entity).unwrap();
            modifiers.get(mark, Some(source))?;
            clocks.shield_of(modifiers, mark, Some(source))
        }
    }
}
