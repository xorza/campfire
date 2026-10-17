// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (09-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use campfire_actions::{Action, ActionBook, ActionSlots, ActionsSet, DamageKindBook, TogglePer};
use campfire_common::EnumAll;
use campfire_declare::{
    ByType, ClientForm, Declarations, InstallCx, Module, ModuleSet, ServerForm, SetPlace,
};
use campfire_math::RngStream;
use campfire_sim::{Capability, SimSet};
use campfire_stats::{Life, LifePool, StatsSet, TypePools};
use campfire_units::{BodyUnit, Dead, Deaths, Due, SpawnPoint, UnitGrid, UnitsSet};

use crate::combat_column::RowReads;
use crate::held_hits::HeldHits;
use crate::next_damage::NextDamage;
use crate::turret::aiming::Aiming;
use crate::volley::{GoingOff, Volley};

mod armor;
mod attacks;
mod clip_shots;
mod combat_api;
mod combat_book;
mod combat_books;
mod combat_column;
mod combat_data;
mod combat_effect;
mod combat_event;
mod damage;
mod damage_effect_data;
mod damage_handle;
mod damage_pass;
mod dying;
mod error;
mod heal;
mod heal_effect_data;
mod heal_handle;
mod held_hits;
mod kept;
mod mode_combat_data;
mod next_damage;
mod on_death;
mod pass_queue;
mod recent_attack;
mod recent_attackers;
mod respawn;
mod turret;
mod volley;
mod weighers;
mod wielded;

pub use crate::armor::{Armor, ArmorShare};
pub use crate::attacks::Attacks;
pub use crate::clip_shots::ClipShots;
pub use crate::combat_api::CombatApi;
pub use crate::combat_book::CombatBook;
pub use crate::combat_column::CombatColumn;
pub use crate::combat_data::CombatData;
pub use crate::combat_effect::CombatEffect;
pub use crate::combat_event::CombatEvent;
pub use crate::damage::{Damage, DamageCause};
pub use crate::damage_effect_data::DamageEffectData;
pub use crate::damage_handle::DamageHandle;
pub use crate::damage_pass::DamagePass;
pub use crate::dying::Dying;
pub use crate::error::{CombatDataError, CombatRefusal};
pub use crate::heal::{Heal, HealCause};
pub use crate::heal_effect_data::HealEffectData;
pub use crate::heal_handle::HealHandle;
pub use crate::kept::Kept;
pub use crate::mode_combat_data::{LeechData, ModeCombatData};
pub use crate::on_death::OnDeath;
pub use crate::pass_queue::{PassEntry, PassQueue};
pub use crate::recent_attack::RecentAttack;
pub use crate::recent_attackers::RecentAttackers;
pub use crate::respawn::Respawn;
pub use crate::turret::Turret;
pub use crate::turret::turret_angle::TurretAngle;
pub use crate::turret::turret_data::TurretData;
pub use crate::weighers::{Weigh, Weighers};

/// The random stream an attack's roll draws from, for its attacker in its tick.
pub const ROLL_STREAM: RngStream = RngStream::new("combat.roll");

/// The random stream an attack's scatter draws from, for its attacker in its tick
/// (04-capabilities/combat.md, An attack may scatter).
pub const SCATTER_STREAM: RngStream = RngStream::new("combat.scatter");

/// The `combat` capability: teams, the life pool, attacks, damage and deaths.
#[derive(Debug)]
pub struct Combat;

/// Combat's steps.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum CombatSet {
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
    /// The tick's damage is dealt.
    Damage,
    /// Units at zero life die.
    Die,
    /// The dead whose type despawns go, after the Mode stage saw them.
    Despawn,
}

impl ModuleSet for CombatSet {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        match self {
            Self::Respawn => SetPlace::at(UnitsSet::Begin),
            Self::Attack => SetPlace::at(ActionsSet::WindUp),
            Self::Hits => SetPlace::at(ActionsSet::Hit),
            Self::Pay => SetPlace::at(ActionsSet::Pay),
            Self::Strike => SetPlace::at(ActionsSet::Strike),
            Self::Damage => SetPlace::stage(SimSet::Resolve).before(ActionsSet::Resolve),
            Self::Die => (SetPlace::stage(SimSet::Resolve).after(StatsSet::GrantAtResolve))
                .before(StatsSet::ClearDead),
            Self::Despawn => SetPlace::at(UnitsSet::End),
        }
    }
}

