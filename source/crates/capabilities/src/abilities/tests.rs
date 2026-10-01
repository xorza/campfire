use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::rc::Rc;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::component::Component;
use bevy_ecs::world::Mut;
use campfire_content::PackagePath;
use campfire_math::{PlayerSlot, Vec3};
use campfire_script::{NumError, ScriptError};
use campfire_sim::{Capability, IdAllocator, SimUpdate};

use super::*;
use crate::abilities::ability_data::RangeField;
use crate::abilities::ability_slots::AbilitySlot;
use crate::abilities::error::AbilityField;
use crate::capability_set::internals::TestMatch;
use crate::combat::assist_window::AssistWindow;
use crate::combat::attack_state::AttackState;
use crate::combat::attack_stats::AttackStats;
use crate::combat::combatant::Combatant;
use crate::combat::damage::{Damage, DamageCause};
use crate::combat::damage_kind::DamageKind;
use crate::combat::damage_queue::DamageQueue;
use crate::combat::health::Health;
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;
use crate::orders::Orders;
use crate::orders::ai_data::AiData;
use crate::scripts::error::ApiError;
use crate::scripts::match_scripts::MatchScripts;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_failures::{ScriptFailure, ScriptFailures};
use crate::scripts::script_limits::ScriptLimits;
use crate::stats::Stats;
use crate::stats::modifier_data::{ModifierData, Reapply};
use crate::stats::modifier_effect::ModifierEffect;
use crate::units::Units;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::declared_name::DeclaredName;
use crate::values::filter_data::FilterData;
use crate::values::number::{Number, ParamRef};
use crate::values::param::Param;
use crate::values::param::Scaling;
use crate::values::ranked::Ranked;
use crate::values::scalar::Scalar;

/// The MOBA's 30 ticks a second.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

const LIMITS: ScriptLimits = ScriptLimits {
    per_call: 10_000,
    player: 100_000,
    think: 100_000,
    mode: 100_000,
};
/// Lash Out as the reference Husk had it when these tests were written: the engine's tests keep
/// their own copy, so a balance change to the reference hero changes none of them.
const LASH_OUT: &str = r#"
fn on_cast(ctx, caster, target) {
    for unit in ctx.find(caster, caster.pos, ctx.p.radius, "enemies") {
        ctx.damage(unit, ctx.p.damage, "magic");
    }
}

fn on_damage_taken(ctx, m, d) {
    if d.attack {
        ctx.reduce_cooldown(m.carrier, "lash_out", ctx.p.cooldown_cut_ms);
    }
}
"#;

fn int(value: i64) -> Number {
    Number::Value(Scalar::Int(value))
}

fn param(name: &str) -> Number {
    Number::Param(ParamRef {
        param: name.to_owned(),
    })
}

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
        attack: Some(AttackStats::new(Num::ZERO, Ticks::new(0), Ticks::new(1), Num::ZERO).unwrap()),
        on_death: OnDeath::Stay,
    }
}

