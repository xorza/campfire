use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use campfire_math::RngStream;
use campfire_sim::{Capability, SimPlace, SimSet};

use crate::actions::ActionsSet;
use crate::actions::effect_queues::EffectQueues;
use crate::combat::attacks::Attacks;
use crate::combat::combat_api::CombatApi;
use crate::combat::combat_book::CombatBook;
use crate::combat::combat_column::{CombatColumn, RowReads};
use crate::combat::combat_effect::CombatEffect;
use crate::combat::combat_event::CombatEvents;
use crate::combat::combat_hooks::CombatHooks;
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
use crate::declare::declarations::Declarations;
use crate::declare::install_cx::InstallCx;
use crate::declare::module::Module;
use crate::declare::module_set::ModuleSet;
use crate::declare::set_order::SetOrder;
use crate::deliveries::DeliverySet;
use crate::scripts::ctx::Ctx;
use crate::stats::StatsSet;
use crate::stats::life::Life;
use crate::units::body_unit::BodyUnit;
use crate::units::by_type::ByType;
use crate::units::dead::Dead;
use crate::units::spawn_point::SpawnPoint;

pub(crate) mod attacks;
pub(crate) mod combat_api;
pub(crate) mod combat_bindings;
pub(crate) mod combat_book;
pub(crate) mod combat_column;
pub(crate) mod combat_data;
pub(crate) mod combat_effect;
pub(crate) mod combat_event;
pub(crate) mod combat_hooks;
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
pub(crate) mod mode_combat_data;
pub(crate) mod modifier_hooks;
pub(crate) mod modifier_intervals;
pub(crate) mod on_death;
pub(crate) mod pass_queue;
pub(crate) mod recent_attack;
pub(crate) mod recent_attackers;
pub(crate) mod respawn;
pub(crate) mod wielded;

/// The random stream an attack's roll draws from, for its attacker in its tick.
pub(crate) const ROLL_STREAM: RngStream = RngStream::new("combat.roll");

/// The `combat` capability: teams, the life pool, attacks, damage and deaths.
#[derive(Debug)]
pub struct Combat;

campfire_common::enum_all! {
    /// Combat's steps, for the capabilities built on it to order theirs against.
    #[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) enum CombatSet {
        /// Dead units whose respawn is due come back, before any input of the tick can reach
        /// them.
        Respawn,
        /// Attacks start, and targets that are gone are dropped.
        Attack,
        /// After the projectiles fly and before areas trigger: the hits of the attacks'
        /// projectiles queue their damage.
        Hits,
        /// The attacks going off pay the toggles that cost an attack.
        Pay,
        /// Windups that end strike, or fire, each with its roll drawn.
        Strike,
        /// Modifiers' intervals come.
        Interval,
        /// The tick's damage is dealt.
        Damage,
        /// Units at zero life die.
        Die,
        /// The dead whose type despawns go, after the Mode stage saw them.
        Despawn,
    }
    /// Every step.
    pub(crate) const ALL;
}

impl ModuleSet for CombatSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Combat);
    const ALL: &'static [CombatSet] = &CombatSet::ALL;

    fn place(self) -> SimPlace {
        SimPlace::Stage(match self {
            CombatSet::Respawn => SimSet::Inputs,
            CombatSet::Attack => SimSet::Act,
            CombatSet::Hits | CombatSet::Pay | CombatSet::Strike | CombatSet::Interval => {
                SimSet::Hit
            }
            CombatSet::Damage | CombatSet::Die => SimSet::Resolve,
            CombatSet::Despawn => SimSet::Vision,
        })
    }

    fn order(self, o: &mut SetOrder) {
        match self {
            CombatSet::Respawn | CombatSet::Despawn => {}
            CombatSet::Attack => {
                o.before(ActionsSet::Start);
            }
            CombatSet::Hits => {
                o.after(DeliverySet::Fly)
                    .before(DeliverySet::Trigger)
                    .before(ActionsSet::Deliver);
            }
            CombatSet::Pay => {
                o.after(ActionsSet::Deliver)
                    .before(ActionsSet::HoldAtStrike);
            }
            CombatSet::Strike => {
                o.after(CombatSet::Pay)
                    .after(StatsSet::GrantAtStrike)
                    .before(ActionsSet::Fire);
            }
            CombatSet::Interval => {
                o.after(ActionsSet::Land);
            }
            CombatSet::Damage => {
                o.before(ActionsSet::Resolve);
            }
            CombatSet::Die => {
                o.after(StatsSet::GrantAtResolve)
                    .before(StatsSet::ClearDead);
            }
        }
    }
}

