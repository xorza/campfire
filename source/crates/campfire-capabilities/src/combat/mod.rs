use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::world::World;
use campfire_math::RngStream;
use campfire_sim::{Capability, SimSet, StateRegistry};

use crate::actions::effect_queues::EffectQueues;
use crate::actions::{Actions, ActionsSet};
use crate::combat::attacks::Attacks;
use crate::combat::combat_column::{CombatColumn, RowParts};
use crate::combat::combat_effect::CombatEffect;
use crate::combat::combat_event::CombatEvents;
use crate::combat::damage_pass::DamagePass;
use crate::combat::deaths::Deaths;
use crate::combat::dying::Dying;
use crate::combat::kept::Kept;
use crate::combat::modifier_hooks::ModifierHooks;
use crate::combat::modifier_intervals::ModifierIntervals;
use crate::combat::on_death::OnDeath;
use crate::combat::pass_queue::PassQueue;
use crate::combat::recent_attackers::RecentAttackers;
use crate::combat::respawn::Respawn;
use crate::combat::shots::Shots;
use crate::scripts::ctx::Ctx;
use crate::stats::StatsSet;
use crate::stats::life_pool::LifePool;
use crate::stats::pool_id::PoolId;
use crate::units::view::View;

pub(crate) mod assist_window;
pub(crate) mod attacks;
pub(crate) mod combat_api;
pub(crate) mod combat_bindings;
pub(crate) mod combat_column;
pub(crate) mod combat_data;
pub(crate) mod combat_effect;
pub(crate) mod combat_event;
pub(crate) mod combat_rules;
pub(crate) mod damage;
pub(crate) mod damage_handle;
pub(crate) mod damage_pass;
pub(crate) mod deaths;
pub(crate) mod dying;
pub(crate) mod going_off;
pub(crate) mod heal;
pub(crate) mod heal_handle;
pub(crate) mod interval_due;
pub(crate) mod kept;
pub(crate) mod modifier_hooks;
pub(crate) mod modifier_intervals;
pub(crate) mod on_death;
pub(crate) mod pass_queue;
pub(crate) mod recent_attack;
pub(crate) mod recent_attackers;
pub(crate) mod respawn;
pub(crate) mod shots;
pub(crate) mod wielded;

/// The random stream an attack's roll draws from, for its attacker in its tick.
pub(crate) const ROLL_STREAM: RngStream = RngStream::new("combat.roll");

/// The `combat` capability: teams, the life pool, attacks, damage and deaths.
#[derive(Debug)]
pub struct Combat;

/// Combat's systems, for the capabilities built on it to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CombatSet {
    /// In `SimSet::Inputs`: dead units whose respawn is due come back, before any input of the
    /// tick can reach them.
    Respawn,
    /// In `SimSet::Act`: attacks start, and targets that are gone are dropped.
    Attack,
    /// In `SimSet::Hit`: the attacks going off pay the toggles that cost an attack.
    Pay,
    /// In `SimSet::Hit`, after `Pay`: windups that end strike, or fire, each with its roll drawn.
    Strike,
    /// In `SimSet::Hit`, after `Strike`: the tick's shots become launches, before any cast
    /// delivers.
    Fire,
    /// In `SimSet::Hit`, after `Fire`: the tick's launches take off.
    Launch,
    /// In `SimSet::Hit`, after `Launch`: modifiers' intervals come.
    Interval,
    /// In `SimSet::Resolve`: the tick's damage is dealt.
    Damage,
    /// In `SimSet::Resolve`, after `Damage`: units at zero life die.
    Die,
}

