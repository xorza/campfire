use std::collections::BTreeMap;

use bevy_ecs::world::World;
use campfire_abilities::internals::{MANA, RAGE};
use campfire_abilities::{Abilities, CastData};
use campfire_actions::{
    ActionData, ActionKeys, ActionRange, ActionSlots, Actions, AttackTarget, ProgressDues,
    RangeField, SlotKind, Targeting, TimedSlots,
};
use campfire_combat::OnDeath;
use campfire_common::{DeclaredName, PlayerSlot};
use campfire_declarations::MatchFacts;
use campfire_math::{Num, Vec3};
use campfire_players::Players;
use campfire_script::ScriptLimits;
use campfire_sim::{Capability, StableId};
use campfire_stats::Pools;
use campfire_units::internals::Spawn;
use campfire_units::{Life, MoveStep, Owner, UnitTypeCore, Units};
use campfire_values::{
    ActionId, FilterData, Number, PackageIndex, PackagePath, Param, Rank, Ranked, Scalar,
    TypeScope, UnitType,
};

use crate::fixtures::actions::ActionsFixtures;
use crate::fixtures::units::UnitsFixtures;
use crate::release::r#match::Match;

/// The match's pools, by name in order: the life pool first, as it is until a mode binds one.
const POOLS: [&str; 3] = ["health", "mana", "rage"];

/// The test fixtures of `Abilities` that need a crate above its own.
pub trait AbilitiesFixtures {
    /// Loads the mode's types the tests spawn: `combatant`, of 500 health that stays when it
    /// dies, with no slot; `caster`, a combatant of 100 mana and 20 rage; and `walking
    /// caster`, a caster that walks a meter a tick.
    fn load_types(world: &mut World);

    /// Gives `unit_type` a combatant's books: no slot, and in a match with combat, 500 health
    /// that stays when it dies.
    fn give_combatant(world: &mut World, unit_type: UnitType);

    /// Gives `unit_type` a combatant's books, and pools of `mana` and `rage`.
    fn give_caster(world: &mut World, unit_type: UnitType, mana: i64, rage: i64);

    /// The unit of `spawn`, placed, that attacks `target`.
    fn attacker(world: &mut World, spawn: Spawn, target: StableId) -> StableId;

    /// Player 0's `caster` at the origin, on team 0, with `action` at `rank`.
    fn caster(world: &mut World, action: ActionId, rank: u8) -> StableId;

    /// What a caster spawns with: player 0 as its owner, and `action` at `rank` in its slot 0.
    fn caster_parts(action: ActionId, rank: u8) -> (Owner, TimedSlots);

    /// Loads `data` as the action `name` of package 0, of 5 ranks, with its script `source`.
    fn load(world: &mut World, name: &str, data: &ActionData, source: &str) -> ActionId;

    /// Loads `data` as the weapon `name` of package 0, of one rank, with its effect lists.
    fn load_weapon(world: &mut World, name: &str, data: &ActionData) -> ActionId;

    /// A match of two players of `declared`, whose scripts run within `limits`, and who hold
    /// gold, as a mode would keep it, with the damage kinds and pools of the MOBA test content
    /// and the types the tests spawn.
    fn match_of(limits: ScriptLimits, declared: &[Capability]) -> Match;

    /// A unit-targeted ability: `damage` true damage to an enemy within 5 m, every second, for 10
    /// mana and 4 rage.
    fn strike() -> ActionData;
}

impl AbilitiesFixtures for Abilities {
    fn load_types(world: &mut World) {
        let core = UnitTypeCore::default();
        let combatant = Units::load_type(world, TypeScope::Mode, "combatant", &core);
        Self::give_combatant(world, combatant);
        for name in ["caster", "walking caster"] {
            let caster = Units::load_type(world, TypeScope::Mode, name, &core);
            Self::give_caster(world, caster, 100, 20);
        }
        let walking = Units::type_named(world, "walking caster").unwrap();
        Units::give_type(world, walking, MoveStep::new(Num::ONE).unwrap());
    }

    fn give_combatant(world: &mut World, unit_type: UnitType) {
        if world
            .resource::<MatchFacts>()
            .set
            .contains(Capability::Combat)
        {
            Units::give_type(world, unit_type, Life::full(Num::int(500)));
            Units::give_type(world, unit_type, OnDeath::Stay);
        }
        Units::give_type(world, unit_type, ActionSlots::new([]));
    }

    fn give_caster(world: &mut World, unit_type: UnitType, mana: i64, rage: i64) {
        Self::give_combatant(world, unit_type);
        let pools = [(MANA, mana), (RAGE, rage)];
        let pools = Pools::new(pools.map(|(pool, max)| (pool, Num::int(max)))).unwrap();
        Units::give_type(world, unit_type, pools);
    }

    fn attacker(world: &mut World, spawn: Spawn, target: StableId) -> StableId {
        let id = spawn.place(world);
        ProgressDues::change_unit(world, id, |progress, dues| {
            progress.set_attack_target(Some(AttackTarget::ordered(target)), dues);
        });
        id
    }

    fn caster(world: &mut World, action: ActionId, rank: u8) -> StableId {
        Spawn::named(world, "caster", Vec3::ZERO)
            .parts(Self::caster_parts(action, rank))
            .place(world)
    }

    fn caster_parts(action: ActionId, rank: u8) -> (Owner, TimedSlots) {
        let slot = (action, SlotKind::new(0), Rank::new(rank));
        (
            Owner::new(PlayerSlot::new(0)),
            TimedSlots::new(ActionSlots::new([slot])),
        )
    }

    fn load(world: &mut World, name: &str, data: &ActionData, source: &str) -> ActionId {
        let script = Units::compile_hooked(world, source).unwrap();
        Actions::load(world, PackageIndex::MODE, name, data, Some(script), 5).unwrap()
    }

    fn load_weapon(world: &mut World, name: &str, data: &ActionData) -> ActionId {
        let weapon = Actions::load(world, PackageIndex::MODE, name, data, None, 1).unwrap();
        Actions::load_effect_lists(world, weapon, PackageIndex::MODE, data);
        weapon
    }

    fn match_of(limits: ScriptLimits, declared: &[Capability]) -> Match {
        let mut game = Match::builder(declared).players(2).limits(limits).build();
        // The damage kinds a mode would declare: the MOBA test content's.
        Units::name_kinds(&mut game, &["physical", "magic", "true"], &POOLS, &["gold"]);
        Players::spawn(&mut game, 2, 1);
        Self::load_types(&mut game);
        game
    }

    fn strike() -> ActionData {
        let targeting = || Targeting::Unit(FilterData::parse("enemies").unwrap());
        let int = |value| Ranked::One(Number::Value(Scalar::Int(value)));
        ActionData {
            keys: ActionKeys {
                script: Some(PackagePath::parse("strike.rhai").unwrap()),
                range: Some(Ranked::One(RangeField::Range(ActionRange::meters(
                    Num::int(5),
                )))),
                cooldown_ms: Some(int(1001)),
                cost: BTreeMap::from([
                    (DeclaredName::new("mana").unwrap(), int(10)),
                    (DeclaredName::new("rage").unwrap(), int(4)),
                ]),
                params: BTreeMap::from([(
                    DeclaredName::new("damage").unwrap(),
                    Param::Ranked(Ranked::One(Scalar::Int(50))),
                )]),
                ..ActionData::of(targeting(), CastData::default()).keys
            },
            ..ActionData::of(targeting(), CastData::default())
        }
    }
}
