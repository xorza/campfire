use std::sync::Arc;

use bevy_ecs::bundle::Bundle;
use campfire_common::{PlayerSlot, Tick};
use campfire_math::{Num, Vec3};
use campfire_script::rhai::Array;
use campfire_sim::{Capability, EntityIndex, Position, SimTick, StableId};

use super::*;
use crate::actions::action_slots::ActionSlots;
use crate::capability_set::test_match::TestMatch;
use crate::combat::internals::Armed;
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;
use crate::navigation::on_path::OnPath;
use crate::scripts::error::ApiError;
use crate::scripts::error::internals::FailureKind;
use crate::scripts::script_limits::ScriptLimits;
use crate::stats::level::Level;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::stats::stats_column::StatsColumn;
use crate::stats::unit_stats::UnitStats;
use crate::units::block::Block;
use crate::units::dead::Dead;
use crate::units::path_id::PathId;
use crate::units::type_scope::TypeScope;
use crate::units::unit::Unit;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::bounds::Bounds;
use crate::values::scalar::Scalar;
use crate::values::stat::Stat;
fn at(x: i64, y: i64, z: i64) -> Position {
    Position::new(Vec3::new(Num::int(x), Num::int(y), Num::int(z))).unwrap()
}

/// A unit of 10 health, the life pool the first of the scene's pools, `health` and `mana`, with
/// a weapon of 2 m.
fn unit() -> Armed {
    Armed::melee(Num::int(10), Num::int(2), 0, 1, Num::ZERO).on_death(OnDeath::Stay)
}

#[derive(Debug)]
struct Scene {
    sim: TestMatch,
}

impl Scene {
    fn new() -> Scene {
        let limits = ScriptLimits::ROOMY;
        let scripts = ScriptBudgets::new(limits, 1);
        let sim = TestMatch::server(&[Capability::Stats, Capability::Combat], scripts);
        Units::name_kinds(&sim.world, &[], &["health", "mana"], &[]);
        Scene { sim }
    }

    fn unit_type(&mut self, tags: &[&str], params: &[(&str, Scalar)]) -> UnitType {
        let data = UnitTypeData::of(tags, params);
        Units::load_next_type(&mut self.sim.world, &data)
    }

    /// A `unit()` of `team` with `parts`.
    fn unit(&mut self, at: Position, team: u8, parts: impl Bundle) -> StableId {
        let armed = unit().bundle(&mut self.sim.world, Team::new(team));
        self.sim.spawn(at, (armed, parts))
    }
}

