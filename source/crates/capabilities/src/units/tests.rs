use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::rc::Rc;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use campfire_math::{Num, PlayerSlot, Vec3};
use campfire_script::Budget;
use campfire_script::rhai::Array;
use campfire_sim::{
    Capability, EntityIndex, IdAllocator, Position, SimTick, StableId, Tick, Ticks,
};

use super::*;
use crate::capability_set::internals::TestMatch;
use crate::combat::attack_state::AttackState;
use crate::combat::attack_stats::AttackStats;
use crate::combat::combatant::Combatant;
use crate::combat::combatant::internals::Armed;
use crate::combat::dead::Dead;
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;
use crate::navigation::on_path::OnPath;
use crate::scripts::error::{ApiError, CallError};
use crate::scripts::script_limits::ScriptLimits;
use crate::stats::level::Level;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::stats::stat::Stat;
use crate::stats::unit_stats::UnitStats;
use crate::units::block::Block;
use crate::units::path_id::PathId;
use crate::units::unit::Unit;
use crate::units::unit_tags::UnitTags;
use crate::values::declared_name::DeclaredName;
use crate::values::scalar::Scalar;

/// The MOBA's 30 ticks a second.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn at(x: i64, y: i64, z: i64) -> Position {
    Position::new(Vec3::new(num(x), num(y), num(z))).unwrap()
}

/// A unit of 10 health, the life pool the first of the scene's pools, `health` and `mana`.
fn unit() -> Armed {
    let combatant = Combatant {
        attack: Some(AttackStats::new(num(2), Ticks::new(0), Ticks::new(1), Num::ZERO).unwrap()),
        on_death: OnDeath::Stay,
    };
    Armed {
        combatant,
        life: num(10),
    }
}

#[derive(Debug)]
struct Scene {
    world: World,
}

impl Scene {
    fn new() -> Scene {
        let limits = ScriptLimits {
            per_call: 10_000,
            player: 100_000,
            think: 100_000,
            mode: 100_000,
        };
        let scripts = MatchScripts {
            limits,
            players: 1,
            damage_kinds: Rc::from([]),
            stats: Rc::from([]),
            pools: ["health", "mana"]
                .map(|pool| DeclaredName::new(pool).unwrap())
                .into(),
        };
        let TestMatch {
            world,
            schedule: _,
            registry: _,
        } = TestMatch::new(
            &[Capability::Stats, Capability::Combat],
            RATE,
            Some(scripts),
        );
        Scene { world }
    }

    fn unit_type(&mut self, tags: &[&str], params: &[(&str, Scalar)]) -> UnitType {
        let data = UnitTypeData {
            tags: tags.iter().map(|&tag| tag.to_owned()).collect(),
            params: params
                .iter()
                .map(|&(name, value)| (name.to_owned(), value))
                .collect::<BTreeMap<_, _>>(),
        };
        let name = format!("type {}", self.world.non_send::<View>().types_count());
        Units::load_type(&mut self.world, &name, &data).unwrap()
    }

    fn spawn(&mut self, at: Position, parts: impl Bundle) -> StableId {
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        let unit = self.world.spawn((id, at, parts)).id();
        UnitTags::give_type_tags(&mut self.world, unit);
        id
    }

    fn entity(&self, id: StableId) -> Entity {
        self.world.resource::<EntityIndex>().get(id).unwrap()
    }

    /// `probe(ctx, of)` in `source`, run on the units as they are now.
    fn probe(&mut self, source: &str, of: StableId) -> Result<Dynamic, CallError> {
        let ctx = self.world.non_send::<Ctx>().clone();
        ctx.view().read(&self.world);
        let unit = ctx.view().unit(of).unwrap();
        let mut host = self.world.non_send_mut::<ScriptHost>();
        let script = host.compile(source).unwrap();
        let mut budget = Budget::new(u64::MAX);
        host.call(&mut budget, script, "probe", (ctx, unit))
            .map_err(CallError::from_script)
    }

    /// The stable ids of `value`, a unit or a list of units.
    fn ids(value: Dynamic) -> Vec<StableId> {
        let units = match value.clone().try_cast::<Vec<Dynamic>>() {
            Some(units) => units,
            None if value.is_unit() => Vec::new(),
            None => vec![value],
        };
        units
            .into_iter()
            .map(|unit| unit.try_cast::<Unit>().unwrap().id)
            .collect()
    }
}

