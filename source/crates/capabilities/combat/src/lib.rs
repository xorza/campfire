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
use campfire_actions::EffectQueues;
use campfire_declare::{
    ByType, ClientForm, Declarations, InstallCx, Module, ModuleSet, Phase, ServerForm, SetPlace,
};
use campfire_math::RngStream;
use campfire_script::Ctx;
use campfire_sim::Capability;
use campfire_stats::Life;
use campfire_units::{BodyUnit, Dead, SpawnPoint};

mod attacks;
mod combat_api;
mod combat_bindings;
mod combat_book;
mod combat_column;
mod combat_data;
mod combat_effect;
mod combat_event;
mod combat_hooks;
mod damage;
mod damage_handle;
mod damage_pass;
mod deaths;
mod dying;
mod going_off;
mod heal;
mod heal_handle;
mod interval_due;
mod kept;
mod mode_combat_data;
mod modifier_hooks;
mod modifier_intervals;
mod on_death;
mod pass_queue;
mod recent_attack;
mod recent_attackers;
mod respawn;
mod wielded;

pub use crate::attacks::Attacks;
pub use crate::combat_api::CombatApi;
pub use crate::combat_bindings::CombatBindings;
pub use crate::combat_book::CombatBook;
pub use crate::combat_column::{CombatColumn, RowReads};
pub use crate::combat_data::CombatData;
pub use crate::combat_effect::CombatEffect;
pub use crate::combat_event::{CombatEvent, CombatEvents};
pub use crate::combat_hooks::CombatHooks;
pub use crate::damage::{Damage, DamageCause, DamageWeigher};
pub use crate::damage_handle::DamageHandle;
pub use crate::damage_pass::DamagePass;
pub use crate::deaths::{DeathView, Deaths, Fallen};
pub use crate::dying::Dying;
pub use crate::heal::{Heal, HealCause, HealWeigher};
pub use crate::heal_handle::HealHandle;
pub use crate::kept::Kept;
pub use crate::mode_combat_data::{LeechData, ModeCombatData};
pub use crate::modifier_hooks::ModifierHooks;
pub use crate::modifier_intervals::ModifierIntervals;
pub use crate::on_death::OnDeath;
pub use crate::pass_queue::{PassEntry, PassQueue};
pub use crate::recent_attack::RecentAttack;
pub use crate::recent_attackers::RecentAttackers;
pub use crate::respawn::Respawn;

/// The random stream an attack's roll draws from, for its attacker in its tick.
pub const ROLL_STREAM: RngStream = RngStream::new("combat.roll");

/// The `combat` capability: teams, the life pool, attacks, damage and deaths.
#[derive(Debug)]
pub struct Combat;

campfire_common::enum_all! {
    /// Combat's steps.
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
    pub const ALL;
}

impl ModuleSet for CombatSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Combat);
    const ALL: &'static [CombatSet] = &CombatSet::ALL;

    fn place(self) -> SetPlace {
        SetPlace::Phase(match self {
            CombatSet::Respawn => Phase::Begin,
            CombatSet::Attack => Phase::Attack,
            CombatSet::Hits => Phase::Hits,
            CombatSet::Pay => Phase::Pay,
            CombatSet::Strike => Phase::Strike,
            CombatSet::Interval => Phase::Interval,
            CombatSet::Damage => Phase::Damage,
            CombatSet::Die => Phase::Die,
            CombatSet::Despawn => Phase::End,
        })
    }
}

