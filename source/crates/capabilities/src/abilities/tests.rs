use std::collections::BTreeMap;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::component::Component;
use campfire_content::PackagePath;
use campfire_math::SegmentSeed;
use campfire_script::{NumError, ScriptError, ScriptLimits};
use campfire_sim::{IdAllocator, SimUpdate, TickInput, TickInputs};

use super::*;
use crate::abilities::ability_data::{Ranked, Scaling};
use crate::abilities::ability_slots::AbilitySlot;
use crate::combat::Combat;
use crate::combat::attack_stats::AttackStats;
use crate::combat::combatant::Combatant;
use crate::combat::health::Health;
use crate::combat::on_death::OnDeath;
use crate::control::Control;
use crate::control::controller::Controller;
use crate::control::order::{Action, Order};
use crate::navigation::Navigation;
use crate::units::Units;
use crate::units::error::ApiError;
use crate::units::relation::Relation;

const LIMITS: ScriptLimits = ScriptLimits {
    per_call: 10_000,
    per_tick: 100_000,
};
const RATE: TickRate = TickRate::new(30).unwrap();
const LASH_OUT: &str = include_str!("../../../../packages/moba/heroes/husk/scripts/lash_out.rhai");

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

/// Halves of a meter.
fn halves(value: i64) -> Num {
    Num::from_bits(value << (Num::FRAC_BITS - 1))
}

fn at(x: Num, y: Num, z: Num) -> Position {
    Position::new(Vec3::new(x, y, z)).unwrap()
}

fn combatant(health: i64) -> Combatant {
    Combatant {
        health: Health::new(num(health)).unwrap(),
        attack: AttackStats::new(Num::ZERO, 0, 1, Num::ZERO).unwrap(),
        on_death: OnDeath::Stay,
    }
}

/// Husk's Lash Out as its data declares it: no target, a cooldown of 10 s down to 6 s, 35 of the
/// caster's resource, and damage within 3.5 m of 75 to 175, plus half the caster's ability power.
fn lash_out() -> AbilityData {
    let ints = |values: &[i64]| values.iter().map(|&value| Scalar::Int(value)).collect();
    AbilityData {
        script: Some(PackagePath::parse("scripts/lash_out.rhai").unwrap()),
        targeting: Targeting::None,
        range: None,
        cooldown_ms: Some(Ranked::PerRank(vec![10_000, 9000, 8000, 7000, 6000])),
        cost: Some(Ranked::One(35)),
        cast_time_ms: None,
        params: BTreeMap::from([
            (
                "radius".to_owned(),
                Param::Value(Scalar::Decimal(halves(7))),
            ),
            (
                "damage".to_owned(),
                Param::Scaling(Scaling {
                    base: Ranked::PerRank(ints(&[75, 100, 125, 150, 175])),
                    per_level: None,
                    ad: None,
                    bonus_ad: None,
                    ap: Some(Scalar::Decimal(halves(1))),
                    max_health: None,
                    bonus_health: None,
                    armor: None,
                    magic_resist: None,
                }),
            ),
            ("cooldown_cut_ms".to_owned(), Param::Value(Scalar::Int(500))),
        ]),
    }
}

/// A unit-targeted ability: `damage` true damage to an enemy within 5 m, every second, for 10.
fn strike() -> AbilityData {
    AbilityData {
        script: Some(PackagePath::parse("strike.rhai").unwrap()),
        targeting: Targeting::Unit(Relation::Enemies),
        range: Some(Ranked::One(Range::Meters(num(5)))),
        cooldown_ms: Some(Ranked::One(1001)),
        cost: Some(Ranked::One(10)),
        cast_time_ms: None,
        params: BTreeMap::from([("damage".to_owned(), Param::Value(Scalar::Int(50)))]),
    }
}

const STRIKE: &str =
    r#"fn on_cast(ctx, caster, target) { ctx.damage(target, ctx.p.damage, "true"); }"#;

#[derive(Debug)]
struct Match {
    world: World,
    registry: StateRegistry,
}