impl Module for Combat {
    const CAPABILITY: Option<Capability> = Some(Capability::Combat);
    const NEEDS: &'static [Capability] = &[Capability::Stats];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<CombatSet>();
        d.component::<Kept>();
        d.component::<Life>();
        d.component::<OnDeath>();
        d.component::<RecentAttackers>();
        d.component::<Respawn>();
        d.scratch::<PassQueue>();
        d.report::<Deaths>();
        d.book::<CombatBook>();
        d.callback::<EffectQueues>(Capability::Combat, CombatEffect::queue_listed);
        d.require::<Respawn, Dead>();
        d.require::<Respawn, SpawnPoint>();
        d.column::<CombatColumn, RowReads>(CombatColumn::fill_row);
        d.api(CombatApi::register);
        d.book::<ByType<Life>>();
        d.book::<ByType<OnDeath>>();
        d.part::<BodyUnit, Life>(|at| at.of_type::<Life>().copied());
        d.part::<BodyUnit, OnDeath>(|at| at.of_type::<OnDeath>().copied());
        // A unit that can die keeps who struck it, for its death's assisters.
        d.part::<BodyUnit, RecentAttackers>(|at| {
            at.of_type::<OnDeath>().map(|_| RecentAttackers::default())
        });
    }

    /// Adds combat to a match: in Inputs, dead units whose respawn is due come back; in Act,
    /// attacks in range start once ready; in Hit, windups that end strike, or fire when ranged
    /// and the match has projectiles; in Resolve, the tick's damage is dealt, then units at zero
    /// life die, each with its killer and assisters; in Vision, the dead whose type despawns
    /// go, after the Mode stage saw them. A unit's despawn and its attackers are the server's: a
    /// predicting client despawns no unit, and forgets no attacker it never held.
    fn install(cx: &mut InstallCx<'_>) {
        let scripts = cx.scripts();
        let predicting = cx.predicting();
        let (world, schedule) = (&mut *cx.world, &mut *cx.schedule);
        world.insert_resource(Deaths::default());
        let events = scripts.then(|| {
            let hooks = ModifierHooks::new(world.non_send::<Ctx>().clone());
            CombatEvents::new(move |batch, event| hooks.hear(batch, event))
        });
        world.insert_non_send(CombatHooks {
            events,
            ..CombatHooks::default()
        });
        schedule.add_systems((
            Attacks::start_attacks.in_set(CombatSet::Attack),
            Attacks::take_hits.in_set(CombatSet::Hits),
            Attacks::pay_attack_toggles.in_set(CombatSet::Pay),
            (Attacks::attack_events, Attacks::strike)
                .chain()
                .in_set(CombatSet::Strike),
            ModifierIntervals::run_intervals.in_set(CombatSet::Interval),
            DamagePass::run.in_set(CombatSet::Damage),
            Dying::die.in_set(CombatSet::Die),
        ));
        if predicting {
            schedule.add_systems(Dying::respawn.in_set(CombatSet::Respawn));
        } else {
            schedule.add_systems((
                (Dying::forget_attackers, Dying::respawn)
                    .chain()
                    .in_set(CombatSet::Respawn),
                Dying::despawn_dead.in_set(CombatSet::Despawn),
            ));
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_math::Num;
    use campfire_sim::StableId;

    use crate::combat::Combat;
    use crate::combat::damage::{Damage, DamageCause};
    use crate::combat::pass_queue::PassQueue;
    use crate::units::view::View;

    impl Combat {
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
                .names()
                .damage_kinds
                .named(kind)
                .unwrap_or_else(|| panic!("the mode declares the damage kind {kind}"));
            world.resource_mut::<PassQueue>().push_damage(Damage {
                source,
                target,
                amount,
                kind,
                cause: DamageCause::Effect,
                action: None,
                depth: 0,
                hit: None,
            });
        }
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::world::World;
    use campfire_common::{Tick, Ticks};
    use campfire_math::{Num, Rounding};
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{Position, StableId, TickRate};

    use crate::actions::Actions;
    use crate::actions::action_book::ActionBook;
    use crate::actions::action_range::ActionRange;
    use crate::actions::action_slots::ActionSlots;
    use crate::actions::in_progress::InProgress;
    use crate::actions::slot_kind::SlotKind;
    use crate::actions::slot_times::{SlotTimes, TimedSlots};
    use crate::actions::test_world::TestWeapon;
    use crate::combat::Combat;
    use crate::combat::combat_bindings::CombatBindings;
    use crate::combat::combat_book::CombatBook;
    use crate::combat::damage::{Damage, DamageCause};
    use crate::combat::damage_pass::DamagePass;
    use crate::combat::on_death::OnDeath;
    use crate::combat::pass_queue::PassQueue;
    use crate::combat::recent_attackers::RecentAttackers;
    use crate::stats::life::Life;
    use crate::stats::pool_book::PoolBook;
    use crate::stats::pool_id::PoolId;
    use crate::stats::stat_book::StatBook;
    use crate::stats::unit_stats::UnitStats;
    use crate::units::Units;
    use crate::units::filter::Filter;
    use crate::units::team::Team;
    use crate::units::unit_type::UnitType;
    use crate::values::damage_kind::DamageKind;
    use crate::values::rank::Rank;
    use crate::values::relation_set::RelationSet;

    /// A test unit's weapon: it aims at enemies within `range`, winds up `windup`, may attack
    /// again `period` after an attack's start, deals `damage`, and fires a projectile of the
    /// homing type `projectile` when given.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct Arms {
        range: Num,
        windup: Ticks,
        period: Ticks,
        damage: Num,
        projectile: Option<UnitType>,
    }

    /// A unit's slots and stats: its weapon's slot, if it has one, and the stats it reads.
    #[derive(Debug, Bundle)]
    pub(crate) struct ArmsParts {
        slots: TimedSlots,
        stats: UnitStats,
    }

    impl ArmsParts {
        /// The parts of a unit with no weapon: no slot, and no stats.
        pub(crate) fn unarmed() -> ArmsParts {
            ArmsParts {
                slots: ActionSlots::new([]).timed(),
                stats: UnitStats::default(),
            }
        }
    }

    impl Arms {
        /// A melee weapon of `range`, `windup` ticks, `period` ticks and `damage`.
        pub(crate) const fn melee(range: Num, windup: u64, period: u64, damage: Num) -> Arms {
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
        pub(crate) const fn ranged(self, projectile: UnitType) -> Arms {
            Arms {
                projectile: Some(projectile),
                ..self
            }
        }

        /// The weapon added to the book of `world`, in a unit's one slot, and the stats it reads:
        /// the rate that makes its period at the match's rate, then its damage, then 0 for every
        /// other stat of the match's stat book.
        pub(crate) fn parts(self, world: &mut World) -> ArmsParts {
            let hz = world.resource::<TickRate>().hz().get();
            let weapon = TestWeapon {
                projectile: self.projectile,
                ..TestWeapon::new(
                    Filter::of_release_relations(RelationSet::Enemies),
                    ActionRange::Meters(self.range),
                    self.windup,
                )
            };
            let id = Actions::add_weapon(world, weapon);
            // The rate whose attacks are `period` ticks apart, rounded up so the period
            // rounds back to `period`.
            let period = Num::from_int(i64::try_from(self.period.get()).unwrap()).unwrap();
            let rate = Num::int(i64::from(hz))
                .checked_div_rounded(period, Rounding::Ceiling)
                .unwrap();
            // One value for each stat of the match's book, as a refresh gives a unit, the rest 0.
            let count = world
                .get_resource::<StatBook>()
                .map_or(2, |book| usize::from(book.len()));
            let mut stats = UnitStats::default();
            let values = stats.refill();
            values.extend([rate, self.damage]);
            values.resize(count.max(2), Num::ZERO);
            ArmsParts {
                slots: ActionSlots::new([(id, SlotKind::new(0), Some(Rank::FIRST))]).timed(),
                stats,
            }
        }
    }

    /// A test unit's combat values: the life pool it starts with, whether it stays when it dies,
    /// and its one weapon, if it has one.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct Armed {
        pub(crate) life: Num,
        pub(crate) on_death: OnDeath,
        pub(crate) arms: Option<Arms>,
    }

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

        /// The components of a new unit of this type in `world`, whose book takes its weapon: its
        /// life pool, its death and its attackers, which combat declares for a unit, and its
        /// weapon's parts.
        pub(crate) fn bundle(self, world: &mut World) -> impl Bundle + use<> {
            let parts = self
                .arms
                .map_or_else(ArmsParts::unarmed, |arms| arms.parts(world));
            (
                Life::full(self.life),
                self.on_death,
                RecentAttackers::default(),
                parts,
            )
        }
    }

    impl Combat {
        /// Heals `id` at once, as the pass heals it, when it exists and its life is above zero.
        pub(crate) fn heal_unit(world: &mut World, id: StableId, amount: impl Into<Num>) {
            let amount = amount.into();
            if let Some(entity) = DamagePass::living(world, id) {
                DamagePass::heal(world, entity, amount);
            }
        }

        /// Binds `life` as the life pool, with no stat bound, as a test world with no mode needs.
        pub(crate) fn bind_life(world: &mut World, life: PoolId) {
            world.resource_mut::<PoolBook>().set_life(Some(life));
            world.resource_mut::<CombatBook>().bindings = CombatBindings::default();
        }

        pub(crate) fn spawn_armed(
            world: &mut World,
            pos: impl Into<Position>,
            team: Team,
            combatant: Armed,
        ) -> StableId {
            let pos = pos.into();
            let unit_type = Units::test_type(world);
            Combat::spawn_armed_of(world, unit_type, pos, team, combatant)
        }

        pub(crate) fn spawn_armed_of(
            world: &mut World,
            unit_type: UnitType,
            pos: impl Into<Position>,
            team: Team,
            combatant: Armed,
        ) -> StableId {
            let pos = pos.into();
            let bundle = combatant.bundle(world);
            Units::spawn_of(world, unit_type, pos, team, bundle)
        }

        pub(crate) fn attack(world: &mut World, attacker: StableId, target: StableId) {
            let entity = world.unit_entity(attacker);
            world
                .get_mut::<ActionSlots>(entity)
                .unwrap()
                .set_attack_target(Some(target));
        }

        /// Gives `id` the values of `load_damage_stats`' stats.
        pub(crate) fn stats(world: &mut World, id: StableId, values: [Num; 3]) {
            let mut stats = UnitStats::default();
            stats.refill().extend(values);
            world.insert_parts(id, stats);
        }

        /// Queues `amount` of damage from `source` to `target`, dealt by `cause`.
        pub(crate) fn damage(
            world: &mut World,
            source: Option<StableId>,
            target: StableId,
            amount: i64,
            cause: DamageCause,
        ) {
            world.resource_mut::<PassQueue>().push_damage(Damage {
                source,
                target,
                amount: Num::int(amount),
                kind: DamageKind::new(0),
                cause,
                action: None,
                depth: 0,
                hit: None,
            });
        }

        /// A unit of 10 health, the life pool the first of the scene's pools, `health` and `mana`,
        /// with a weapon of 2 m, of the match's own type and `team`, with `parts`.
        pub(crate) fn spawn_melee(
            world: &mut World,
            pos: impl Into<Position>,
            team: Team,
            parts: impl Bundle,
        ) -> StableId {
            let pos = pos.into();
            let unit_type = Units::test_type(world);
            Combat::spawn_melee_of(world, unit_type, pos, team, parts)
        }

        /// A unit as `spawn_melee` gives, of `unit_type`.
        pub(crate) fn spawn_melee_of(
            world: &mut World,
            unit_type: UnitType,
            pos: impl Into<Position>,
            team: Team,
            parts: impl Bundle,
        ) -> StableId {
            let pos = pos.into();
            let armed = Armed::melee(Num::int(10), Num::int(2), 0, 1, Num::ZERO)
                .on_death(OnDeath::Stay)
                .bundle(world);
            Units::spawn_of(world, unit_type, pos, team, (armed, parts))
        }
    }

    /// A unit's attack as the tests read it: its target, the tick the attack in its windup started
    /// in, and the first tick its weapon may attack again.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub(crate) struct Attack {
        target: Option<StableId>,
        started: Option<Tick>,
        ready_at: Tick,
    }

    impl Attack {
        /// The attack of unit `id` of `world`, whose one weapon sits in its first slot.
        pub(crate) fn of(world: &World, id: StableId) -> Attack {
            let slots = world.part::<ActionSlots>(id);
            let slot = slots.slot(0).unwrap();
            let time = world.part::<SlotTimes>(id).slot(0).unwrap();
            let book = world.resource::<ActionBook>();
            let windup = book.get(slot.action.unwrap()).unwrap().ranks[0].windup;
            let started = slots.attacking().and_then(|_| slots.in_progress());
            let started = started.and_then(InProgress::resolves_at);
            Attack {
                target: slots.attack_target(),
                started: started.map(|at| Tick::new(at.get() - windup.get())),
                ready_at: time.ready_at,
            }
        }

        pub(crate) const fn target(self) -> Option<StableId> {
            self.target
        }

        pub(crate) const fn started(self) -> Option<Tick> {
            self.started
        }

        pub(crate) const fn ready_at(self) -> Tick {
            self.ready_at
        }
    }
}

#[cfg(test)]
mod tests;