impl Combat {
    /// Adds combat to a match: in Inputs, dead units whose respawn is due come back; in Act,
    /// attacks in range start once ready; in Hit, windups that end strike, or fire when ranged
    /// and the match has projectiles; in Resolve, the tick's damage is dealt, then units at zero
    /// life die, each with its killer and assisters; in Vision, the dead whose type despawns
    /// go, after the Mode stage saw them.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        let view = world.non_send::<View>().clone();
        view.add_column(CombatColumn::default());
        view.add_source::<RowParts, _>(world, CombatColumn::fill_row);
        world.insert_resource(PassQueue::default());
        world.insert_resource(Shots::default());
        world.insert_resource(Deaths::default());
        world
            .resource_mut::<EffectQueues>()
            .register(Capability::Combat, CombatEffect::queue_listed);
        if let Some(ctx) = world.get_non_send::<Ctx>().cloned() {
            let hooks = ModifierHooks::new(ctx);
            world.insert_non_send(CombatEvents::new(move |batch, event| {
                hooks.hear(batch, event);
            }));
        }
        schedule.configure_sets((
            CombatSet::Fire.in_set(SimSet::Hit).after(CombatSet::Strike),
            CombatSet::Launch.in_set(SimSet::Hit).after(CombatSet::Fire),
            CombatSet::Die.before(StatsSet::Hold),
            CombatSet::Attack
                .in_set(SimSet::Act)
                .before(ActionsSet::Start),
            ActionsSet::HoldAtResolve
                .after(CombatSet::Damage)
                .before(CombatSet::Die),
            CombatSet::Pay.in_set(SimSet::Hit).before(CombatSet::Strike),
            ActionsSet::HoldAtStrike
                .after(CombatSet::Pay)
                .before(CombatSet::Strike),
        ));
        Actions::schedule(schedule);
        schedule.add_systems((
            Attacks::start_attacks.in_set(CombatSet::Attack),
            Attacks::pay_attack_toggles.in_set(CombatSet::Pay),
            (Attacks::attack_events, Attacks::strike)
                .chain()
                .in_set(SimSet::Hit)
                .in_set(CombatSet::Strike),
            ModifierIntervals::run_intervals
                .in_set(SimSet::Hit)
                .in_set(CombatSet::Interval)
                .after(CombatSet::Launch),
            (
                DamagePass::run.in_set(CombatSet::Damage),
                Dying::die.in_set(CombatSet::Die),
            )
                .chain()
                .in_set(SimSet::Resolve),
            Dying::respawn
                .in_set(SimSet::Inputs)
                .in_set(CombatSet::Respawn),
            Dying::despawn_dead.in_set(SimSet::Vision),
        ));
        registry.register_component::<Kept>();
        registry.register_component::<OnDeath>();
        registry.register_component::<RecentAttackers>();
        registry.register_component::<Respawn>();
    }

    /// The match's life pool; `None` when its mode names none.
    pub fn life(world: &World) -> Option<PoolId> {
        Some(world.get_resource::<LifePool>()?.0)
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {

    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::world::World;
    use campfire_common::Ticks;
    use campfire_math::Num;
    use campfire_sim::TickRate;

    #[cfg(test)]
    use crate::combat::on_death::OnDeath;
    #[cfg(test)]
    use crate::combat::recent_attackers::RecentAttackers;
    #[cfg(test)]
    use crate::stats::pools::Pools;
    #[cfg(test)]
    use crate::units::team::Team;
    use crate::values::rank::Rank;

    use campfire_sim::StableId;

    use crate::actions::action_book::internals::{self, TestWeapon};
    use crate::actions::action_range::ActionRange;
    use crate::actions::action_slots::ActionSlots;
    use crate::actions::slot_kind::SlotKind;
    #[cfg(test)]
    use crate::combat::combat_bindings::CombatBindings;
    use crate::combat::damage::{Damage, DamageCause};
    use crate::combat::pass_queue::PassQueue;
    #[cfg(test)]
    use crate::stats::life_pool::LifePool;
    use crate::units::view::View;

    #[cfg(test)]
    use crate::stats::pool_id::PoolId;
    use crate::stats::stat_book::StatBook;
    use crate::stats::unit_stats::UnitStats;
    use crate::units::filter::Filter;
    use crate::units::unit_type::UnitType;
    use crate::values::relation_set::RelationSet;

    /// Queues `amount` of the damage kind `kind` from `source` to `target` for the tick's damage
    /// pass, as an effect of no action deals it: for a test that deals what no input can.
    pub fn queue_damage(
        world: &mut World,
        source: Option<StableId>,
        target: StableId,
        amount: Num,
        kind: &str,
    ) {
        let kind = world
            .non_send::<View>()
            .damage_kind_named(kind)
            .unwrap_or_else(|| panic!("the mode declares the damage kind {kind}"));
        world.resource_mut::<PassQueue>().push_damage(Damage {
            source,
            target,
            amount,
            kind,
            cause: DamageCause::Effect,
            ability: None,
            depth: 0,
            hit: None,
        });
    }

    /// A test unit's weapon: it aims at enemies within `range`, winds up `windup`, may attack
    /// again `period` after an attack's start, deals `damage`, and fires a projectile of the
    /// homing type `projectile` when given.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Arms {
        range: Num,
        windup: Ticks,
        period: Ticks,
        damage: Num,
        projectile: Option<UnitType>,
    }

    /// A unit's slots and stats: its weapon's slot, if it has one, and the stats it reads.
    #[derive(Debug, Bundle)]
    pub struct ArmsParts {
        slots: ActionSlots,
        stats: UnitStats,
    }

    impl ArmsParts {
        /// The parts of a unit with no weapon: no slot, and no stats.
        pub fn unarmed() -> ArmsParts {
            ArmsParts {
                slots: ActionSlots::new([]),
                stats: UnitStats::default(),
            }
        }
    }

    impl Arms {
        /// A melee weapon of `range`, `windup` ticks, `period` ticks and `damage`.
        pub const fn melee(range: Num, windup: u64, period: u64, damage: Num) -> Arms {
            Arms {
                range,
                windup: Ticks::new(windup),
                period: Ticks::new(period),
                damage,
                projectile: None,
            }
        }

        /// The same weapon, firing projectiles of the homing type `projectile`.
        #[must_use]
        pub const fn ranged(self, projectile: UnitType) -> Arms {
            Arms {
                projectile: Some(projectile),
                ..self
            }
        }

        /// The weapon added to the book of `world`, in a unit's one slot, and the stats it reads:
        /// the rate that makes its period at the match's rate, then its damage, then 0 for every
        /// other stat of the match's stat book.
        pub fn parts(self, world: &mut World) -> ArmsParts {
            let hz = world.resource::<TickRate>().hz().get();
            let weapon = TestWeapon {
                projectile: self.projectile,
                ..TestWeapon::new(
                    Filter::of_relations(RelationSet::Enemies),
                    ActionRange::Meters(self.range),
                    self.windup,
                )
            };
            let id = internals::weapon(world, weapon);
            // The rate whose attacks are `period` ticks apart, rounded up so the period
            // rounds back to `period`.
            let bits = (u128::from(hz) << (2 * Num::FRAC_BITS))
                .div_ceil(u128::from(self.period.get()) << Num::FRAC_BITS);
            let rate = Num::from_bits(i64::try_from(bits).unwrap());
            // One value for each stat of the match's book, as a refresh gives a unit, the rest 0.
            let count = world
                .get_resource::<StatBook>()
                .map_or(2, |book| usize::from(book.len()));
            let mut stats = UnitStats::default();
            let values = stats.refill();
            values.extend([rate, self.damage]);
            values.resize(count.max(2), Num::ZERO);
            ArmsParts {
                slots: ActionSlots::new([(id, SlotKind::new(0), Some(Rank::FIRST))]),
                stats,
            }
        }
    }

    /// Binds `life` as the life pool, with no stat bound, as a test world with no mode needs.
    #[cfg(test)]
    pub(crate) fn bind_life(world: &mut World, life: PoolId) {
        world.insert_resource(LifePool(life));
        world.insert_resource(CombatBindings::UNBOUND);
    }

    /// A test unit's combat values: the life pool it starts with, whether it stays when it dies,
    /// and its one weapon, if it has one.
    #[cfg(test)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct Armed {
        pub(crate) life: Num,
        pub(crate) on_death: OnDeath,
        pub(crate) arms: Option<Arms>,
    }

    #[cfg(test)]
    impl Armed {
        /// A unit of `life` that despawns when it dies, with a melee weapon of `range`, `windup`
        /// ticks, `period` ticks and `damage`.
        pub(crate) fn melee(life: Num, range: Num, windup: u64, period: u64, damage: Num) -> Armed {
            Armed {
                life,
                on_death: OnDeath::Despawn,
                arms: Some(Arms::melee(range, windup, period, damage)),
            }
        }

        /// A unit of `life` with no weapon, that despawns when it dies.
        pub(crate) const fn unarmed(life: Num) -> Armed {
            Armed {
                life,
                on_death: OnDeath::Despawn,
                arms: None,
            }
        }

        /// The same unit, which `on_death` says whether it stays when it dies.
        pub(crate) const fn on_death(self, on_death: OnDeath) -> Armed {
            Armed { on_death, ..self }
        }

        /// The same unit, its weapon firing projectiles of the homing type `projectile`.
        pub(crate) fn ranged(self, projectile: UnitType) -> Armed {
            let arms = self
                .arms
                .expect("a ranged unit is armed")
                .ranged(projectile);
            Armed {
                arms: Some(arms),
                ..self
            }
        }

        /// The components of a new unit of this type on `team` in `world`, whose book takes its
        /// weapon: its team and its life pool, which the spawn and its kit give; its death and
        /// its attackers, which its `combat` gives; and its weapon's parts.
        pub(crate) fn bundle(self, world: &mut World, team: Team) -> impl Bundle + use<> {
            let parts = self
                .arms
                .map_or_else(ArmsParts::unarmed, |arms| arms.parts(world));
            (
                team,
                Pools::life(self.life),
                self.on_death,
                RecentAttackers::default(),
                parts,
            )
        }
    }
}

#[cfg(test)]
mod tests;