impl Match {
    fn new() -> Match {
        let mut world = World::new();
        SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]));
        let mut schedule = SimUpdate::schedule();
        let mut registry = StateRegistry::new();
        Units::install(&mut world, &mut schedule, &mut registry, LIMITS, RATE);
        Combat::install(&mut world, &mut schedule, &mut registry);
        Navigation::install(&mut world, &mut schedule, &mut registry);
        Abilities::install(&mut world, &mut schedule, &mut registry);
        Control::install(&mut world, &mut schedule, &mut registry);
        world.add_schedule(schedule);
        Match { world, registry }
    }

    fn load(&mut self, data: &AbilityData, source: &str) -> AbilityId {
        Abilities::load(&mut self.world, data, Some(source)).unwrap()
    }

    fn spawn(&mut self, team: u8, at: Position, parts: impl Bundle) -> StableId {
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        self.world
            .spawn((id, at, combatant(500).bundle(Team::new(team)), parts));
        id
    }

    /// Player 0's caster at the origin, on team 0, with `ability` at `rank` and 100 resource.
    fn caster(&mut self, ability: AbilityId, rank: u8) -> StableId {
        self.spawn(
            0,
            at(Num::ZERO, Num::ZERO, Num::ZERO),
            (
                Controller::new(0),
                AbilitySlots::new([(ability, rank)]),
                ResourcePool::new(num(100)).unwrap(),
            ),
        )
    }

    fn cast(&mut self, unit: StableId, target: CastTarget) {
        let payload = Order::payload(&[Order {
            unit,
            action: Action::Cast { slot: 0, target },
        }]);
        self.world.resource_mut::<TickInputs>().push(TickInput {
            slot: 0,
            payload: &payload,
        });
        self.world.run_schedule(SimUpdate);
    }

    fn run_until(&mut self, tick: u64) {
        while self.world.resource::<SimTick>().get() < tick {
            self.world.run_schedule(SimUpdate);
        }
    }

    fn get<C: Component + Copy>(&self, id: StableId) -> C {
        let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
        *self.world.entity(entity).get::<C>().unwrap()
    }

    fn health(&self, id: StableId) -> i64 {
        self.get::<Health>(id).current().round()
    }

    fn pool(&self, id: StableId) -> i64 {
        self.get::<ResourcePool>(id).current().round()
    }

    fn slot(&self, id: StableId) -> AbilitySlot {
        let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
        self.world
            .entity(entity)
            .get::<AbilitySlots>()
            .unwrap()
            .slot(0)
            .unwrap()
    }

    fn failures(&self) -> &[ScriptFailure] {
        self.world.non_send::<ScriptFailures>().get()
    }
}

#[test]
fn lash_out_hits_every_enemy_within_its_radius_exactly() {
    let mut game = Match::new();
    // Strike's one param name comes first in the frame, so Lash Out's three follow from the
    // second.
    game.load(&strike(), STRIKE);
    let lash_out = game.load(&lash_out(), LASH_OUT);
    let husk = game.caster(lash_out, 2);
    let near = game.spawn(1, at(num(3), Num::ZERO, Num::ZERO), ());
    // At exactly 3.5 m: within the radius.
    let edge = game.spawn(1, at(Num::ZERO, Num::ZERO, halves(7)), ());
    let beyond = game.spawn(1, at(num(4), Num::ZERO, Num::ZERO), ());
    // Up at y = 9, 2 m away on the ground plane.
    let high = game.spawn(1, at(Num::ZERO, num(9), num(2)), ());
    let ally = game.spawn(0, at(num(1), Num::ZERO, Num::ZERO), ());
    let dead = game.spawn(1, at(num(1), Num::ZERO, num(1)), ());
    let entity = game.world.resource::<EntityIndex>().get(dead).unwrap();
    game.world.entity_mut(entity).insert(Dead);

    // Rank 2 deals 100, and 0.5 × 0 ability power: 500 → 400 for the three enemies in reach.
    // It costs 35 of 100, and its 9000 ms cooldown is 9 × 30 = 270 ticks.
    game.cast(husk, CastTarget::None);
    let healths =
        |game: &Match| [near, edge, beyond, high, ally, dead].map(|unit| game.health(unit));
    assert_eq!(healths(&game), [400, 400, 500, 400, 500, 500]);
    assert_eq!(game.pool(husk), 65);
    assert_eq!(game.slot(husk).ready_at, 270);
    assert!(game.failures().is_empty());

    // On cooldown until tick 270: a cast in tick 1 does nothing.
    game.cast(husk, CastTarget::None);
    assert_eq!(healths(&game), [400, 400, 500, 400, 500, 500]);
    assert_eq!(game.pool(husk), 65);
    game.run_until(270);
    game.cast(husk, CastTarget::None);
    assert_eq!(healths(&game), [300, 300, 500, 300, 500, 500]);
    assert_eq!(game.pool(husk), 30);
    // 30 left cannot pay 35.
    game.run_until(540);
    game.cast(husk, CastTarget::None);
    assert_eq!(healths(&game), [300, 300, 500, 300, 500, 500]);
    assert_eq!(game.pool(husk), 30);
}