impl Module for Combat {
    const CAPABILITY: Option<Capability> = Some(Capability::Combat);
    const NEEDS: &'static [Capability] = &[Capability::Stats];
    const USES: &'static [Capability] = &[];
    type Set = CombatSet;

    fn declare<D: Declarations>(d: &mut D) {
        d.scratch::<Due<GoingOff>>();
        d.section::<CombatData>();
        d.section::<TurretData>();
        d.section::<ModeCombatData>();
        d.component::<TurretAngle>();
        d.book::<ByType<Turret>>(Turret::book);
        d.part::<BodyUnit, TurretAngle>(|at| at.of_type::<Turret>().map(|_| TurretAngle::REST));
        d.component::<Kept>();
        d.component::<Life>();
        d.component::<RecentAttackers>();
        d.component::<ClipShots>();
        // A unit with slots counts the shots of its weapons with a clip, from its spawn or a
        // grant of slots on; a client receives its own units' counts, as it predicts them.
        d.server_part::<ActionSlots, ClipShots>();
        d.component::<Respawn>();
        d.state::<BodyUnit, Respawn>();
        d.scratch::<PassQueue>();
        d.scratch::<Volley>();
        d.resource::<NextDamage>();
        d.resource::<HeldHits>();
        d.observer(HeldHits::drop_died);
        d.report::<Deaths>();
        d.calls::<Weighers>();
        d.book::<DamageKindBook>(Self::damage_kinds);
        d.fact::<LifePool>(Self::life_pool);
        d.book::<CombatBook>(CombatBook::build);
        d.rule(Self::rule);
        d.rule(Self::attacks_rule);
        d.listed::<DamageEffectData>();
        d.listed::<HealEffectData>();
        d.require::<Respawn, Dead>();
        d.require::<Respawn, SpawnPoint>();
        d.column::<CombatColumn, RowReads>(CombatColumn::fill_row);
        d.api(CombatApi::register);
        d.book::<ByType<Life>>(TypePools::lives);
        d.book::<ByType<OnDeath>>(OnDeath::book);
        d.book::<ByType<Armor>>(Armor::book);
        d.part::<BodyUnit, Life>(|at| at.of_type::<Life>().copied());
        // A unit that can die keeps who struck it, for its death's assisters.
        d.part::<BodyUnit, RecentAttackers>(|at| {
            at.of_type::<OnDeath>().map(|_| RecentAttackers::default())
        });
    }