#[test]
fn queries_select_living_units_by_filter_and_exact_distance() {
    let mut scene = Scene::new();
    let creep = scene.unit_type(&["creep"], &[]);
    let of = scene.spawn(at(0, 0, 0), unit().bundle(Team::new(1)));
    // Up at y = 9, 3 m away on the ground plane: the nearest.
    let high = scene.spawn(at(0, 9, 3), unit().bundle(Team::new(0)));
    let east = scene.spawn(at(4, 0, 0), (creep, unit().bundle(Team::new(0))));
    let west = scene.spawn(at(-4, 0, 0), unit().bundle(Team::new(0)));
    let edge = scene.spawn(at(0, 0, 5), unit().bundle(Team::new(0)));
    let far = scene.spawn(at(6, 0, 0), unit().bundle(Team::new(0)));
    let ally = scene.spawn(at(1, 0, 0), unit().bundle(Team::new(1)));
    let dead = scene.spawn(at(0, 0, 1), (unit().bundle(Team::new(0)), Dead));
    // Within 2 m, but no target: one untargetable, one invulnerable.
    let hidden = UnitTags::blocking(&[Block::Target]);
    let hidden = scene.spawn(at(2, 0, 0), (unit().bundle(Team::new(0)), hidden));
    let guarded = UnitTags::blocking(&[Block::Target, Block::Damage]);
    let guarded = scene.spawn(at(-2, 0, 0), (unit().bundle(Team::new(0)), guarded));

    let find = |scene: &mut Scene, filter: &str| {
        let source = format!(r#"fn probe(ctx, of) {{ ctx.find(of, of.pos, 5, "{filter}") }}"#);
        Scene::ids(scene.probe(&source, of).unwrap())
    };
    // By stable id; the edge at exactly 5 m is in; the far one, the dead one and the two that are
    // no target are not.
    assert_eq!(find(&mut scene, "enemies"), [high, east, west, edge]);
    assert_eq!(find(&mut scene, "enemies:creep"), [east]);
    assert_eq!(find(&mut scene, "enemies:!creep"), [high, west, edge]);
    assert_eq!(find(&mut scene, "enemies:creep:!creep"), []);
    assert_eq!(find(&mut scene, "allies"), [of, ally]);
    assert_eq!(find(&mut scene, "all"), [of, high, east, west, edge, ally]);
    assert!(far.get() > 0 && dead.get() > 0 && hidden.get() > 0 && guarded.get() > 0);
    // In space, the high one is √(81 + 9) ≈ 9.49 m away: out of reach, and no longer the nearest.
    scene.world.insert_resource(Metric::Spatial);
    assert_eq!(find(&mut scene, "enemies"), [east, west, edge]);
    let nearest = r#"fn probe(ctx, of) { ctx.nearest_visible(of, num(10), "enemies") }"#;
    assert_eq!(Scene::ids(scene.probe(nearest, of).unwrap()), [east]);
    scene.world.insert_resource(Metric::Planar);

    // The nearest in turn as each despawns: east and west tie at 4 m, and east has the lower id.
    let nearest = r#"fn probe(ctx, of) { ctx.nearest_visible(of, num(5), "enemies") }"#;
    let mut order = Vec::new();
    while let [next] = Scene::ids(scene.probe(nearest, of).unwrap())[..] {
        order.push(next);
        let entity = scene.entity(next);
        scene.world.despawn(entity);
    }
    assert_eq!(order, [high, east, west, edge]);

    let refusals = [
        (
            r#"ctx.find(of, of.pos, 5, "friends")"#,
            ApiError::UnknownFilter,
        ),
        (
            r#"ctx.find(of, of.pos, 5, "enemies:boss")"#,
            ApiError::UnknownTag,
        ),
        (
            r#"ctx.find(of, of.pos, -1, "all")"#,
            ApiError::NegativeRadius,
        ),
        (
            r#"ctx.nearest_visible(of, 1 << 40, "all")"#,
            ApiError::IntegerBeyondNum,
        ),
    ];
    for (call, refusal) in refusals {
        let source = format!("fn probe(ctx, of) {{ {call} }}");
        let error = scene.probe(&source, of).unwrap_err();
        assert!(
            matches!(error, CallError::Api(api) if api == refusal),
            "{call}: {error:?}"
        );
    }
}

#[test]
fn a_position_measures_reach_and_distance_in_the_maps_metric() {
    let mut scene = Scene::new();
    let of = scene.spawn(at(0, 0, 0), unit().bundle(Team::new(0)));
    scene.spawn(at(3, 0, 4), unit().bundle(Team::new(1)));
    scene.spawn(at(0, 12, 3), unit().bundle(Team::new(1)));
    let probe = r#"fn probe(ctx, of) {
        let found = ctx.find(of, of.pos, 13, "enemies");
        let near = found[0];
        let up = found[1];
        [of.pos.within(near.pos, 5), of.pos.within(near.pos, 4), of.pos.within(up.pos, 3),
            of.pos.within(up.pos, 12), of.pos.within(up.pos, 13),
            of.pos.distance_to(near.pos), of.pos.distance_to(up.pos)]
    }"#;
    // Exactly, as every range. The near one is 5 m away in either metric: 3, 4, 5. The one up at
    // y = 12 is 3 m away on a planar map, and √153 ≈ 12.37 m on a spatial one, 207522701.02 in 24
    // fraction bits.
    let metrics = [
        (Metric::Planar, [true, false, true, true, true], num(3)),
        (
            Metric::Spatial,
            [true, false, false, false, true],
            Num::from_bits(207_522_701),
        ),
    ];
    for (metric, within, distance) in metrics {
        scene.world.insert_resource(metric);
        let read = scene.probe(probe, of).unwrap().cast::<Array>();
        let read_within = read[..5].iter().map(|reach| reach.as_bool().unwrap());
        assert!(read_within.eq(within), "{metric:?}");
        let read_distance = read[5..].iter().map(|at| at.clone().cast::<Num>());
        assert!(read_distance.eq([num(5), distance]), "{metric:?}");
    }
    let error = scene
        .probe("fn probe(ctx, of) { of.pos.within(of.pos, -1) }", of)
        .unwrap_err();
    assert!(matches!(error, CallError::Api(ApiError::NegativeRadius)));
}