#[test]
fn a_cast_passes_its_checks_or_does_nothing() {
    let mut game = Match::new();
    let strike = game.load(&strike(), STRIKE);
    let caster = game.caster(strike, 1);
    let unlearned = game.spawn(
        0,
        at(num(1), Num::ZERO, Num::ZERO),
        (
            Controller::new(0),
            AbilitySlots::new([(strike, 0)]),
            ResourcePool::new(num(100)).unwrap(),
        ),
    );
    let poor = game.spawn(
        0,
        at(num(2), Num::ZERO, Num::ZERO),
        (
            Controller::new(0),
            AbilitySlots::new([(strike, 1)]),
            ResourcePool::new(num(5)).unwrap(),
        ),
    );
    let enemy = game.spawn(1, at(num(5), Num::ZERO, Num::ZERO), ());
    let far = game.spawn(1, at(num(6), Num::ZERO, Num::ZERO), ());
    let ally = game.spawn(0, at(num(1), Num::ZERO, num(1)), ());

    // An ally, a unit beyond 5 m, and no target at all are refused; so are a slot not learned
    // and a pool of 5 against a cost of 10.
    for (unit, target) in [
        (caster, CastTarget::Unit(ally)),
        (caster, CastTarget::Unit(far)),
        (caster, CastTarget::None),
        (unlearned, CastTarget::Unit(enemy)),
        (poor, CastTarget::Unit(enemy)),
    ] {
        game.cast(unit, target);
        assert_eq!(game.health(enemy), 500, "{unit:?} at {target:?}");
    }
    assert_eq!([game.health(far), game.health(ally)], [500, 500]);
    assert_eq!(game.pool(caster), 100);

    // At exactly 5 m, the enemy takes 50. The cooldown, 1001 ms, is 30.03 ticks, rounded up to
    // 31: the cast in tick 5 is ready again in tick 36.
    game.cast(caster, CastTarget::Unit(enemy));
    assert_eq!(game.health(enemy), 450);
    assert_eq!(game.pool(caster), 90);
    assert_eq!(game.slot(caster).ready_at, 36);
}