    /// Adds combat to a match: in Inputs, dead units whose respawn is due come back; in Act,
    /// attacks in range start once ready; in Hit, windups that end strike, or fire when ranged
    /// and the match has projectiles; in Resolve, the tick's damage is dealt, then units at zero
    /// life die, each with its killer and assisters; in Vision, the dead whose type despawns
    /// go, after the Mode stage saw them. A unit's hits, damage, death and despawn, its attackers
    /// and its toggles' pay are the server's: a predicting client deals no damage, as its pools
    /// and its deaths come from the server, despawns no unit, forgets no attacker it never held,
    /// and pays no toggle; it predicts a respawn that is due. The turrets' aim, the slow weapons'
    /// hits and the pay of toggles that cost an attack install only in a match whose books have
    /// them (02-engine-core.md, Books before the install).
    fn install(cx: &mut InstallCx<'_, Self::Set>) {
        let turrets = cx
            .book::<ByType<Turret>>()
            .is_some_and(|turrets| !turrets.is_empty());
        let actions = cx.book::<ActionBook>();
        let slow = actions.is_some_and(|book| book.iter().any(Action::is_slow));
        let toggles = actions.is_some_and(|book| {
            book.iter()
                .any(|action| action.toggles_per(TogglePer::Attack))
        });
        if turrets {
            cx.add(
                CombatSet::Attack,
                Aiming::aim.before(Attacks::start_attacks),
            );
        }
        cx.add(CombatSet::Attack, Attacks::start_attacks);
        cx.add(CombatSet::Respawn, Dying::respawn);
        cx.server(
            CombatSet::Hits,
            UnitGrid::before_reading(Attacks::take_hits),
        );
        cx.server(CombatSet::Damage, DamagePass::run);
        cx.server(CombatSet::Die, Dying::die);
        cx.add(CombatSet::Pay, Volley::collect);
        cx.by_role(
            CombatSet::Strike,
            (
                (Attacks::collect_attacks, Attacks::attack_events)
                    .chain()
                    .run_if(Volley::any),
                Attacks::strike::<ServerForm>,
            )
                .chain(),
            Attacks::strike::<ClientForm>,
        );
        if toggles {
            cx.server(
                CombatSet::Pay,
                Attacks::pay_attack_toggles.after(Volley::collect),
            );
        }
        cx.server(
            CombatSet::Respawn,
            Dying::forget_attackers.before(Dying::respawn),
        );
        cx.server(CombatSet::Despawn, Dying::despawn_dead);
        if slow {
            cx.server(
                CombatSet::Hits,
                Attacks::land_held.before(Attacks::take_hits),
            );
        }
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use std::collections::BTreeMap;

    use bevy_ecs::system::RunSystemOnce;
    use bevy_ecs::world::World;
    use campfire_actions::internals::TestWeapon;
    use campfire_actions::{
        ActionBook, ActionProgress, ActionRange, ActionSlots, Actions, InProgress, Pitch, SlotKind,
        SlotTimes, SplashData,
    };
    use campfire_common::{Tick, Ticks};
    use campfire_declare::ServerForm;
    use campfire_math::{Num, Rounding};
    use campfire_script::{Filter, MatchNames};
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{StableId, TickRate};
    use campfire_stats::{Life, PoolBook, StatBook, StatRule, Stats};
    use campfire_units::{PoolId, UnitTypeCore, Units};
    use campfire_values::{DamageKind, Rank, RelationSet, Speed, Stat, UnitType};

    use crate::Combat;
    use crate::attacks::Attacks;
    use crate::combat_book::CombatBook;
    use crate::damage::{Damage, DamageCause};
    use crate::damage_pass::DamagePass;
    use crate::held_hits::HeldHits;
    use crate::on_death::OnDeath;
    use crate::pass_queue::PassEntry;
    use crate::volley::Volley;

    impl Combat {
        /// Strikes with each attack whose windup ended, as the server's strike set does once the
        /// tick's attacks paid: for a test that strikes outside a tick.
        pub fn strike_now(world: &mut World) {
            world.run_system_once(Volley::collect).unwrap();
            world
                .run_system_once(Attacks::strike::<ServerForm>)
                .unwrap();
        }

        /// Queues `amount` of the damage kind `kind` from `source` to `target` for the tick's
        /// damage pass, as an effect of no action deals it: for a test that deals what no input
        /// can.
        pub fn queue_damage(
            world: &mut World,
            source: Option<StableId>,
            target: StableId,
            amount: Num,
            kind: &str,
        ) {
            let kind = world
                .resource::<MatchNames>()
                .named(kind)
                .unwrap_or_else(|| panic!("the mode declares the damage kind {kind}"));
            DamagePass::queue(
                world,
                PassEntry::Damage(Damage {
                    source,
                    target,
                    amount,
                    kind,
                    cause: DamageCause::Effect,
                    action: None,
                    depth: 0,
                    hit: None,
                }),
            );
        }
    }

    /// A test unit's weapon: it aims at enemies within `range`, winds up `windup`, may attack
    /// again `period` after an attack's start, deals `damage`, fires a projectile of the type
    /// `projectile` when given, and splashes, scatters, bounds its pitch and flies its hit at a
    /// speed as given.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Arms {
        range: Num,
        windup: Ticks,
        period: Ticks,
        damage: Num,
        projectile: Option<UnitType>,
        splash: Option<SplashData>,
        scatter: Option<Num>,
        pitch: Option<[Num; 2]>,
        speed: Option<Speed>,
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
        pub const fn melee(range: Num, windup: u64, period: u64, damage: Num) -> Self {
            Self {
                range,
                windup: Ticks::new(windup),
                period: Ticks::new(period),
                damage,
                projectile: None,
                splash: None,
                scatter: None,
                pitch: None,
                speed: None,
            }
        }

        /// The same weapon, firing projectiles of the homing type `projectile`.
        #[must_use]
        pub const fn ranged(self, projectile: UnitType) -> Self {
            Self {
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
                splash: self.splash,
                scatter: self.scatter,
                pitch: self.pitch.map(|[low, high]| Pitch::of(low, high)),
                speed: self.speed,
                ..TestWeapon::new(
                    Filter::of_match_relations(world, RelationSet::Enemies),
                    ActionRange::meters(self.range),
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
                .checked_div(period, Rounding::Ceiling)
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
        pub const fn melee(life: Num, range: Num, windup: u64, period: u64, damage: Num) -> Self {
            Self {
                life,
                on_death: OnDeath::Despawn,
                arms: Some(Arms::melee(range, windup, period, damage)),
            }
        }

        /// A unit of 10 life that stays when it dies, with a melee weapon of 2 m that winds up no
        /// tick and deals nothing, each tick.
        pub const fn sparring() -> Self {
            Self::melee(Num::int(10), Num::int(2), 0, 1, Num::ZERO).on_death(OnDeath::Stay)
        }

        /// A unit of `life` with no weapon, that despawns when it dies.
        pub const fn unarmed(life: Num) -> Self {
            Self {
                life,
                on_death: OnDeath::Despawn,
                arms: None,
            }
        }

        /// The same unit, which `on_death` says whether it stays when it dies.
        #[must_use]
        pub const fn on_death(self, on_death: OnDeath) -> Self {
            Self { on_death, ..self }
        }

        /// The same unit, its weapon firing projectiles of the homing type `projectile`.
        #[must_use]
        pub const fn ranged(self, projectile: UnitType) -> Self {
            let arms = self
                .arms
                .expect("a ranged unit is armed")
                .ranged(projectile);
            Self {
                arms: Some(arms),
                ..self
            }
        }

        /// The same unit, its weapon's projectile splashing as `splash` says.
        #[must_use]
        pub const fn splashing(self, splash: SplashData) -> Self {
            let arms = self.arms.expect("a splashing unit is armed");
            Self {
                arms: Some(Arms {
                    splash: Some(splash),
                    ..arms
                }),
                ..self
            }
        }

        /// The same unit, its weapon's projectile scattering within `radius` of its target.
        #[must_use]
        pub const fn scattering(self, radius: Num) -> Self {
            let arms = self.arms.expect("a scattering unit is armed");
            Self {
                arms: Some(Arms {
                    scatter: Some(radius),
                    ..arms
                }),
                ..self
            }
        }

        /// The same unit, its weapon's targets lying from `low` to `high` degrees of pitch.
        #[must_use]
        pub const fn pitched(self, low: Num, high: Num) -> Self {
            let arms = self.arms.expect("a pitched unit is armed");
            Self {
                arms: Some(Arms {
                    pitch: Some([low, high]),
                    ..arms
                }),
                ..self
            }
        }

        /// The same unit, its weapon of no projectile slow: its hit flies at `speed` meters a
        /// second.
        #[must_use]
        pub const fn slow(self, speed: Num) -> Self {
            let arms = self.arms.expect("a slow unit is armed");
            Self {
                arms: Some(Arms {
                    speed: Speed::new(speed),
                    ..arms
                }),
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
        /// How many slow weapons' hits are on their way.
        pub fn held_hits(world: &World) -> usize {
            world.resource::<HeldHits>().len()
        }

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

        /// Binds `life` as the life pool, with no stat bound and no assist window, as a test world
        /// with no mode needs.
        pub fn bind_life(world: &mut World, life: PoolId) {
            world.resource_mut::<PoolBook>().set_life(Some(life));
            world.insert_resource(CombatBook::default());
        }

        pub fn attack(world: &mut World, attacker: StableId, target: StableId) {
            let entity = world.unit_entity(attacker);
            world
                .get_mut::<ActionProgress>(entity)
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
            DamagePass::queue(
                world,
                PassEntry::Damage(Damage {
                    source,
                    target,
                    amount: Num::int(amount),
                    kind: DamageKind::new(0),
                    cause,
                    action: None,
                    depth: 0,
                    hit: None,
                }),
            );
        }
    }

    impl Attack {
        /// The attack of unit `id` of `world`, whose one weapon sits in its first slot.
        pub fn of(world: &World, id: StableId) -> Self {
            let slots = world.part::<ActionSlots>(id);
            let slot = slots.slot(0).unwrap();
            let time = world.part::<SlotTimes>(id).slot(0).unwrap();
            let book = world.resource::<ActionBook>();
            let windup = book.get(slot.action.unwrap()).unwrap().ranks[0].windup;
            let progress = world.part::<ActionProgress>(id);
            let started = progress.attacking().and_then(|_| progress.in_progress());
            let started = started.and_then(InProgress::resolves_at);
            Self {
                target: progress.attack_target(),
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