#[test]
fn a_handle_reads_its_units_fields_as_the_view_read_them() {
    let mut scene = Scene::new();
    let window = ("help_window_ms", Scalar::Int(2000));
    let range = ("aggro_range", Scalar::Decimal(num(7)));
    let hero = scene.unit_type(&["avatar"], &[window, range]);
    // On a path, but the scene has no `navigation` to fill `unit.path`.
    let owner = Owner::new(PlayerSlot::new(2));
    let path = OnPath::new(PathId::new(0));
    let body = Body::new(Num::from_bits(3 << (Num::FRAC_BITS - 2))).unwrap();
    let of = scene.spawn(
        at(0, 0, 0),
        (hero, unit().bundle(Team::new(0)), owner, path, body),
    );
    let near = scene.spawn(at(3, 0, 4), unit().bundle(Team::new(1)));
    let recent = scene.spawn(at(9, 0, 0), unit().bundle(Team::new(1)));
    let fallen = scene.spawn(at(9, 0, 1), (unit().bundle(Team::new(1)), Dead));
    let health_only = (Team::new(1), Pools::life(num(1)));
    let bare = scene.spawn(at(9, 0, 2), health_only);
    let entity = scene.entity(of);
    scene
        .world
        .get_mut::<AttackState>(entity)
        .unwrap()
        .set_target(Some(near));

    // At 30 ticks a second, 2000 ms is 60 ticks: in tick 100, a strike in tick 40 is recent, and
    // one in tick 39 is not; a dead attacker is never returned.
    scene.world.insert_resource(SimTick::new(Tick::new(100)));
    let index = scene.world.resource::<EntityIndex>();
    let mut attackers = RecentAttackers::default();
    // A strike later than the view's tick, as a rollback can leave, is not recent either.
    for (source, tick) in [(near, 39), (recent, 40), (fallen, 100), (bare, 101)] {
        attackers.record(source, Tick::new(tick), index);
    }
    *scene.world.get_mut::<RecentAttackers>(entity).unwrap() = attackers;

    let read = |scene: &mut Scene, expression: &str| {
        let source = format!("fn probe(ctx, of) {{ {expression} }}");
        scene.probe(&source, of)
    };
    let value = |scene: &mut Scene, expression: &str| read(scene, expression).unwrap();
    assert!(value(&mut scene, "of.is_avatar").as_bool().unwrap());
    assert!(value(&mut scene, "of.alive").as_bool().unwrap());
    assert_eq!(value(&mut scene, "of.owner").as_int(), Ok(2));
    assert!(value(&mut scene, "of.path").is_unit());
    // Its body's radius, 0.75 m; a unit with no body has none.
    let radius = |scene: &mut Scene, unit| {
        let source = "fn probe(ctx, of) { of.radius }";
        scene.probe(source, unit).unwrap().cast::<Num>()
    };
    assert_eq!(radius(&mut scene, of), body.radius());
    assert_eq!(radius(&mut scene, near), Num::ZERO);
    assert_eq!(
        value(&mut scene, "of.params.help_window_ms").as_int(),
        Ok(2000)
    );
    assert_eq!(
        value(&mut scene, "of.params.aggro_range").cast::<Num>(),
        num(7)
    );
    assert_eq!(value(&mut scene, "of.attack_range").cast::<Num>(), num(2));
    assert!(
        value(&mut scene, "of.target.is_enemy_of(of)")
            .as_bool()
            .unwrap()
    );
    assert!(!value(&mut scene, "of.target.is_avatar").as_bool().unwrap());
    assert!(
        value(&mut scene, "of.target != () && of != ()")
            .as_bool()
            .unwrap()
    );
    let attackers = value(&mut scene, "of.recent_attackers(2000)");
    assert_eq!(Scene::ids(attackers), [recent]);
    // 2034 ms is 61.02 ticks, up to 62: tick 39 is in.
    let attackers = value(&mut scene, "of.recent_attackers(2034)");
    assert_eq!(Scene::ids(attackers), [near, recent]);

    let refusals = [
        ("of.params.gold", ApiError::UnknownParam),
        ("of.recent_attackers(-1)", ApiError::NegativeTime),
    ];
    for (expression, refusal) in refusals {
        let error = read(&mut scene, expression).unwrap_err();
        assert!(
            matches!(error, CallError::Api(api) if api == refusal),
            "{expression}"
        );
    }
    let error = scene
        .probe("fn probe(ctx, of) { of.attack_range }", bare)
        .unwrap_err();
    assert!(
        matches!(error, CallError::Api(ApiError::NoAttack)),
        "{error:?}"
    );

    // A target the view did not read is `()`.
    let entity = scene.entity(near);
    scene.world.despawn(entity);
    assert!(value(&mut scene, "of.target == ()").as_bool().unwrap());
}

