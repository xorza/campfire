// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use campfire_actions::{ActionBook, ActionSlots, ActionsSystems, TogglePer};
use campfire_common::EnumAll;
use campfire_declarations::{
    ById, ClientForm, Declarations, InstallCx, Module, ModuleSystems, ServerForm, SetPlace,
};
use campfire_math::RngStream;
use campfire_script::{ApiVersion, EventPhase, Status};
use campfire_sim::{Capability, SimSystems};
use campfire_stats::{LifePool, ModifierAppliers, StatsSystems, TypePools};
use campfire_units::{BodyUnit, Dead, Deaths, Due, Life, SpawnPoint, UnitGrid, UnitsSystems};
use campfire_values::{ActionId, DamageKind, UnitType};

use crate::combat_column::RowReads;
use crate::combat_event::{OnAttack, OnAttackHit, OnDamageTaken, OnKill, OnTakedown};
use crate::damage_kind_book::DamageKindBook;
use crate::held_hits::HeldHits;
use crate::next_damage::NextDamage;
use crate::streak_shots::StreakShots;
use crate::turret::aiming::Aiming;
use crate::volley::{GoingOff, Volley};

mod armor;
mod attack_data;
mod attacks;
mod barrel_turns;
mod barrels;
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
mod damage_kind_book;
mod damage_pass;
mod decay;
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
mod shot_streak;
mod streak_shots;
mod turret;
mod type_armor;
mod unanswered_deaths;
mod unit_offset;
mod volley;
mod weapon;
mod weapon_names;
mod weighers;
mod wielded;

pub use crate::armor::{Armor, ArmorShare};
pub use crate::attack_data::{
    AttackData, ClipData, OuterData, PitchData, PreAttackData, PreAttackEach, ScatterData,
    SplashData, StageData, StreakData, WeaponValue,
};
pub use crate::attacks::Attacks;
pub use crate::barrel_turns::BarrelTurns;
pub use crate::barrels::Barrels;
pub use crate::barrels::barrels_data::{BarrelData, BarrelsData};
pub use crate::clip_shots::ClipShots;
pub use crate::combat_api::CombatApi;
pub use crate::combat_book::CombatBook;
pub use crate::combat_column::CombatColumn;
pub use crate::combat_data::{CombatData, CombatPatch, TaggedArmorData};
pub use crate::damage::{Damage, DamageCause};
pub use crate::damage_effect_data::DamageEffectData;
pub use crate::damage_handle::DamageHandle;
pub use crate::damage_pass::{CalcDamage, CalcHeal, DamagePass};
pub use crate::decay::Decay;
pub use crate::dying::Dying;
pub use crate::error::{CombatDataError, CombatRefusal};
pub use crate::heal::{Heal, HealCause};
pub use crate::heal_effect_data::HealEffectData;
pub use crate::heal_handle::HealHandle;
pub use crate::kept::Kept;
pub use crate::mode_combat_data::{LeechData, ModeCombatData, ModeCombatPatch};
pub use crate::on_death::OnDeath;
pub use crate::pass_queue::{PassEntry, PassQueue};
pub use crate::recent_attack::RecentAttack;
pub use crate::recent_attackers::RecentAttackers;
pub use crate::respawn::Respawn;
pub use crate::shot_streak::ShotStreak;
pub use crate::turret::Turret;
pub use crate::turret::turret_aim::TurretAim;
pub use crate::turret::turret_data::{TurretData, TurretPatch, TurretPitchData};
pub use crate::type_armor::{TaggedArmor, TypeArmor};
pub use crate::unanswered_deaths::{DeathCall, OnUnitDeath, UnansweredDeaths};
pub use crate::unit_offset::UnitOffset;
pub use crate::weapon::{
    ClipRule, OuterRing, Pitch, PreAttackRule, Rate, Scatter, Splash, Streak, Weapon, WeaponDamage,
};
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
pub enum CombatSystems {
    /// Dead units whose respawn is due come back, before any input of the tick can reach
    /// them.
    Respawn,
    /// The streaks whose coast ended lose their stage, and the units whose streak changed push
    /// their grants, before the grant pass of Inputs.
    StreakAtInputs,
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
    /// The units whose streak changed as they struck push their grants, before the grant pass of
    /// Resolve.
    StreakAtResolve,
    /// Units at zero life die.
    Die,
    /// The dead whose type despawns go, after the Mode stage saw them, and those whose type kept
    /// them go as their time ends.
    Despawn,
}