/// Husk's Lash Out as its data declares it: no target, a cooldown of 10 s down to 6 s, 35 of the
/// caster's resource, and damage within 3.5 m of 75 to 175, plus half the caster's ability power.
fn lash_out() -> AbilityData {
    let scalars = |values: &[i64]| values.iter().map(|&value| Scalar::Int(value)).collect();
    AbilityData {
        script: Some(PackagePath::parse("scripts/lash_out.rhai").unwrap()),
        targeting: Targeting::None,
        range: None,
        cooldown_ms: Some(Ranked::PerRank(
            [10_000, 9000, 8000, 7000, 6000].map(int).to_vec(),
        )),
        cost: Some(Ranked::One(int(35))),
        cast_time_ms: None,
        clamp_to_range: false,
        toggle: None,
        channel: None,
        hold: None,
        charges: None,
        charge: None,
        passive_modifier: None,
        passive_while_ready: false,
        projectile: None,
        area: None,
        projectile_state: BTreeMap::new(),
        params: BTreeMap::from([
            (
                "radius".to_owned(),
                Param::Ranked(Ranked::One(Scalar::Decimal(halves(7)))),
            ),
            (
                "damage".to_owned(),
                Param::Scaling(Scaling {
                    base: Ranked::PerRank(scalars(&[75, 100, 125, 150, 175])),
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
            (
                "cooldown_cut_ms".to_owned(),
                Param::Ranked(Ranked::One(Scalar::Int(500))),
            ),
        ]),
    }
}

/// A unit-targeted ability: `damage` true damage to an enemy within 5 m, every second, for 10.
fn strike() -> AbilityData {
    AbilityData {
        script: Some(PackagePath::parse("strike.rhai").unwrap()),
        targeting: Targeting::Unit(FilterData::parse("enemies").unwrap()),
        range: Some(Ranked::One(RangeField::Range(Range::Meters(num(5))))),
        cooldown_ms: Some(Ranked::One(int(1001))),
        cost: Some(Ranked::One(int(10))),
        cast_time_ms: None,
        clamp_to_range: false,
        toggle: None,
        channel: None,
        hold: None,
        charges: None,
        charge: None,
        passive_modifier: None,
        passive_while_ready: false,
        projectile: None,
        area: None,
        projectile_state: BTreeMap::new(),
        params: BTreeMap::from([(
            "damage".to_owned(),
            Param::Ranked(Ranked::One(Scalar::Int(50))),
        )]),
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
        Match::with(LIMITS, &[Capability::Combat, Capability::Abilities])
    }

    /// A match of two players of `declared`, whose scripts run within `limits`.
    fn with(limits: ScriptLimits, declared: &[Capability]) -> Match {
        // The damage kinds a mode would declare: the reference MOBA's.
        let kinds = ["physical", "magic", "true"].map(|kind| DeclaredName::new(kind).unwrap());
        let scripts = MatchScripts {
            limits,
            players: 2,
            damage_kinds: Rc::from(kinds),
        };
        let TestMatch {
            mut world,
            schedule,
            registry,
        } = TestMatch::new(declared, RATE, Some(scripts));
        world.add_schedule(schedule);
        Match { world, registry }
    }

    fn load(&mut self, data: &AbilityData, source: &str) -> AbilityId {
        let script = Units::compile(&mut self.world, source).unwrap();
        Abilities::load(&mut self.world, 0, "lash_out", data, Some(script), 5).unwrap()
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
                Owner::new(PlayerSlot::new(0)),
                AbilitySlots::new([(ability, rank)]),
                ResourcePool::new(num(100)).unwrap(),
            ),
        )
    }

    fn cast(&mut self, unit: StableId, target: CastTarget) {
        self.casts(&[(unit, target)]);
    }

    /// Runs a tick in which each unit casts its first slot's ability at its target, as an order
    /// would make it.
    fn casts(&mut self, casts: &[(StableId, CastTarget)]) {
        for &(unit, target) in casts {
            let entity = self.world.resource::<EntityIndex>().get(unit).unwrap();
            let mut slots = self.world.get_mut::<AbilitySlots>(entity).unwrap();
            slots.order(0, target);
        }
        self.world.run_schedule(SimUpdate);
    }

    fn run_until(&mut self, tick: u64) {
        while self.world.resource::<SimTick>().start().get() < tick {
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
fn damage_of_a_kind_the_mode_does_not_declare_fails_the_cast() {
    let mut game = Match::new();
    let fire = game.load(&lash_out(), &LASH_OUT.replace(r#""magic""#, r#""fire""#));
    let husk = game.caster(fire, 2);
    let near = game.spawn(1, at(num(3), Num::ZERO, Num::ZERO), ());
    game.cast(husk, CastTarget::None);
    // The call fails, so the cast applies nothing: no damage, no cost.
    assert_eq!((game.health(near), game.pool(husk)), (500, 100));
    let errors: Vec<_> = game
        .failures()
        .iter()
        .map(|failure| &failure.error)
        .collect();
    assert!(
        matches!(errors[..], [CallError::Api(ApiError::UnknownDamageKind)]),
        "{errors:?}"
    );
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
    assert_eq!(game.slot(husk).ready_at, Tick::new(270));
    assert!(game.failures().is_empty());

    // On cooldown until tick 270: a cast in tick 1 does nothing.
    game.cast(husk, CastTarget::None);
    assert_eq!(healths(&game), [400, 400, 500, 400, 500, 500]);
    assert_eq!(game.pool(husk), 65);
    // An ability that takes no target ignores the one its order names: the cast at the ally hits
    // the same three enemies.
    game.run_until(270);
    game.cast(husk, CastTarget::Unit(ally));
    assert_eq!(healths(&game), [300, 300, 500, 300, 500, 500]);
    assert_eq!(game.pool(husk), 30);
    // 30 left cannot pay 35.
    game.run_until(540);
    game.cast(husk, CastTarget::None);
    assert_eq!(healths(&game), [300, 300, 500, 300, 500, 500]);
    assert_eq!(game.pool(husk), 30);
}

#[test]
fn ai_load_does_not_spend_what_a_cast_needs() {
    // The one test of how abilities and `orders` meet: AI's calls and a cast's draw from pools of
    // their own.
    let declared = [
        Capability::Combat,
        Capability::Navigation,
        Capability::Abilities,
        Capability::Orders,
    ];
    let mut game = Match::with(LIMITS, &declared);
    let strike = game.load(&strike(), STRIKE);
    let caster = game.caster(strike, 1);
    let enemy = game.spawn(1, at(num(5), Num::ZERO, Num::ZERO), ());
    // Eleven units whose AI spins, all due in every tick: ten calls fail at the 10 000 limit and
    // spend the 100 000 of the think pool, and the eleventh finds it spent.
    let spinner = Units::load_type(&mut game.world, "spinner", &UnitTypeData::default()).unwrap();
    let ai = AiData {
        ai: PackagePath::parse("scripts/ai.rhai").unwrap(),
        think_ms: 1,
    };
    let spin = "fn think(ctx, unit) { loop {} }";
    let spin = Units::compile(&mut game.world, spin).unwrap();
    Orders::load_ai(&mut game.world, spinner, &ai, spin).unwrap();
    for z in 0..11 {
        game.spawn(2, at(Num::ZERO, Num::ZERO, num(20 + z)), spinner);
    }

    // The cast in the same tick draws from its player's pool, whole: 500 → 450, 100 → 90.
    game.cast(caster, CastTarget::Unit(enemy));
    assert_eq!(game.health(enemy), 450);
    assert_eq!(game.pool(caster), 90);
    let failures = game.failures();
    assert_eq!(failures.len(), 10);
    assert!(failures.iter().all(|failure| failure.hook == Hook::Think));
    let mut budgets = game.world.resource_mut::<ScriptBudgets>();
    assert_eq!(budgets.get_mut(Pool::Think).left(), 0);
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
            Owner::new(PlayerSlot::new(0)),
            AbilitySlots::new([(strike, 0)]),
            ResourcePool::new(num(100)).unwrap(),
        ),
    );
    let poor = game.spawn(
        0,
        at(num(2), Num::ZERO, Num::ZERO),
        (
            Owner::new(PlayerSlot::new(0)),
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
    assert_eq!(game.slot(caster).ready_at, Tick::new(36));

    // The range counts from the edge of each body: once the unit 6 m off has a body of 1 m, it
    // is within 5 m, and takes 50 in tick 36.
    let far_entity = game.world.resource::<EntityIndex>().get(far).unwrap();
    game.world
        .entity_mut(far_entity)
        .insert(Body::new(Num::ONE).unwrap());
    game.run_until(36);
    game.cast(caster, CastTarget::Unit(far));
    assert_eq!(game.health(far), 450);
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
                assert_eq!(failures[0].unit, Some(caster));
                assert_eq!(failures[0].hook, Hook::OnCast);
                assert!(expected(&failures[0].error), "{:?}", failures[0].error);
                let mut budgets = game.world.resource_mut::<ScriptBudgets>();
                spent
                    .push(LIMITS.player - budgets.get_mut(Pool::Player(PlayerSlot::new(0))).left());
            } else {
                game.world.run_schedule(SimUpdate);
            }
            assert_eq!(game.health(enemy), 500);
            assert_eq!(game.pool(caster), 100);
            assert_eq!(game.slot(caster).ready_at, Tick::new(0));
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
fn a_cast_draws_from_its_casters_player_pool() {
    // Each player's pool holds one whole call: player 0's spinning cast spends all of its pool,
    // and player 1's strike in the same tick still resolves from its own, which it spends as it
    // does alone.
    let mut left = Vec::new();
    for spins in [true, false] {
        let limits = ScriptLimits {
            player: LIMITS.per_call,
            ..LIMITS
        };
        let mut game = Match::with(limits, &[Capability::Combat, Capability::Abilities]);
        let data = AbilityData {
            params: BTreeMap::new(),
            ..lash_out()
        };
        let spin = game.load(&data, "fn on_cast(ctx, caster, target) { loop {} }");
        let strike = game.load(&strike(), STRIKE);
        let spinner = game.caster(spin, 1);
        let striker = game.spawn(
            0,
            at(Num::ZERO, Num::ZERO, num(1)),
            (
                Owner::new(PlayerSlot::new(1)),
                AbilitySlots::new([(strike, 1)]),
                ResourcePool::new(num(100)).unwrap(),
            ),
        );
        let enemy = game.spawn(1, at(num(1), Num::ZERO, Num::ZERO), ());
        let casts = [
            (spinner, CastTarget::None),
            (striker, CastTarget::Unit(enemy)),
        ];
        game.casts(if spins { &casts } else { &casts[1..] });
        let failures = game.failures();
        assert_eq!(failures.len(), usize::from(spins));
        if spins {
            assert_eq!(failures[0].unit, Some(spinner));
            assert!(matches!(
                failures[0].error,
                CallError::Script(ScriptError::CallLimit)
            ));
        }
        // The strike's 50 true damage: 500 → 450, and its cost of 10: 100 → 90.
        assert_eq!(game.health(enemy), 450);
        assert_eq!(game.pool(striker), 90);
        let mut budgets = game.world.resource_mut::<ScriptBudgets>();
        let spent = if spins { 0 } else { LIMITS.per_call };
        assert_eq!(
            budgets.get_mut(Pool::Player(PlayerSlot::new(0))).left(),
            spent
        );
        left.push(budgets.get_mut(Pool::Player(PlayerSlot::new(1))).left());
    }
    assert_eq!(left[0], left[1]);
    assert!(left[0] < LIMITS.per_call);
}

#[test]
fn an_ability_loads_only_when_its_data_holds() {
    let mut game = Match::new();
    let load = |game: &mut Match, data: &AbilityData, source: &str| {
        let script = Units::compile(&mut game.world, source).unwrap();
        Abilities::load(&mut game.world, 0, "lash_out", data, Some(script), 5)
    };
    let mut uneven = lash_out();
    uneven.cost = Some(Ranked::PerRank(vec![int(35), int(40)]));
    let mut aimed = lash_out();
    aimed.targeting = Targeting::Direction;
    let mut forever = lash_out();
    forever.cooldown_ms = Some(Ranked::One(int(i64::MAX)));
    let mut scaled = lash_out();
    scaled.cooldown_ms = Some(Ranked::One(param("damage")));
    let mut negative = lash_out();
    negative.cost = Some(Ranked::One(int(-1)));
    let mut unknown = lash_out();
    unknown.range = Some(Ranked::One(RangeField::Param(ParamRef {
        param: "reach".to_owned(),
    })));
    // The data's rules, which the package load checks: two costs for five ranks, and the field
    // that does not hold at rank 1. A scaling param is the caster's value, which no cooldown may
    // take.
    assert!(lash_out().check_ranks(5) && !uneven.check_ranks(5));
    for (data, field) in [
        (scaled, AbilityField::Cooldown),
        (negative, AbilityField::Cost),
        (unknown, AbilityField::Range),
    ] {
        assert_eq!(data.fields_at(1).err(), Some(field), "{field:?}");
    }
    // What only a match's rate decides: i64::MAX ms counts in no tick.
    assert!(matches!(
        load(&mut game, &forever, LASH_OUT),
        Err(AbilityError::TimeTooLarge)
    ));
    assert!(load(&mut game, &lash_out(), LASH_OUT).is_ok());
    // A direction loads, as every targeting does; no cast can aim one yet.
    assert!(load(&mut game, &aimed, LASH_OUT).is_ok());

    // A script may serve only the ability's modifiers: a cast of rank 1 in tick 0 then runs no
    // script, and spends 35 of 100 and its 10 000 ms, 300 ticks at 30 a second.
    let modifiers_only = "fn on_damage_taken(ctx, m, d) { }";
    let passive = load(&mut game, &lash_out(), modifiers_only).unwrap();
    let caster = game.caster(passive, 1);
    game.cast(caster, CastTarget::None);
    assert!(game.failures().is_empty());
    assert_eq!(game.pool(caster), 65);
    assert_eq!(game.slot(caster).ready_at, Tick::new(300));
}

#[test]
fn a_capability_field_reads_its_param_at_each_rank() {
    // A cooldown of `{ param = "cd" }`, 1000 ms at rank 1 up to 3000 at rank 3: 30, 60 and 90
    // ticks at 30 a second, and a cost of `{ param = "price" }`, 7 at every rank.
    let mut data = strike();
    data.cooldown_ms = Some(Ranked::One(param("cd")));
    data.cost = Some(Ranked::PerRank(vec![int(5), param("price"), int(9)]));
    let per_rank = |values: &[i64]| {
        Param::Ranked(Ranked::PerRank(
            values.iter().map(|&value| Scalar::Int(value)).collect(),
        ))
    };
    data.params
        .insert("cd".to_owned(), per_rank(&[1000, 2000, 3000]));
    data.params.insert(
        "price".to_owned(),
        Param::Ranked(Ranked::One(Scalar::Int(7))),
    );
    let mut game = Match::new();
    let strike = Units::compile(&mut game.world, STRIKE).unwrap();
    let id = Abilities::load(&mut game.world, 0, "strike", &data, Some(strike), 3).unwrap();
    let book = game.world.resource::<AbilityBook>();
    let ranks: Vec<_> = book
        .get(id)
        .unwrap()
        .ranks
        .iter()
        .map(|values| (values.cooldown.get(), values.cost))
        .collect();
    assert_eq!(ranks, [(30, 5), (60, 7), (90, 9)]);
    // Three ranks of an array of 3 is the rank count; five is not.
    assert!(data.check_ranks(3) && !data.check_ranks(5));
}

#[test]
fn a_unit_target_is_one_its_filter_selects_tag_and_all() {
    let mut game = Match::new();
    let mut load_type = |name: &str| {
        let data = UnitTypeData {
            tags: vec![name.to_owned()],
            params: BTreeMap::new(),
        };
        Units::load_type(&mut game.world, name, &data).unwrap()
    };
    let (hero, creep) = (load_type("avatar"), load_type("creep"));
    let mut heroes_only = strike();
    heroes_only.targeting = Targeting::Unit(FilterData::parse("enemies:avatar").unwrap());
    let strike = game.load(&heroes_only, STRIKE);
    let caster = game.caster(strike, 1);
    let enemy_creep = game.spawn(1, at(num(3), Num::ZERO, Num::ZERO), creep);
    let enemy_hero = game.spawn(1, at(num(4), Num::ZERO, Num::ZERO), hero);
    // The creep is an enemy, but no hero: the cast goes nowhere. The hero takes 50.
    game.cast(caster, CastTarget::Unit(enemy_creep));
    assert_eq!(game.health(enemy_creep), 500);
    game.cast(caster, CastTarget::Unit(enemy_hero));
    assert_eq!(game.health(enemy_hero), 450);
}

#[test]
fn a_passive_is_held_while_its_ability_has_a_rank_and_is_ready() {
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = Match::with(LIMITS, &declared);
    let stats = StatBook::new(&BTreeMap::new(), [], RATE, num(6)).unwrap();
    Stats::load(&mut game.world, stats);
    // A guard whose shield is Lash Out's damage at its rank: 75, then 100.
    let guard = ModifierData {
        script: None,
        duration_ms: None,
        interval_ms: None,
        stacks_expire_ms: None,
        reapply: Reapply::Refresh,
        max_stacks: None,
        stats: BTreeMap::new(),
        states: Vec::new(),
        shield: Some(param("damage")),
        aura: None,
        params: BTreeMap::new(),
        state: BTreeMap::new(),
    };
    Stats::load_modifier(&mut game.world, 0, "guard", &guard, None);
    let mut data = lash_out();
    data.passive_modifier = Some("guard".to_owned());
    data.passive_while_ready = true;
    let ability = game.load(&data, LASH_OUT);
    let caster = game.caster(ability, 0);
    let entity = game.world.resource::<EntityIndex>().get(caster).unwrap();
    game.world.entity_mut(entity).insert(Modifiers::default());
    let id = game
        .world
        .resource::<ModifierBook>()
        .find(0, "guard")
        .unwrap();
    let shield = |game: &Match| {
        let modifiers = game.world.get::<Modifiers>(entity).unwrap();
        let held = modifiers.get(id, Some(caster))?;
        assert!(held.passive && held.until.is_none());
        held.shield
    };
    let learn = |game: &mut Match| {
        let mut slots = game.world.get_mut::<AbilitySlots>(entity).unwrap();
        slots.learn(0);
        game.world.run_schedule(SimUpdate);
    };
    // Unlearned, none; at rank 1, the shield of 75; at rank 2, applied again, 100.
    game.world.run_schedule(SimUpdate);
    assert_eq!(shield(&game), None);
    learn(&mut game);
    assert_eq!(shield(&game), Some(num(75)));
    learn(&mut game);
    assert_eq!(shield(&game), Some(num(100)));
    // A cast puts it on cooldown for 9000 ms at rank 2, 270 ticks: gone from that tick, back in
    // the 270th after it.
    game.cast(caster, CastTarget::None);
    assert_eq!(shield(&game), None);
    for _ in 0..269 {
        game.world.run_schedule(SimUpdate);
    }
    assert_eq!(shield(&game), None);
    game.world.run_schedule(SimUpdate);
    assert_eq!(shield(&game), Some(num(100)));
}

#[test]
fn a_cast_applies_a_modifier_from_its_caster_with_its_abilitys_params() {
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = Match::with(LIMITS, &declared);
    let stats = StatBook::new(&BTreeMap::new(), [], RATE, num(6)).unwrap();
    Stats::load(&mut game.world, stats);
    let mark = ModifierData {
        script: None,
        duration_ms: Some(int(1000)),
        interval_ms: None,
        stacks_expire_ms: None,
        reapply: Reapply::Refresh,
        max_stacks: None,
        stats: BTreeMap::new(),
        states: Vec::new(),
        shield: Some(param("damage")),
        aura: None,
        params: BTreeMap::new(),
        state: BTreeMap::new(),
    };
    Stats::load_modifier(&mut game.world, 0, "mark", &mark, None);
    let marker = r#"
fn on_cast(ctx, caster, target) {
    let m = ctx.add_modifier(caster, "mark");
    if m.stacks != 1 { throw "a new modifier's handle has one stack"; }
}
"#;
    let ability = game.load(&lash_out(), marker);
    let caster = game.caster(ability, 3);
    let entity = game.world.resource::<EntityIndex>().get(caster).unwrap();
    game.world.entity_mut(entity).insert(Modifiers::default());
    let t = game.world.resource::<SimTick>().start();
    game.cast(caster, CastTarget::None);
    // From the caster, by Lash Out at rank 3: a shield of its damage there, 125; 1000 ms at 30
    // ticks a second, 30 ticks, so it ends as tick t + 31 starts.
    let id = game
        .world
        .resource::<ModifierBook>()
        .find(0, "mark")
        .unwrap();
    let modifiers = game.world.get::<Modifiers>(entity).unwrap();
    let held = modifiers.get(id, Some(caster)).unwrap();
    assert_eq!((held.ability, held.rank), (Some(ability), 3));
    assert_eq!(
        (held.shield, held.until),
        (Some(num(125)), Some(Tick::new(t.get() + 31)))
    );
}

#[test]
fn a_cast_heals_and_restores_and_a_negative_amount_fails_it() {
    let mut game = Match::new();
    let mender = "
fn on_cast(ctx, caster, target) {
    ctx.heal(caster, 30);
    ctx.restore(caster, num(20));
}
";
    let ability = game.load(&lash_out(), mender);
    let caster = game.caster(ability, 1);
    let entity = game.world.resource::<EntityIndex>().get(caster).unwrap();
    game.world.get_mut::<Health>(entity).unwrap().take(num(460));
    game.world
        .get_mut::<ResourcePool>(entity)
        .unwrap()
        .spend(num(50));
    // From 40 health and 50 resource: 30 healed and 20 restored as the effects apply, then the
    // cost of 35: 70 and 35.
    game.cast(caster, CastTarget::None);
    assert_eq!((game.health(caster), game.pool(caster)), (70, 35));

    let mut game = Match::new();
    let negative =
        "fn on_cast(ctx, caster, target) { ctx.restore(caster, 5); ctx.heal(caster, -1); }";
    let ability = game.load(&lash_out(), negative);
    let caster = game.caster(ability, 1);
    game.cast(caster, CastTarget::None);
    let refused = game.failures().iter().map(|failure| &failure.error);
    assert!(
        refused
            .clone()
            .all(|error| matches!(error, CallError::Api(ApiError::NegativeHeal)))
            && refused.count() == 1
    );
    assert_eq!(game.pool(caster), 100);
}

/// A modifier of no duration that runs `script`, with an interval of `interval_ms` and the
/// params `params`.
fn scripted(interval_ms: Option<i64>, params: &[(&str, i64)]) -> ModifierData {
    ModifierData {
        script: Some(PackagePath::parse("scripts/hooks.rhai").unwrap()),
        duration_ms: None,
        interval_ms: interval_ms.map(int),
        stacks_expire_ms: None,
        reapply: Reapply::Refresh,
        max_stacks: None,
        stats: BTreeMap::new(),
        states: Vec::new(),
        shield: None,
        aura: None,
        params: params
            .iter()
            .map(|&(name, value)| {
                (
                    name.to_owned(),
                    Param::Ranked(Ranked::One(Scalar::Int(value))),
                )
            })
            .collect(),
        state: BTreeMap::new(),
    }
}

impl Match {
    /// A match of stats, combat and abilities whose modifiers, by name in package 0, run
    /// `source`, as `scripted` declares them.
    fn with_modifiers(source: &str, modifiers: &[(&str, ModifierData)]) -> Match {
        let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
        let mut game = Match::with(LIMITS, &declared);
        let stats = StatBook::new(&BTreeMap::new(), [], RATE, num(6)).unwrap();
        Stats::load(&mut game.world, stats);
        let script = Units::compile(&mut game.world, source).unwrap();
        let mut sorted = modifiers.to_vec();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        for (name, data) in &sorted {
            Stats::load_modifier(&mut game.world, 0, name, data, Some(script));
        }
        game
    }

    /// Gives `unit` the modifier `name` from itself.
    fn give(&mut self, unit: StableId, name: &str) {
        let id = Stats::modifier(&self.world, 0, name).unwrap();
        let entity = self.world.resource::<EntityIndex>().get(unit).unwrap();
        if !self.world.entity(entity).contains::<Modifiers>() {
            self.world.entity_mut(entity).insert(Modifiers::default());
        }
        let applier = Applier {
            source: Some(unit),
            ability: None,
            rank: 1,
            passive: false,
            aura: false,
        };
        let add = ModifierEffect::Add {
            target: unit,
            id,
            duration: None,
        };
        Stats::apply_effect(&mut self.world, add, applier, |_| None);
    }

    /// Each failed call of the tick: the unit it ran for and its hook.
    fn calls(&self) -> Vec<(StableId, Hook)> {
        let failures = self.failures().iter();
        failures
            .map(|failure| (failure.unit.unwrap(), failure.hook))
            .collect()
    }
}

#[test]
fn combat_events_reach_each_modifier_once_in_their_order() {
    // Every hook fails, so the failures list the calls in the order they ran.
    let log = r#"
fn on_attack(ctx, m, target) { throw "attack"; }
fn on_attack_hit(ctx, m, d) { throw "hit"; }
fn on_damage_taken(ctx, m, d) { throw "taken"; }
fn on_kill(ctx, m, victim) { throw "kill"; }
fn on_takedown(ctx, m, victim) { throw "takedown"; }
fn on_interval(ctx, m) { throw "interval"; }
"#;
    let mut game = Match::with_modifiers(
        log,
        &[
            ("log", scripted(None, &[])),
            ("pulse", scripted(Some(100), &[])),
        ],
    );
    game.world.insert_resource(AssistWindow(Ticks::new(10)));
    let attacker = game.spawn(0, at(Num::ZERO, Num::ZERO, Num::ZERO), ());
    let assister = game.spawn(0, at(num(3), Num::ZERO, Num::ZERO), ());
    let victim = game.spawn(1, at(Num::ONE, Num::ZERO, Num::ZERO), ());
    let bystander = game.spawn(1, at(num(9), Num::ZERO, Num::ZERO), ());
    for unit in [attacker, assister, victim] {
        game.give(unit, "log");
    }
    game.give(bystander, "pulse");
    // The attacker strikes for 500 as its windup of no ticks ends, in tick 0; the assister
    // struck the victim in tick 0 too.
    let entity = |game: &Match, id| game.world.resource::<EntityIndex>().get(id).unwrap();
    let striker = entity(&game, attacker);
    let stats = AttackStats::new(num(2), Ticks::new(0), Ticks::new(5), num(500)).unwrap();
    game.world.entity_mut(striker).insert(stats);
    let mut attack = game.world.get_mut::<AttackState>(striker).unwrap();
    attack.set_target(Some(victim));
    let target = entity(&game, victim);
    game.world
        .resource_scope(|world, index: Mut<'_, EntityIndex>| {
            let mut attackers = world.get_mut::<RecentAttackers>(target).unwrap();
            attackers.record(assister, Tick::new(0), &index);
        });
    game.world.run_schedule(SimUpdate);
    assert_eq!(game.health(victim), 0);
    assert_eq!(
        game.calls(),
        [
            (attacker, Hook::OnAttack),
            (attacker, Hook::OnAttackHit),
            (victim, Hook::OnDamageTaken),
            (attacker, Hook::OnKill),
            (attacker, Hook::OnTakedown),
            (assister, Hook::OnTakedown),
        ]
    );
    // The pulse of 100 ms, 3 ticks at 30 a second, applied in tick 0: in ticks 3 and 6 alone.
    for tick in 1..=6 {
        game.world.run_schedule(SimUpdate);
        let expected = if tick % 3 == 0 {
            vec![(bystander, Hook::OnInterval)]
        } else {
            Vec::new()
        };
        assert_eq!(game.calls(), expected, "tick {tick}");
    }
}

#[test]
fn a_hook_deals_damage_into_the_pass_and_a_chain_16_deep_fails() {
    // An extra attack on each attack that is not one, and a unit that hurts itself again, by its
    // modifier's `echo`, each time it takes damage.
    let hooks = r#"
fn on_attack_hit(ctx, m, d) {
    if !d.extra {
        ctx.attack_hit(d.target);
    }
}
fn on_damage_taken(ctx, m, d) {
    ctx.damage(m.carrier, ctx.p.echo, "true");
}
"#;
    let mut game = Match::with_modifiers(
        hooks,
        &[
            ("double", scripted(None, &[])),
            ("echo", scripted(None, &[("echo", 1)])),
        ],
    );
    let attacker = game.spawn(0, at(Num::ZERO, Num::ZERO, Num::ZERO), ());
    let victim = game.spawn(1, at(Num::ONE, Num::ZERO, Num::ZERO), ());
    let echoer = game.spawn(1, at(num(9), Num::ZERO, Num::ZERO), ());
    game.give(attacker, "double");
    game.give(echoer, "echo");
    let striker = game.world.resource::<EntityIndex>().get(attacker).unwrap();
    let stats = AttackStats::new(num(2), Ticks::new(0), Ticks::new(5), num(30)).unwrap();
    game.world.entity_mut(striker).insert(stats);
    let mut attack = game.world.get_mut::<AttackState>(striker).unwrap();
    attack.set_target(Some(victim));
    game.world.resource_mut::<DamageQueue>().push(Damage {
        source: None,
        target: echoer,
        amount: num(10),
        kind: DamageKind::new(2),
        cause: DamageCause::Effect,
        ability: None,
        depth: 0,
    });
    game.world.run_schedule(SimUpdate);
    // The attack's 30, then the extra attack's 30, which adds none: 500 − 60. The echoer's 10,
    // then an echo of 1 from each hook at depths 1 to 15; the one at 16 fails: 500 − 10 − 15.
    assert_eq!(game.health(victim), 440);
    assert_eq!(game.health(echoer), 475);
    let failures: Vec<_> = game
        .failures()
        .iter()
        .map(|failure| (failure.unit, failure.hook, failure.error.clone()))
        .collect();
    assert!(matches!(
        failures.as_slice(),
        [(Some(unit), Hook::OnDamageTaken, CallError::Api(ApiError::ChainTooDeep))] if *unit == echoer
    ));
}