#[test]
fn queries_select_living_units_by_filter_and_exact_distance() {
    let mut scene = Scene::new();
    let creep = scene.unit_type(&["creep"], &[]);
    let of = scene.unit(at(0, 0, 0), 1, ());
    // Up at y = 9, 3 m away on the ground plane: the nearest.
    let high = scene.unit(at(0, 9, 3), 0, ());
    let east = scene.unit(at(4, 0, 0), 0, creep);
    let west = scene.unit(at(-4, 0, 0), 0, ());
    let edge = scene.unit(at(0, 0, 5), 0, ());
    let far = scene.unit(at(6, 0, 0), 0, ());
    let ally = scene.unit(at(1, 0, 0), 1, ());
    let dead = scene.unit(at(0, 0, 1), 0, Dead);
    // Within 2 m, but no target: one untargetable, one invulnerable.
    let hidden = UnitTags::blocking(&[Block::Target]);
    let hidden = scene.unit(at(2, 0, 0), 0, hidden);
    let guarded = UnitTags::blocking(&[Block::Target, Block::Damage]);
    let guarded = scene.unit(at(-2, 0, 0), 0, guarded);

    let find = |scene: &mut Scene, filter: &str| {
        let source = format!(r#"fn probe(ctx, of) {{ ctx.find(of, of.pos, 5, "{filter}") }}"#);
        Unit::ids(scene.sim.probe(&source, of).unwrap())
    };
    // By stable id; the edge at exactly 5 m is in; the far one, the dead one and the two that are
    // no target are not.
    assert_eq!(find(&mut scene, "enemies"), [high, east, west, edge]);
    assert_eq!(find(&mut scene, "enemies:creep"), [east]);
    assert_eq!(find(&mut scene, "enemies:!creep"), [high, west, edge]);
    assert_eq!(find(&mut scene, "enemies:creep:!creep"), []);
    assert_eq!(find(&mut scene, "allies"), [of, ally]);
    assert_eq!(find(&mut scene, "all"), [of, high, east, west, edge, ally]);
    // At 6 m the far one is in, so the radius kept it out; the dead one and the two that are no
    // target stay out at any radius.
    let within_six = r#"fn probe(ctx, of) { ctx.find(of, of.pos, 6, "enemies") }"#;
    let found = Unit::ids(scene.sim.probe(within_six, of).unwrap());
    assert_eq!(found, [high, east, west, edge, far]);
    assert!(
        ![dead, hidden, guarded]
            .iter()
            .any(|unit| found.contains(unit))
    );
    // In space, the high one is √(81 + 9) ≈ 9.49 m away: out of reach, and no longer the nearest.
    scene.sim.world.insert_resource(Metric::Spatial);
    assert_eq!(find(&mut scene, "enemies"), [east, west, edge]);
    let nearest = r#"fn probe(ctx, of) { ctx.nearest_visible(of, num(10), "enemies") }"#;
    assert_eq!(Unit::ids(scene.sim.probe(nearest, of).unwrap()), [east]);
    scene.sim.world.insert_resource(Metric::Planar);

    // A query reaches a body to its edge, as an area does: far, at 6 m, with a body of 1 m comes
    // within 5 m. `nearest_visible` reaches from the edge of `of`'s body too, as a weapon's
    // range: 2 m reaches high, 3 m away, only once `of` has a body of 1 m.
    let body = Body::new(Num::int(1)).unwrap();
    scene.sim.insert(far, body);
    assert_eq!(find(&mut scene, "enemies"), [high, east, west, edge, far]);
    let near_two = r#"fn probe(ctx, of) { ctx.nearest_visible(of, num(2), "enemies") }"#;
    assert_eq!(Unit::ids(scene.sim.probe(near_two, of).unwrap()), []);
    scene.sim.insert(of, body);
    assert_eq!(Unit::ids(scene.sim.probe(near_two, of).unwrap()), [high]);

    // The nearest in turn as each despawns, by distance between centres: east and west tie at
    // 4 m, and east has the lower id; far, its body's edge at 5 m as edge's centre is, is 6 m
    // away.
    let nearest = r#"fn probe(ctx, of) { ctx.nearest_visible(of, num(5), "enemies") }"#;
    let mut order = Vec::new();
    while let [next] = Unit::ids(scene.sim.probe(nearest, of).unwrap())[..] {
        order.push(next);
        let entity = scene.sim.entity(next);
        scene.sim.world.despawn(entity);
    }
    assert_eq!(order, [high, east, west, edge, far]);

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
        let error = scene.sim.read(call, of).unwrap_err();
        assert_eq!(error.kind(), FailureKind::Api(refusal), "{call}: {error:?}");
    }
}

#[test]
fn a_position_measures_reach_and_distance_in_the_maps_metric() {
    let mut scene = Scene::new();
    let of = scene.unit(at(0, 0, 0), 0, ());
    scene.unit(at(3, 0, 4), 1, ());
    scene.unit(at(0, 12, 3), 1, ());
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
        (Metric::Planar, [true, false, true, true, true], Num::int(3)),
        (
            Metric::Spatial,
            [true, false, false, false, true],
            Num::from_bits(207_522_701),
        ),
    ];
    for (metric, within, distance) in metrics {
        scene.sim.world.insert_resource(metric);
        let read = scene.sim.probe(probe, of).unwrap().cast::<Array>();
        let read_within: Vec<_> = read[..5]
            .iter()
            .map(|reach| reach.as_bool().unwrap())
            .collect();
        assert_eq!(read_within, within, "{metric:?}");
        let read_distance: Vec<_> = read[5..]
            .iter()
            .map(|at| at.clone().cast::<Num>())
            .collect();
        assert_eq!(read_distance, [Num::int(5), distance], "{metric:?}");
    }
    let error = scene
        .sim
        .probe("fn probe(ctx, of) { of.pos.within(of.pos, -1) }", of)
        .unwrap_err();
    assert_eq!(error.kind(), FailureKind::Api(ApiError::NegativeRadius));
}