impl ModuleSystems for CombatSystems {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        match self {
            Self::Respawn => SetPlace::at(UnitsSystems::Begin),
            Self::StreakAtInputs => SetPlace::stage(SimSystems::Inputs)
                .after(UnitsSystems::Begin)
                .before(ActionsSystems::HoldAtInputs),
            Self::Attack => SetPlace::at(ActionsSystems::WindUp),
            Self::Hits => SetPlace::at(ActionsSystems::Hit),
            Self::Pay => SetPlace::at(ActionsSystems::Pay),
            Self::Strike => SetPlace::at(ActionsSystems::Strike),
            Self::Damage => SetPlace::stage(SimSystems::Resolve).before(ActionsSystems::Resolve),
            Self::StreakAtResolve => SetPlace::at(ActionsSystems::Resolve),
            Self::Die => (SetPlace::stage(SimSystems::Resolve).after(StatsSystems::GrantAtResolve))
                .before(StatsSystems::ClearDead),
            Self::Despawn => SetPlace::at(UnitsSystems::End),
        }
    }
}

impl Module for Combat {
    const CAPABILITY: Option<Capability> = Some(Capability::Combat);
    const NEEDS: &'static [Capability] = &[Capability::Stats];
    const USES: &'static [Capability] = &[];
    type Systems = CombatSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.scratch::<Due<GoingOff>>();
        d.section::<CombatData>();
        d.section::<TurretData>();
        d.section::<ModeCombatData>();
        d.component::<TurretAim>();
        d.timed::<TurretAim>();
        d.book::<ById<UnitType, Turret>>(Turret::book);
        d.part::<BodyUnit, TurretAim>(|at| at.of_type::<Turret>().map(TurretAim::rest));
        d.section::<BarrelsData>();
        d.book::<ById<UnitType, Barrels>>(Barrels::book);
        d.component::<BarrelTurns>();
        // A unit with slots turns the barrels of its weapons on a server, which alone launches;
        // a client never holds it.
        d.server_part::<ActionSlots, BarrelTurns>();
        d.component::<Kept>();
        d.component::<Life>();
        d.component::<RecentAttackers>();
        d.component::<ClipShots>();
        // A unit with slots counts the shots of its weapons with a clip, from its spawn or a
        // grant of slots on; a client receives its own units' counts, as it predicts them.
        d.server_part::<ActionSlots, ClipShots>();
        d.component::<ShotStreak>();
        // A unit with slots counts its streak on a server, as a script may grant it a streak
        // weapon; a client never holds it.
        d.server_part::<ActionSlots, ShotStreak>();
        d.timed::<ShotStreak>();
        d.scratch::<StreakShots>();
        d.component::<Respawn>();
        d.timed::<Respawn>();
        d.state::<BodyUnit, Respawn>();
        d.component::<Decay>();
        d.timed::<Decay>();
        d.state::<BodyUnit, Decay>();
        d.scratch::<PassQueue>();
        d.scratch::<Volley>();
        d.resource::<NextDamage>();
        d.resource::<HeldHits>();
        d.observer(HeldHits::drop_died);
        d.report::<Deaths>();
        d.resource::<UnansweredDeaths>();
        d.mode_events::<UnansweredDeaths>(EventPhase::Losses);
        d.calls::<Weighers>();
        d.names::<DamageKind>(Self::damage_kinds);
        d.book::<DamageKindBook>(DamageKindBook::build);
        d.fact::<LifePool>(Self::life_pool);
        d.book::<CombatBook>(CombatBook::build);
        d.listed::<AttackData>();
        d.book::<ById<ActionId, Weapon>>(Weapon::book);
        d.extend::<ModifierAppliers>(ShotStreak::applied_ways);
        d.rule(Self::rule);
        d.rule(Self::attacks_rule);
        d.listed::<DamageEffectData>();
        d.listed::<HealEffectData>();
        d.require::<Respawn, Dead>();
        d.require::<Decay, Dead>();
        d.require::<Respawn, SpawnPoint>();
        d.column::<CombatColumn, RowReads>(CombatColumn::fill_row);
        d.api(CombatApi::register);
        d.hook::<OnAttack>(Status::Runs(ApiVersion::FIRST));
        d.hook::<OnUnitDeath>(Status::Runs(ApiVersion::FIRST));
        d.hook::<OnAttackHit>(Status::Runs(ApiVersion::FIRST));
        d.hook::<OnDamageTaken>(Status::Runs(ApiVersion::FIRST));
        d.hook::<OnKill>(Status::Runs(ApiVersion::FIRST));
        d.hook::<OnTakedown>(Status::Runs(ApiVersion::FIRST));
        d.hook::<CalcDamage>(Status::Runs(ApiVersion::FIRST));
        d.hook::<CalcHeal>(Status::Runs(ApiVersion::FIRST));
        d.book::<ById<UnitType, Life>>(TypePools::lives);
        d.book::<ById<UnitType, OnDeath>>(OnDeath::book);
        d.book::<ById<UnitType, TypeArmor>>(TypeArmor::book);
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
    /// go, after the Mode stage saw them. A streak weapon's shot counts in its unit's streak, whose
    /// stage's modifier its unit holds, the stage ending in Inputs past its coast. A unit's hits,
    /// damage, death and despawn, its attackers, its toggles' pay and its streak are the
    /// server's: a predicting client deals no damage, as its pools and its deaths come from the
    /// server, despawns no unit, forgets no attacker it never held, pays no toggle and counts no
    /// streak; it predicts a respawn that is due. The server gives each shot of a weapon of
    /// barrels its launch once it struck. The turrets' aim, the slow weapons' hits, the pay of
    /// toggles that cost an attack, the barrels and the streaks install only in a match whose
    /// books have them (02-engine-core.md, Books before the install).
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        let turrets = cx
            .book::<ById<UnitType, Turret>>()
            .is_some_and(|turrets| !turrets.is_empty());
        let slow = cx
            .book::<ById<ActionId, Weapon>>()
            .is_some_and(|weapons| weapons.values().any(|weapon| weapon.speed.is_some()));
        let barrels = cx
            .book::<ById<UnitType, Barrels>>()
            .is_some_and(|barrels| !barrels.is_empty());
        let streaks = cx
            .book::<ById<ActionId, Weapon>>()
            .is_some_and(|weapons| weapons.values().any(|weapon| weapon.streak.is_some()));
        let actions = cx.book::<ActionBook>();
        let toggles = actions.is_some_and(|book| {
            book.iter()
                .any(|action| action.toggles_per(TogglePer::Attack))
        });
        if turrets {
            cx.add(
                CombatSystems::Attack,
                Aiming::aim.before(Attacks::start_attacks),
            );
        }
        cx.add(CombatSystems::Attack, Attacks::start_attacks);
        cx.add(CombatSystems::Respawn, Dying::respawn);
        cx.server(
            CombatSystems::Hits,
            UnitGrid::before_reading(Attacks::take_hits),
        );
        cx.server(CombatSystems::Damage, DamagePass::run);
        cx.server(CombatSystems::Die, Dying::die);
        cx.add(CombatSystems::Pay, Volley::collect);
        cx.by_role(
            CombatSystems::Strike,
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
                CombatSystems::Pay,
                Attacks::pay_attack_toggles.after(Volley::collect),
            );
        }
        cx.server(
            CombatSystems::Respawn,
            Dying::forget_attackers.before(Dying::respawn),
        );
        cx.server(CombatSystems::Despawn, (Dying::despawn_dead, Dying::decay));
        if slow {
            cx.server(
                CombatSystems::Hits,
                Attacks::land_held.before(Attacks::take_hits),
            );
        }
        if barrels {
            cx.server(
                CombatSystems::Strike,
                BarrelTurns::launch.after(Attacks::strike::<ServerForm>),
            );
        }
        if streaks {
            cx.server(
                CombatSystems::StreakAtInputs,
                (ShotStreak::coast, ShotStreak::want).chain(),
            );
            cx.server(
                CombatSystems::Strike,
                ShotStreak::count.after(Attacks::strike::<ServerForm>),
            );
            cx.server(CombatSystems::StreakAtResolve, ShotStreak::want);
        }
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use campfire_declarations::internals::BookLoad;
    use std::collections::BTreeMap;

    use bevy_ecs::system::RunSystemOnce;
    use bevy_ecs::world::World;
    use campfire_actions::internals::TestAction;
    use campfire_actions::{
        ActionBook, ActionData, ActionKind, ActionProgress, ActionRange, ActionSlots, Actions, Aim,
        AttackTarget, InProgress, LoadedAction, ProgressDues, SlotKind, SlotTimes,
    };
    use campfire_common::{DeclaredName, Tick, Ticks};
    use campfire_declarations::{ById, ServerForm};
    use campfire_math::{Num, Rounding};
    use campfire_players::ResourceAmount;
    use campfire_script::{Filter, MatchNames};
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{MatchRate, StableId};
    use campfire_stats::{ModifierBook, PoolBook, StatBook, StatRule, Stats};
    use campfire_units::{Life, PoolId, UnitTypeCore, Units};
    use campfire_units::{PoolCost, StatId};
    use campfire_values::{
        ActionId, DamageKind, Distance, PackageIndex, Rank, RelationSet, Speed, Stat, UnitType,
    };

    use crate::Combat;
    use crate::attack_data::AttackData;
    use crate::attacks::Attacks;
    use crate::combat_book::CombatBook;
    use crate::damage::{Damage, DamageCause};
    use crate::damage_pass::DamagePass;
    use crate::held_hits::HeldHits;
    use crate::on_death::OnDeath;
    use crate::pass_queue::PassEntry;
    use crate::volley::Volley;
    use crate::weapon::{
        ClipRule, Pitch, PreAttackRule, Rate, Scatter, Splash, Streak, Weapon, WeaponDamage,
    };
    use crate::weapon_names::WeaponNames;

    /// A weapon tests arm units with: what it aims at, its range, its windup, the type of the
    /// homing projectile it fires, if it fires one, where its rate and its damage come from, the
    /// stat that speeds its own period, its clip, its cost in pools and in a player resource, and
    /// its splash, its scatter's radius, its bound of pitch, its speed, its streak, its pre-attack
    /// and the bearing its attacker aims within, at its one rank.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct TestWeapon {
        pub aim: Filter,
        pub range: ActionRange,
        pub min_range: Option<Distance>,
        pub windup: Ticks,
        pub projectile: Option<UnitType>,
        pub rate: Rate,
        pub rate_bonus: Option<StatId>,
        pub damage: WeaponDamage,
        pub damage_bonus: Option<StatId>,
        pub clip: Option<ClipRule>,
        pub cost: PoolCost,
        pub resource_cost: Option<ResourceAmount>,
        pub splash: Option<Splash>,
        pub scatter: Option<Num>,
        pub pitch: Option<Pitch>,
        pub speed: Option<Speed>,
        pub streak: Option<Streak>,
        pub pre_attack: Option<PreAttackRule>,
        pub aim_deg: Option<Num>,
    }

    impl TestWeapon {
        /// A melee weapon at `aim` within `range` that winds up `windup`, of the first two stats'
        /// rate and damage, costing nothing.
        pub fn new(aim: Filter, range: ActionRange, windup: Ticks) -> Self {
            Self {
                aim,
                range,
                min_range: None,
                windup,
                projectile: None,
                rate: Rate::Stat(StatId::new(0)),
                rate_bonus: None,
                damage: WeaponDamage::Stat(StatId::new(1)),
                damage_bonus: None,
                clip: None,
                cost: PoolCost::default(),
                resource_cost: None,
                splash: None,
                scatter: None,
                pitch: None,
                speed: None,
                streak: None,
                pre_attack: None,
                aim_deg: None,
            }
        }
    }

    impl Combat {
        /// Adds `weapon` to the match's action book and its weapon book, which deals damage of
        /// the first kind, and its streak's modifiers to the book of the streaks' modifiers.
        pub fn add_weapon(world: &mut World, weapon: TestWeapon) -> ActionId {
            let action = TestAction {
                min_range: weapon.min_range,
                cost: weapon.cost,
                resource_cost: weapon.resource_cost,
                ..TestAction::new(
                    ActionKind::Attack,
                    Aim::Unit(weapon.aim),
                    weapon.range,
                    weapon.windup,
                )
            };
            let action = match weapon.projectile {
                Some(projectile) => action.homing(projectile),
                None => action,
            };
            let id = Actions::add(world, "weapon", action);
            ById::<ActionId, Weapon>::amend(world, |_, book| {
                book.set(
                    id,
                    Weapon {
                        rate: weapon.rate,
                        rate_bonus: weapon.rate_bonus,
                        damage: weapon.damage,
                        damage_bonus: weapon.damage_bonus,
                        kind: DamageKind::new(0),
                        clip: weapon.clip.map(|clip| Box::from([clip])),
                        splash: weapon.splash,
                        scatter: weapon.scatter.map(|radius| Scatter {
                            radius,
                            tags: Box::new([]),
                        }),
                        pitch: weapon.pitch,
                        speed: weapon.speed,
                        streak: weapon.streak,
                        pre_attack: weapon.pre_attack,
                        aim: weapon.aim_deg,
                    },
                );
            });
            id
        }

        /// Resolves the weapon of `action`, which the action book of `world` loaded from `data` of
        /// the mode's package, into the match's weapon book, as the load resolves it.
        pub fn load_weapon(
            world: &mut World,
            action: ActionId,
            data: &ActionData,
            name: &DeclaredName,
        ) {
            let Some(attack) = data.section::<AttackData>() else {
                return;
            };
            let book = world.resource::<ActionBook>();
            let names = WeaponNames {
                loaded: LoadedAction {
                    package: PackageIndex::MODE,
                    name,
                    action,
                    data,
                },
                action: book.get(action).expect("the book loaded the action"),
                tick_rate: *world.resource::<MatchRate>(),
                stats: world.get_resource::<StatBook>(),
                kinds: world.resource::<MatchNames>().table::<DamageKind>(),
                tags: world.resource::<MatchNames>().tags(),
                modifiers: world.get_resource::<ModifierBook>(),
            };
            let weapon = names.weapon(attack).expect("a test's weapon resolves");
            ById::<ActionId, Weapon>::amend(world, |_, book| book.set(action, weapon));
        }

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
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Arms {
        range: Num,
        min_range: Option<Num>,
        windup: Ticks,
        period: Ticks,
        damage: Num,
        projectile: Option<UnitType>,
        splash: Option<Splash>,
        scatter: Option<Num>,
        pitch: Option<[Num; 2]>,
        speed: Option<Speed>,
        aim: Option<Num>,
    }

    /// A test unit's combat values: the life pool it starts with, whether it stays when it dies,
    /// and its one weapon, if it has one.
    #[derive(Debug, Clone, PartialEq, Eq)]
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
                min_range: None,
                windup: Ticks::new(windup),
                period: Ticks::new(period),
                damage,
                projectile: None,
                splash: None,
                scatter: None,
                pitch: None,
                speed: None,
                aim: None,
            }
        }

        /// The same weapon, firing projectiles of the homing type `projectile`.
        #[must_use]
        pub fn ranged(self, projectile: UnitType) -> Self {
            Self {
                projectile: Some(projectile),
                ..self
            }
        }

        /// Adds the weapon to the book of `world`, and gives `unit_type` it in its one slot and
        /// the stats it reads: the rate that makes its period at the match's rate, then its
        /// damage, at the first two places of the match's stat book.
        pub fn give(self, world: &mut World, unit_type: UnitType) {
            let hz = world.resource::<MatchRate>().hz().get();
            let weapon = TestWeapon {
                projectile: self.projectile,
                splash: self.splash,
                scatter: self.scatter,
                pitch: self.pitch.map(|[low, high]| Pitch::of(low, high)),
                speed: self.speed,
                aim_deg: self.aim,
                min_range: self.min_range.map(|meters| Distance::new(meters).unwrap()),
                ..TestWeapon::new(
                    Filter::of_match_relations(world, RelationSet::Enemies),
                    ActionRange::meters(self.range),
                    self.windup,
                )
            };
            let id = Combat::add_weapon(world, weapon);
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
        pub fn sparring() -> Self {
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
        pub fn on_death(self, on_death: OnDeath) -> Self {
            Self { on_death, ..self }
        }

        /// The same unit, its weapon firing projectiles of the homing type `projectile`.
        #[must_use]
        pub fn ranged(self, projectile: UnitType) -> Self {
            let arms = self
                .arms
                .expect("a ranged unit is armed")
                .ranged(projectile);
            Self {
                arms: Some(arms),
                ..self
            }
        }

        /// The same unit, its weapon taking no target nearer than `meters`.
        #[must_use]
        pub fn keeping_off(self, meters: Num) -> Self {
            let arms = self.arms.expect("a unit that keeps off is armed");
            Self {
                arms: Some(Arms {
                    min_range: Some(meters),
                    ..arms
                }),
                ..self
            }
        }

        /// The same unit, its weapon's projectile splashing as `splash` says.
        #[must_use]
        pub fn splashing(self, splash: Splash) -> Self {
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
        pub fn scattering(self, radius: Num) -> Self {
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
        pub fn pitched(self, low: Num, high: Num) -> Self {
            let arms = self.arms.expect("a pitched unit is armed");
            Self {
                arms: Some(Arms {
                    pitch: Some([low, high]),
                    ..arms
                }),
                ..self
            }
        }

        /// The same unit, which turns toward its weapon's target until it faces it within `aim`
        /// degrees.
        #[must_use]
        pub fn aiming(self, aim: Num) -> Self {
            let arms = self.arms.expect("an aiming unit is armed");
            Self {
                arms: Some(Arms {
                    aim: Some(aim),
                    ..arms
                }),
                ..self
            }
        }

        /// The same unit, its weapon of no projectile slow: its hit flies at `speed` meters a
        /// second.
        #[must_use]
        pub fn slow(self, speed: Num) -> Self {
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
            PoolBook::amend(world, |_, book| book.set_life(Some(life)));
            CombatBook::default().load_into(world);
        }

        pub fn attack(world: &mut World, attacker: StableId, target: StableId) {
            ProgressDues::change_unit(world, attacker, |progress, dues| {
                progress.set_attack_target(Some(AttackTarget::ordered(target)), dues);
            });
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