impl Module for Combat {
    const CAPABILITY: Option<Capability> = Some(Capability::Combat);
    const NEEDS: &'static [Capability] = &[Capability::Stats];
    const USES: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<CombatSet>();
        d.component::<Kept>();
        d.component::<Life>();
        d.component::<OnDeath>();
        d.component::<RecentAttackers>();
        d.component::<Respawn>();
        d.state::<BodyUnit, Respawn>();
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
    /// go, after the Mode stage saw them. A unit's despawn, its attackers and its toggles' pay are
    /// the server's: a predicting client despawns no unit, forgets no attacker it never held, and
    /// pays no toggle, as its pools come from the server.
    fn install(cx: &mut InstallCx<'_>) {
        let scripts = cx.scripts();
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
            ModifierIntervals::run_intervals.in_set(CombatSet::Interval),
            DamagePass::run.in_set(CombatSet::Damage),
            Dying::die.in_set(CombatSet::Die),
            Dying::respawn.in_set(CombatSet::Respawn),
        ));
        cx.by_role(
            (Attacks::attack_events, Attacks::strike::<ServerForm>)
                .chain()
                .in_set(CombatSet::Strike),
            (Attacks::attack_events, Attacks::strike::<ClientForm>)
                .chain()
                .in_set(CombatSet::Strike),
        );
        cx.server((
            Attacks::pay_attack_toggles.in_set(CombatSet::Pay),
            Dying::forget_attackers
                .before(Dying::respawn)
                .in_set(CombatSet::Respawn),
            Dying::despawn_dead.in_set(CombatSet::Despawn),
        ));
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use std::collections::BTreeMap;

    use bevy_ecs::world::World;
    use campfire_actions::internals::TestWeapon;
    use campfire_actions::{
        ActionBook, ActionRange, ActionSlots, Actions, InProgress, SlotKind, SlotTimes,
    };
    use campfire_common::{Tick, Ticks};
    use campfire_math::{Num, Rounding};
    use campfire_script::{Filter, View};
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{StableId, TickRate};
    use campfire_stats::{Life, PoolBook, PoolId, StatBook, StatRule, Stats};
    use campfire_units::{UnitTypeCore, Units};
    use campfire_values::{DamageKind, Rank, RelationSet, Stat, UnitType};

    use crate::Combat;
    use crate::combat_bindings::CombatBindings;
    use crate::combat_book::CombatBook;
    use crate::damage::{Damage, DamageCause};
    use crate::damage_pass::DamagePass;
    use crate::on_death::OnDeath;
    use crate::pass_queue::PassQueue;

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

    /// A test unit's combat values: the life pool it starts with, whether it stays when it dies,
    /// and its one weapon, if it has one.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Armed {
        pub life: Num,
        pub on_death: OnDeath,
        pub arms: Option<Arms>,
    }

    /// A unit's attack as the tests read it: its target, the tick the attack in its windup started
    /// in, and the first tick its weapon may attack again.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct Attack {
        target: Option<StableId>,
        started: Option<Tick>,
        ready_at: Tick,
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

        /// Adds the weapon to the book of `world`, and gives `unit_type` it in its one slot and
        /// the stats it reads: the rate that makes its period at the match's rate, then its
        /// damage, at the first two places of the match's stat book.
        pub fn give(self, world: &mut World, unit_type: UnitType) {
            let hz = world.resource::<TickRate>().hz().get();
            let weapon = TestWeapon {
                projectile: self.projectile,
                ..TestWeapon::new(
                    Filter::of_match_relations(world, RelationSet::Enemies),
                    ActionRange::Meters(self.range),
                    self.windup,
                )
            };
            let id = Actions::add_weapon(world, weapon);
            Units::give_type(
                world,
                unit_type,
                ActionSlots::new([(id, SlotKind::new(0), Some(Rank::FIRST))]),
            );
            // The rate whose attacks are `period` ticks apart, rounded up so the period
            // rounds back to `period`.
            let period = Num::from_int(i64::try_from(self.period.get()).unwrap()).unwrap();
            let rate = Num::int(i64::from(hz))
                .checked_div_rounded(period, Rounding::Ceiling)
                .unwrap();
            debug_assert!(
                world.resource::<StatBook>().len() >= 2,
                "a stat book of a rate and a damage"
            );
            Stats::give_type_stats(world, unit_type, &[rate, self.damage]);
        }
    }

    impl Armed {
        /// A unit of `life` that despawns when it dies, with a melee weapon of `range`, `windup`
        /// ticks, `period` ticks and `damage`.
        pub const fn melee(life: Num, range: Num, windup: u64, period: u64, damage: Num) -> Armed {
            Armed {
                life,
                on_death: OnDeath::Despawn,
                arms: Some(Arms::melee(range, windup, period, damage)),
            }
        }

        /// A unit of 10 life that stays when it dies, with a melee weapon of 2 m that winds up no
        /// tick and deals nothing, each tick.
        pub const fn sparring() -> Armed {
            Armed::melee(Num::int(10), Num::int(2), 0, 1, Num::ZERO).on_death(OnDeath::Stay)
        }

        /// A unit of `life` with no weapon, that despawns when it dies.
        pub const fn unarmed(life: Num) -> Armed {
            Armed {
                life,
                on_death: OnDeath::Despawn,
                arms: None,
            }
        }

        /// The same unit, which `on_death` says whether it stays when it dies.
        #[must_use]
        pub const fn on_death(self, on_death: OnDeath) -> Armed {
            Armed { on_death, ..self }
        }

        /// The same unit, its weapon firing projectiles of the homing type `projectile`.
        #[must_use]
        pub const fn ranged(self, projectile: UnitType) -> Armed {
            let arms = self
                .arms
                .expect("a ranged unit is armed")
                .ranged(projectile);
            Armed {
                arms: Some(arms),
                ..self
            }
        }

        /// Loads the next unit type of the mode, of these values, as `give` gives them.
        pub fn load(self, world: &mut World) -> UnitType {
            let unit_type = Units::load_next_type(world, &UnitTypeCore::default());
            self.give(world, unit_type);
            unit_type
        }

        /// Gives `unit_type` these values in its books, as its `combat` and `slots` sections
        /// would: its life, its death, and its weapon, whose book takes it, with the stats it
        /// reads; no slot when it has no weapon.
        pub fn give(self, world: &mut World, unit_type: UnitType) {
            Units::give_type(world, unit_type, Life::full(self.life));
            Units::give_type(world, unit_type, self.on_death);
            match self.arms {
                Some(arms) => arms.give(world, unit_type),
                None => Units::give_type(world, unit_type, ActionSlots::new([])),
            }
        }
    }

    impl Combat {
        /// Heals `id` at once, as the pass heals it, when it exists and its life is above zero.
        pub fn heal_unit(world: &mut World, id: StableId, amount: impl Into<Num>) {
            let amount = amount.into();
            if let Some(entity) = DamagePass::living(world, id) {
                DamagePass::heal(world, entity, amount);
            }
        }

        /// Gives a match with no mode the stat book a weapon reads, its rate, then its damage.
        pub fn load_weapon_stats(world: &mut World) {
            let rules = ["attack_rate", "damage"]
                .map(|name| (Stat::named(name).unwrap(), StatRule::default()));
            Stats::load_rules(world, &BTreeMap::from(rules));
        }

        /// Binds `life` as the life pool, with no stat bound, as a test world with no mode needs.
        pub fn bind_life(world: &mut World, life: PoolId) {
            world.resource_mut::<PoolBook>().set_life(Some(life));
            world.resource_mut::<CombatBook>().bindings = CombatBindings::default();
        }

        pub fn attack(world: &mut World, attacker: StableId, target: StableId) {
            let entity = world.unit_entity(attacker);
            world
                .get_mut::<ActionSlots>(entity)
                .unwrap()
                .set_attack_target(Some(target));
        }

        /// Queues `amount` of damage from `source` to `target`, dealt by `cause`.
        pub fn damage(
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
    }

    impl Attack {
        /// The attack of unit `id` of `world`, whose one weapon sits in its first slot.
        pub fn of(world: &World, id: StableId) -> Attack {
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

        pub const fn target(self) -> Option<StableId> {
            self.target
        }

        pub const fn started(self) -> Option<Tick> {
            self.started
        }

        pub const fn ready_at(self) -> Tick {
            self.ready_at
        }
    }
}