#[test]
fn the_view_reads_the_maps_bounds_or_the_worlds() {
    let mut scene = Scene::new();
    let view = scene.sim.world.non_send::<View>().clone();
    // A match with no mode has the whole world's bounds; a mode's map gives its own.
    view.read(&mut scene.sim.world);
    assert_eq!(view.bounds(), Bounds::WORLD);
    let bounds = Bounds::new([Num::int(-10), Num::int(-5)], [Num::int(10), Num::int(6)]).unwrap();
    scene.sim.world.insert_resource(bounds);
    view.read(&mut scene.sim.world);
    assert_eq!(view.bounds(), bounds);
}

#[test]
fn a_handle_reads_its_units_fields_as_the_view_read_them() {
    let mut scene = Scene::new();
    let window = ("help_window_ms", Scalar::Int(2000));
    let range = ("aggro_range", Scalar::Decimal(Num::int(7)));
    let hero = scene.unit_type(&["avatar"], &[window, range]);
    // On a path, but the scene has no `navigation` to fill `unit.path`.
    let owner = Owner::new(PlayerSlot::new(2));
    let path = OnPath::new(PathId::new(0));
    let body = Body::new(Num::from_bits(3 << (Num::FRAC_BITS - 2))).unwrap();
    let spawn = SpawnPoint::new(at(5, 0, -5));
    let of = scene.unit(at(0, 0, 0), 0, (hero, owner, path, body, spawn));
    let near = scene.unit(at(3, 0, 4), 1, ());
    let recent = scene.unit(at(9, 0, 0), 1, ());
    let fallen = scene.unit(at(9, 0, 1), 1, Dead);
    let health_only = (Team::new(1), Pools::life(Num::int(1)));
    let bare = scene.sim.spawn(at(9, 0, 2), health_only);
    let entity = scene.sim.entity(of);
    scene
        .sim
        .world
        .get_mut::<ActionSlots>(entity)
        .unwrap()
        .set_attack_target(Some(near));

    // At 30 ticks a second, 2000 ms is 60 ticks: in tick 100, a strike in tick 40 is recent, and
    // one in tick 39 is not; a dead attacker is never returned.
    scene
        .sim
        .world
        .insert_resource(SimTick::new(Tick::new(100)));
    let index = scene.sim.world.resource::<EntityIndex>();
    let mut attackers = RecentAttackers::default();
    // A strike later than the view's tick, as a rollback can leave, is not recent either.
    for (source, tick) in [(near, 39), (recent, 40), (fallen, 100), (bare, 101)] {
        attackers.record(source, Tick::new(tick), index);
    }
    *scene.sim.world.get_mut::<RecentAttackers>(entity).unwrap() = attackers;

    let read = |scene: &mut Scene, expression: &str| scene.sim.read(expression, of);
    let value = |scene: &mut Scene, expression: &str| read(scene, expression).unwrap();
    assert!(value(&mut scene, "of.is_avatar").as_bool().unwrap());
    assert!(value(&mut scene, "of.alive").as_bool().unwrap());
    assert_eq!(value(&mut scene, "of.owner").as_int(), Ok(2));
    assert!(value(&mut scene, "of.path").is_unit());
    // With no `vision`, every team sees every unit.
    assert!(value(&mut scene, "of.can_see(of)").as_bool().unwrap());
    // Its body's radius, 0.75 m; a unit with no body has none.
    let radius = |scene: &mut Scene, unit| {
        let source = "fn probe(ctx, of) { of.radius }";
        scene.sim.probe(source, unit).unwrap().cast::<Num>()
    };
    assert_eq!(radius(&mut scene, of), body.radius());
    assert_eq!(radius(&mut scene, near), Num::ZERO);
    // Where it spawned, which a unit the mode did not spawn has none of.
    let spawn_pos = |scene: &mut Scene, unit| {
        let source = "fn probe(ctx, of) { of.spawn_pos }";
        scene.sim.probe(source, unit).unwrap()
    };
    assert_eq!(spawn_pos(&mut scene, of).cast::<Position>(), spawn.get());
    assert!(spawn_pos(&mut scene, near).is_unit());
    assert_eq!(
        value(&mut scene, "of.params.help_window_ms").as_int(),
        Ok(2000)
    );
    assert_eq!(
        value(&mut scene, "of.params.aggro_range").cast::<Num>(),
        Num::int(7)
    );
    assert_eq!(
        value(&mut scene, "of.attack_range").cast::<Num>(),
        Num::int(2)
    );
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
    assert_eq!(Unit::ids(attackers), [recent]);
    // 2034 ms is 61.02 ticks, up to 62: tick 39 is in.
    let attackers = value(&mut scene, "of.recent_attackers(2034)");
    assert_eq!(Unit::ids(attackers), [near, recent]);

    let refusals = [
        ("of.params.gold", ApiError::UnknownParam),
        ("of.recent_attackers(-1)", ApiError::NegativeTime),
    ];
    for (expression, refusal) in refusals {
        let error = read(&mut scene, expression).unwrap_err();
        assert_eq!(error.kind(), FailureKind::Api(refusal), "{expression}");
    }
    let error = scene
        .sim
        .probe("fn probe(ctx, of) { of.attack_range }", bare)
        .unwrap_err();
    assert_eq!(
        error.kind(),
        FailureKind::Api(ApiError::NoAttack),
        "{error:?}"
    );

    // A target the view did not read is `()`.
    let entity = scene.sim.entity(near);
    scene.sim.world.despawn(entity);
    assert!(value(&mut scene, "of.target == ()").as_bool().unwrap());
}