#[test]
fn a_failed_script_changes_nothing_and_fails_the_same_way_everywhere() {
    let spin = r#"fn on_cast(ctx, caster, target) { for unit in ctx.find(caster, caster.pos, 10, "enemies") { ctx.damage(unit, 50, "magic"); } loop {} }"#;
    let wrong_kind = r#"fn on_cast(ctx, caster, target) { for unit in ctx.find(caster, caster.pos, 10, "enemies") { ctx.damage(unit, 50, "fire"); } }"#;
    let overflow = r#"fn on_cast(ctx, caster, target) { for unit in ctx.find(caster, caster.pos, 10, "enemies") { ctx.damage(unit, 50, "magic"); } num(1 << 20) * num(1 << 20) }"#;
    let undeclared = r#"fn on_cast(ctx, caster, target) { for unit in ctx.find(caster, caster.pos, 10, "enemies") { ctx.damage(unit, 50, "magic"); } ctx.p.radius }"#;
    let data = AbilityData {
        params: BTreeMap::new(),
        ..lash_out()
    };
    let cases: [(&str, fn(&CallError) -> bool); 4] = [
        (spin, |error| {
            matches!(error, CallError::Script(ScriptError::CallLimit))
        }),
        (wrong_kind, |error| {
            matches!(error, CallError::Api(ApiError::UnknownDamageKind))
        }),
        (undeclared, |error| {
            matches!(error, CallError::Api(ApiError::UnknownParam))
        }),
        (overflow, |error| match error {
            CallError::Script(ScriptError::Raised(raised)) => {
                raised.get::<NumError>() == Some(NumError::Overflow)
            }
            _ => false,
        }),
    ];
    for (script, expected) in cases {
        // Two matches alike, but only one gets the cast order.
        let mut hashes = Vec::new();
        let mut spent = Vec::new();
        for ordered in [true, false] {
            let mut game = Match::new();
            let ability = game.load(&data, script);
            let caster = game.caster(ability, 1);
            let enemy = game.spawn(1, at(num(1), Num::ZERO, Num::ZERO), ());
            if ordered {
                game.cast(caster, CastTarget::None);
                let failures = game.failures();
                assert_eq!(failures.len(), 1, "{script}");
                assert_eq!(failures[0].unit, caster);
                assert_eq!(failures[0].hook, Hook::OnCast);
                assert!(expected(&failures[0].error), "{:?}", failures[0].error);
                spent.push(game.world.non_send::<ScriptHost>().spent());
            } else {
                game.world.run_schedule(SimUpdate);
            }
            assert_eq!(game.health(enemy), 500);
            assert_eq!(game.pool(caster), 100);
            assert_eq!(game.slot(caster).ready_at, 0);
            hashes.push(game.registry.hash(&game.world));
        }
        assert_eq!(hashes[0], hashes[1], "{script}");
        if script == spin {
            // The call ran exactly its limit of operations.
            assert_eq!(spent, [LIMITS.per_call]);
        }
    }
}

#[test]
fn an_ability_loads_only_when_its_data_holds() {
    let mut game = Match::new();
    let load = |game: &mut Match, data: &AbilityData, source: Option<&str>| {
        Abilities::load(&mut game.world, data, source)
    };
    let mut uneven = lash_out();
    uneven.cost = Some(Ranked::PerRank(vec![35, 40]));
    let mut aimed = lash_out();
    aimed.targeting = Targeting::Direction;
    let mut unscripted = lash_out();
    unscripted.script = None;
    let mut forever = lash_out();
    forever.cooldown_ms = Some(Ranked::One(u64::MAX));
    let cases: [(AbilityData, Option<&str>, fn(&AbilityError) -> bool); 5] = [
        (uneven, Some(LASH_OUT), |error| {
            matches!(error, AbilityError::RankCounts)
        }),
        (aimed, Some(LASH_OUT), |error| {
            matches!(
                error,
                AbilityError::UnsupportedTargeting(Targeting::Direction)
            )
        }),
        (unscripted, Some(LASH_OUT), |error| {
            matches!(error, AbilityError::ScriptMismatch)
        }),
        (lash_out(), None, |error| {
            matches!(error, AbilityError::ScriptMismatch)
        }),
        (forever, Some(LASH_OUT), |error| {
            matches!(error, AbilityError::TimeTooLarge)
        }),
    ];
    for (data, source, expected) in cases {
        let error = load(&mut game, &data, source).unwrap_err();
        assert!(expected(&error), "{error:?}");
    }
    assert!(matches!(
        load(&mut game, &lash_out(), Some("fn on_cast(")),
        Err(AbilityError::Script(ScriptError::Compile(_)))
    ));
    assert!(load(&mut game, &lash_out(), Some(LASH_OUT)).is_ok());

    // A script may serve only the ability's modifiers: a cast of rank 1 in tick 0 then runs no
    // script, and spends 35 of 100 and its 10 000 ms, 300 ticks at 30 a second.
    let modifiers_only = "fn on_damage_taken(ctx, m, d) { }";
    let passive = load(&mut game, &lash_out(), Some(modifiers_only)).unwrap();
    let caster = game.caster(passive, 1);
    game.cast(caster, CastTarget::None);
    assert!(game.failures().is_empty());
    assert_eq!(game.pool(caster), 65);
    assert_eq!(game.slot(caster).ready_at, 300);
}