#[test]
fn a_handle_reads_its_units_level_pools_and_stats() {
    let mut scene = Scene::new();
    // Level 3, with armor 25 and health 10 among the mode's stats; its health pool of 10, 4
    // taken, and no mana pool. One unit has a pool and no stats, one neither.
    let names = ["armor", "health"].map(|name| Stat::named(name).unwrap());
    scene
        .world
        .non_send::<View>()
        .set_stat_names(Rc::from(names));
    let mut stats = UnitStats::default();
    stats.refill().extend([num(25), num(10)]);
    let of = scene.spawn(
        at(0, 0, 0),
        (unit().bundle(Team::new(0)), Level::new(3).unwrap(), stats),
    );
    let entity = scene.entity(of);
    let mut pools = scene.world.get_mut::<Pools>(entity).unwrap();
    pools.take(PoolId::FIRST, num(4));
    let bare = scene.spawn(at(1, 0, 0), (Team::new(1), Pools::life(num(1))));
    let shell = scene.spawn(at(2, 0, 0), Team::new(1));

    let read = |scene: &mut Scene, unit, expression: &str| {
        let source = format!("fn probe(ctx, of) {{ {expression} }}");
        scene.probe(&source, unit)
    };
    let value = |scene: &mut Scene, expression: &str| read(scene, of, expression).unwrap();
    assert_eq!(value(&mut scene, "of.level").as_int(), Ok(3));
    assert_eq!(
        value(&mut scene, r#"of.pool("health")"#).cast::<Num>(),
        num(6)
    );
    assert_eq!(
        value(&mut scene, r#"of.pool_max("health")"#).cast::<Num>(),
        num(10)
    );
    assert_eq!(
        value(&mut scene, r#"of.stat("armor")"#).cast::<Num>(),
        num(25)
    );
    assert_eq!(
        value(&mut scene, r#"of.stat("health")"#).cast::<Num>(),
        num(10)
    );

    let refusals = [
        (of, r#"of.stat("spirit")"#, ApiError::UnknownStat),
        (bare, "of.level", ApiError::NoStats),
        (bare, r#"of.stat("armor")"#, ApiError::NoStats),
        (of, r#"of.pool("mana")"#, ApiError::NoPool),
        (of, r#"of.pool_max("rage")"#, ApiError::UnknownPool),
        (shell, r#"of.pool("health")"#, ApiError::NoPool),
        (shell, r#"of.pool_max("health")"#, ApiError::NoPool),
    ];
    for (unit, expression, refusal) in refusals {
        let error = read(&mut scene, unit, expression).unwrap_err();
        assert!(
            matches!(error, CallError::Api(api) if api == refusal),
            "{expression}: {error:?}"
        );
    }
}

#[test]
fn a_unit_type_name_is_one_types_only() {
    let mut scene = Scene::new();
    let data = UnitTypeData::default();
    let first = Units::load_type(&mut scene.world, "grunt", &data).unwrap();
    assert_eq!(
        Units::load_type(&mut scene.world, "grunt", &data),
        Err(UnitTypeError::RepeatedName)
    );
    let second = Units::load_type(&mut scene.world, "tower", &data).unwrap();
    let view = scene.world.non_send::<View>();
    assert_eq!(
        (view.unit_type("grunt"), view.unit_type("tower")),
        (Some(first), Some(second))
    );
    assert_eq!(view.unit_type("wolf"), None);
}