#[test]
fn a_handle_reads_its_units_level_pools_and_stats() {
    let mut scene = Scene::new();
    // Level 3, with armor 25 and health 10 among the mode's stats; its health pool of 10, 4
    // taken, and no mana pool. One unit has a pool and no stats, one neither.
    let names = ["armor", "health"].map(|name| Stat::named(name).unwrap());
    StatsColumn::share_stat_names(scene.sim.world.non_send::<View>(), Arc::from(names));
    let mut stats = UnitStats::default();
    stats.refill().extend([Num::int(25), Num::int(10)]);
    let of = scene.unit(at(0, 0, 0), 0, Level::new(3).unwrap());
    let entity = scene.sim.entity(of);
    scene.sim.insert(of, stats);
    let mut pools = scene.sim.world.get_mut::<Pools>(entity).unwrap();
    pools.take(PoolId::FIRST, Num::int(4));
    let bare = scene
        .sim
        .spawn(at(1, 0, 0), (Team::new(1), Pools::life(Num::int(1))));
    let shell = scene.sim.spawn(at(2, 0, 0), Team::new(1));

    let read = |scene: &mut Scene, unit, expression: &str| scene.sim.read(expression, unit);
    let value = |scene: &mut Scene, expression: &str| read(scene, of, expression).unwrap();
    assert_eq!(value(&mut scene, "of.level").as_int(), Ok(3));
    assert_eq!(
        value(&mut scene, r#"of.pool("health")"#).cast::<Num>(),
        Num::int(6)
    );
    assert_eq!(
        value(&mut scene, r#"of.pool_max("health")"#).cast::<Num>(),
        Num::int(10)
    );
    assert_eq!(
        value(&mut scene, r#"of.stat("armor")"#).cast::<Num>(),
        Num::int(25)
    );
    assert_eq!(
        value(&mut scene, r#"of.stat("health")"#).cast::<Num>(),
        Num::int(10)
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
        assert_eq!(
            error.kind(),
            FailureKind::Api(refusal),
            "{expression}: {error:?}"
        );
    }
}

#[test]
fn a_unit_type_name_is_one_types_only_in_its_scope() {
    let mut scene = Scene::new();
    let data = UnitTypeData::default();
    let mut load = |scope, name| Units::load_type(&mut scene.sim.world, scope, name, &data);
    let first = load(TypeScope::Mode, "grunt");
    let second = load(TypeScope::Mode, "tower");
    // A package's own scope holds a `grunt` of its own, which a mode name does not reach.
    let theirs = load(TypeScope::Package(1), "grunt");
    let view = scene.sim.world.non_send::<View>();
    assert_eq!(
        (view.unit_type_named("grunt"), view.unit_type_named("tower")),
        (Some(first), Some(second))
    );
    assert_eq!(view.unit_type_named("wolf"), None);
    let types = view.types_mut();
    assert_eq!(types.named(TypeScope::Package(1), "grunt"), Some(theirs));
    assert_eq!(types.named(TypeScope::Package(2), "grunt"), None);
    assert_ne!(theirs, first);
}

#[test]
#[should_panic(expected = "a scope names a type once")]
fn a_type_named_twice_in_one_scope_is_a_logic_error() {
    let mut scene = Scene::new();
    let data = UnitTypeData::default();
    Units::load_type(&mut scene.sim.world, TypeScope::Package(1), "grunt", &data);
    Units::load_type(&mut scene.sim.world, TypeScope::Package(1), "grunt", &data);
}
