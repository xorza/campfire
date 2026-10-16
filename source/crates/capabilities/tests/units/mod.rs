use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};

use bevy_ecs::bundle::Bundle;
use bevy_ecs::change_detection::{CHECK_TICK_THRESHOLD, DetectChangesMut};
use bevy_ecs::component::ComponentId;
use campfire_actions::ActionSlots;
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Release};
use campfire_combat::internals::Armed;
use campfire_combat::{Combat, RecentAttackers};
use campfire_common::{PlayerSlot, Tick};
use campfire_deliveries::Deliveries;
use campfire_geometry::{Bounds, Metric};
use campfire_math::{Num, Vec3};
use campfire_mode::Mode;
use campfire_navigation::OnPath;
use campfire_script::internals::FailureKind;
use campfire_script::{
    ApiError, CapabilityRefusal, NameKind, NumError, ScriptBudgets, ScriptLimits, ScriptList,
    ScriptValue, Unit, View,
};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{
    Capability, EntityIndex, IdAllocator, Position, Restoring, SimComponent, SimTick, SimWorld,
    SnapshotError, StableId,
};
use campfire_stats::{Level, Life, StatRule, Stats, StatsRefusal};
use campfire_units::*;
use campfire_values::{
    Block, PackageIndex, PathId, Scalar, Stat, Team, TypeScope, UnitTags, UnitType,
};

/// A match of stats and combat, with the stat book a weapon reads, and its types: `melee`, of
/// 10 life that stays when it dies, with a weapon of 2 m; `unit`, of no section; and `bare`, of
/// 1 life and no slot.
fn units_match() -> Match {
    let limits = ScriptLimits::ROOMY;
    let scripts = ScriptBudgets::new(limits, 1);
    let mut game = Match::server(&[Capability::Stats, Capability::Combat], scripts);
    Units::name_kinds(&game, &[], &["health", "mana"], &[]);
    Combat::load_weapon_stats(&mut game);
    let core = UnitTypeCore::default();
    let melee = Units::load_type(&mut game, TypeScope::Mode, "melee", &core);
    Armed::sparring().give(&mut game, melee);
    Units::load_type(&mut game, TypeScope::Mode, "unit", &core);
    let bare = Units::load_type(&mut game, TypeScope::Mode, "bare", &core);
    Units::give_type(&mut game, bare, Life::full(Num::int(1)));
    game
}

/// A `melee` unit of `team` at `pos` with `parts`.
fn melee(game: &mut Match, pos: impl Into<Vec3>, team: Team, parts: impl Bundle) -> StableId {
    Mode::spawn_named(game, "melee", pos, team, parts)
}

/// A unit of the type `name` of `team` at `pos`.
fn spawn_named(game: &mut Match, name: &str, pos: impl Into<Vec3>, team: Team) -> StableId {
    Mode::spawn_named(game, name, pos, team, ())
}

/// Gives `unit_type` the combat of a `melee` unit.
fn arm(game: &mut Match, unit_type: UnitType) {
    Armed::sparring().give(game, unit_type);
}

#[test]
fn queries_select_living_units_by_filter_and_exact_distance() {
    let mut game = units_match();
    let creep = Units::tagged_type(&mut game, &["creep"], &[]);
    arm(&mut game, creep);
    let of = melee(&mut game, (0, 0, 0), Team::new(1), ());
    // Up at y = 9, 3 m away on the ground plane: the nearest.
    let high = melee(&mut game, (0, 9, 3), Team::new(0), ());
    let east = Mode::spawn(&mut game, creep, (4, 0, 0), Team::new(0), ());
    let west = melee(&mut game, (-4, 0, 0), Team::new(0), ());
    let edge = melee(&mut game, (0, 0, 5), Team::new(0), ());
    let far = melee(&mut game, (6, 0, 0), Team::new(0), ());
    let ally = melee(&mut game, (1, 0, 0), Team::new(1), ());
    let dead = melee(&mut game, (0, 0, 1), Team::new(0), Dead);
    // Within 2 m, but no target: one untargetable, one invulnerable.
    let hidden = melee(&mut game, (2, 0, 0), Team::new(0), ());
    Units::set_blocks(&mut game, hidden, &[Block::Target]);
    let guarded = melee(&mut game, (-2, 0, 0), Team::new(0), ());
    Units::set_blocks(&mut game, guarded, &[Block::Target, Block::Damage]);

    let find = |game: &mut Match, filter: &str| {
        let source = format!(r#"fn probe(ctx, of) {{ ctx.find(of, of.pos, 5, "{filter}") }}"#);
        Unit::ids(Units::probe(game, &source, of).unwrap())
    };
    // By stable id; the edge at exactly 5 m is in; the far one, the dead one and the two that are
    // no target are not.
    assert_eq!(find(&mut game, "enemies"), [high, east, west, edge]);
    assert_eq!(find(&mut game, "enemies:creep"), [east]);
    assert_eq!(find(&mut game, "enemies:!creep"), [high, west, edge]);
    assert_eq!(find(&mut game, "enemies:creep:!creep"), []);
    assert_eq!(find(&mut game, "allies"), [of, ally]);
    assert_eq!(find(&mut game, "all"), [of, high, east, west, edge, ally]);
    // At 6 m the far one is in, so the radius kept it out; the dead one and the two that are no
    // target stay out at any radius.
    let within_six = r#"fn probe(ctx, of) { ctx.find(of, of.pos, 6, "enemies") }"#;
    let found = Unit::ids(Units::probe(&mut game, within_six, of).unwrap());
    assert_eq!(found, [high, east, west, edge, far]);
    assert!(
        ![dead, hidden, guarded]
            .iter()
            .any(|unit| found.contains(unit))
    );
    // In space, the high one is √(81 + 9) ≈ 9.49 m away: out of reach, and no longer the nearest.
    game.resource_mut::<MapBook>().metric = Metric::Spatial;
    assert_eq!(find(&mut game, "enemies"), [east, west, edge]);
    let nearest = r#"fn probe(ctx, of) { ctx.nearest_visible(of, num(10), "enemies") }"#;
    assert_eq!(
        Unit::ids(Units::probe(&mut game, nearest, of).unwrap()),
        [east]
    );
    game.resource_mut::<MapBook>().metric = Metric::Planar;

    // A query reaches a body to its edge, as an area does: far, at 6 m, with a body of 1 m comes
    // within 5 m. `nearest_visible` reaches from the edge of `of`'s body too, as a weapon's
    // range: 2 m reaches high, 3 m away, only once `of` has a body of 1 m.
    let body = Body::new(Num::int(1)).unwrap();
    game.insert_parts(far, body);
    assert_eq!(find(&mut game, "enemies"), [high, east, west, edge, far]);
    let near_two = r#"fn probe(ctx, of) { ctx.nearest_visible(of, num(2), "enemies") }"#;
    assert_eq!(
        Unit::ids(Units::probe(&mut game, near_two, of).unwrap()),
        []
    );
    game.insert_parts(of, body);
    assert_eq!(
        Unit::ids(Units::probe(&mut game, near_two, of).unwrap()),
        [high]
    );

    // The nearest in turn as each despawns, by distance between centers: east and west tie at
    // 4 m, and east has the lower id; far, its body's edge at 5 m as edge's center is, is 6 m
    // away.
    let nearest = r#"fn probe(ctx, of) { ctx.nearest_visible(of, num(5), "enemies") }"#;
    let mut order = Vec::new();
    while let [next] = Unit::ids(Units::probe(&mut game, nearest, of).unwrap())[..] {
        order.push(next);
        let entity = game.unit_entity(next);
        game.despawn(entity);
    }
    assert_eq!(order, [high, east, west, edge, far]);

    let refusals = [
        (
            r#"ctx.find(of, of.pos, 5, "friends")"#,
            ApiError::Unknown(NameKind::Filter),
        ),
        (
            r#"ctx.find(of, of.pos, 5, "enemies:boss")"#,
            ApiError::Unknown(NameKind::Tag),
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
        let error = Units::read(&mut game, call, of).unwrap_err();
        assert_eq!(error.kind(), FailureKind::Api(refusal), "{call}: {error:?}");
    }
}

#[test]
fn a_position_measures_reach_and_distance_in_the_maps_metric() {
    let mut game = units_match();
    let of = melee(&mut game, (0, 0, 0), Team::new(0), ());
    melee(&mut game, (3, 0, 4), Team::new(1), ());
    melee(&mut game, (0, 12, 3), Team::new(1), ());
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
        game.resource_mut::<MapBook>().metric = metric;
        let read = Units::probe(&mut game, probe, of)
            .unwrap()
            .cast::<ScriptList>();
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
    let error = Units::probe(
        &mut game,
        "fn probe(ctx, of) { of.pos.within(of.pos, -1) }",
        of,
    )
    .unwrap_err();
    assert_eq!(error.kind(), FailureKind::Api(ApiError::NegativeRadius));
}

#[test]
fn vectors_add_subtract_and_scale_each_rounded_once() {
    let mut game = units_match();
    let of = melee(&mut game, (0, 0, 0), Team::new(0), ());
    melee(&mut game, (3, 0, 4), Team::new(1), ());
    let probe = r#"fn probe(ctx, of) {
        let d = of.pos.direction_to(ctx.find(of, of.pos, 6, "enemies")[0].pos);
        [d + d, d - d, d * 5, 5 * d, d * (num(1) / 2), (num(1) / 2) * d]
    }"#;
    // The direction to (3, 0, 4) is (0.6, 0, 0.8): 10 066 329.6 and 13 421 772.8 in 24 fraction
    // bits, each rounded once, to 10 066 330 and 13 421 773. Their sum doubles each, the
    // difference is none, and 5 times each is exact. Half of 13 421 773 is 6 710 886.5, a tie,
    // which goes to the even 6 710 886.
    let vector = |x, z| Vec3::new(Num::from_bits(x), Num::ZERO, Num::from_bits(z));
    let read = Units::probe(&mut game, probe, of)
        .unwrap()
        .cast::<ScriptList>();
    let read: Vec<Vec3> = read.into_iter().map(ScriptValue::cast::<Vec3>).collect();
    let five = vector(5 * 10_066_330, 5 * 13_421_773);
    let half = vector(5_033_165, 6_710_886);
    assert_eq!(
        read,
        [
            vector(20_132_660, 26_843_546),
            Vec3::ZERO,
            five,
            five,
            half,
            half
        ]
    );
    // Turned a quarter about the vertical, (x, z) goes to (z, −x), exact; 2³⁸ degrees, whose
    // radians would pass what a number holds, turn as their 184° modulo 360° does.
    let probe = r#"fn probe(ctx, of) {
        let d = of.pos.direction_to(ctx.find(of, of.pos, 6, "enemies")[0].pos);
        [d.rotated_deg(90), d.rotated_deg(1 << 38), d.rotated_deg(184)]
    }"#;
    let read = Units::probe(&mut game, probe, of)
        .unwrap()
        .cast::<ScriptList>();
    let read: Vec<Vec3> = read.into_iter().map(ScriptValue::cast::<Vec3>).collect();
    assert_eq!(read[0], vector(13_421_773, -10_066_330));
    assert_eq!(read[1], read[2]);
    // 0.8 × 2³⁹ is about 4.4 × 10¹¹, within a number's 5.5 × 10¹¹; its double is past it, and
    // the sum fails the call.
    let error = Units::read(&mut game,
            r#"let d = of.pos.direction_to(ctx.find(of, of.pos, 6, "enemies")[0].pos) * (1 << 38) * 2; d + d"#,
            of,
        )
        .unwrap_err();
    assert_eq!(error.kind(), FailureKind::Raised(Some(NumError::Overflow)));
}

#[test]
fn the_view_reads_the_maps_bounds_or_the_worlds() {
    let mut game = units_match();
    let view = game.non_send::<View>().clone();
    // A match with no mode has the whole world's bounds; a mode's map gives its own.
    view.read(&mut game);
    assert_eq!(view.bounds(), Bounds::WORLD);
    let bounds = Bounds::new([Num::int(-10), Num::int(-5)], [Num::int(10), Num::int(6)]).unwrap();
    game.resource_mut::<MapBook>().bounds = bounds;
    view.read(&mut game);
    assert_eq!(view.bounds(), bounds);

    // A restored unit stands within the map's bounds, as every system keeps it: spawned at the
    // origin and moved to the edge at x = 10, it restores; moved a little past it, though well
    // within the world's bound, it does not.
    let restores = |x: Num| {
        let mut game = units_match();
        game.resource_mut::<MapBook>().bounds = bounds;
        let id = spawn_named(&mut game, "unit", Vec3::ZERO, Team::new(0));
        *game.part_mut::<Position>(id) = Position::from(Vec3::new(x, Num::ZERO, Num::ZERO));
        let mut snapshot = Vec::new();
        game.snapshot(&mut snapshot);
        let mut fresh = units_match();
        fresh.resource_mut::<MapBook>().bounds = bounds;
        let unit = |game: &Match| Units::type_named(game, "unit");
        assert_eq!(unit(&fresh), unit(&game));
        Release::clear(&mut fresh);
        let restored = fresh.restore(&snapshot);
        restored.map_err(|error| (error, id))
    };
    assert_eq!(restores(Num::int(10)), Ok(()));
    let (error, unit) = restores(Num::int(10) + Num::EPSILON).unwrap_err();
    let past = SnapshotError::Invalid {
        ty: Position::NAME,
        id: Some(unit),
    };
    assert_eq!(error, past);
}

#[test]
fn a_read_fills_again_the_rows_whose_parts_changed_came_or_went() {
    // A read keeps the row of each unit whose parts did not change since the last read, and a
    // debug build checks every read against one that fills every row. A part that only goes
    // away, and a team an entity gains, change no part's tick, only the unit's archetype. The
    // loose unit loses its team at once, as no spawn makes a unit of no team.
    let mut game = units_match();
    let of = melee(&mut game, (0, 0, 0), Team::new(0), ());
    let other = melee(&mut game, (3, 0, 4), Team::new(1), ());
    let loose = spawn_named(&mut game, "unit", (1, 0, 0), Team::new(1));
    let loose_entity = game.unit_entity(loose);
    game.entity_mut(loose_entity).remove::<Team>();
    let value =
        |game: &mut Match, unit, expression: &str| Units::read(game, expression, unit).unwrap();
    let enemies =
        |game: &mut Match| Unit::ids(value(game, of, r#"ctx.find(of, of.pos, 100, "enemies")"#));
    assert_eq!(enemies(&mut game), [other]);
    assert!(value(&mut game, other, "of.alive").as_bool().unwrap());

    let entity = game.unit_entity(other);
    game.entity_mut(entity).insert(Dead);
    assert!(!value(&mut game, other, "of.alive").as_bool().unwrap());
    assert_eq!(enemies(&mut game), []);
    game.entity_mut(entity).remove::<Dead>();
    assert!(value(&mut game, other, "of.alive").as_bool().unwrap());
    assert_eq!(enemies(&mut game), [other]);

    // The entity with no team is no unit until it gains one; one that loses its team is none.
    game.entity_mut(loose_entity).insert(Team::new(1));
    assert!(value(&mut game, loose, "of.alive").as_bool().unwrap());
    game.entity_mut(entity).remove::<Team>();
    assert_eq!(enemies(&mut game), []);
    assert!(game.non_send::<View>().row(other).is_none());

    // Bevy's check of the world's change ticks, once they advanced past its threshold, makes the
    // next read fill every row, as it clamps old ticks but not the view's last read: a move
    // that hid from change detection shows then, where a debug read's check would fail. The
    // tick that runs then makes the check, as a schedule's run does, so a later check finds it
    // done.
    let mut pos = game.get_mut::<Position>(loose_entity).unwrap();
    *pos.bypass_change_detection() = Position::from((2, 0, 0));
    for _ in 0..CHECK_TICK_THRESHOLD {
        game.increment_change_tick();
    }
    game.step();
    assert!(game.check_change_ticks().is_none());
    let moved = value(&mut game, loose, "of.pos");
    assert_eq!(moved.cast::<Vec3>(), Vec3::from((2, 0, 0)));
}

#[test]
fn a_handle_reads_its_units_fields_as_the_view_read_them() {
    let mut game = units_match();
    let window = ("help_window_ms", Scalar::Int(2000));
    let range = ("aggro_range", Scalar::Decimal(Num::int(7)));
    let hero = Units::tagged_type(&mut game, &["avatar"], &[window, range]);
    arm(&mut game, hero);
    // On a path, but the scene has no `navigation` to fill `unit.path`.
    let owner = Owner::new(PlayerSlot::new(2));
    let path = OnPath::new(PathId::new(0));
    let body = Body::new(Num::from_bits(3 << (Num::FRAC_BITS - 2))).unwrap();
    let spawn = SpawnPoint::new(Vec3::from((5, 0, -5)));
    let of = Mode::spawn(
        &mut game,
        hero,
        (0, 0, 0),
        Team::new(0),
        (owner, path, body, spawn),
    );
    let near = melee(&mut game, (3, 0, 4), Team::new(1), ());
    let recent = melee(&mut game, (9, 0, 0), Team::new(1), ());
    let fallen = melee(&mut game, (9, 0, 1), Team::new(1), Dead);
    let bare = spawn_named(&mut game, "bare", (9, 0, 2), Team::new(1));
    let entity = game.unit_entity(of);
    game.get_mut::<ActionSlots>(entity)
        .unwrap()
        .set_attack_target(Some(near));

    // At 30 ticks a second, 2000 ms is 60 ticks: in tick 100, a strike in tick 40 is recent, and
    // one in tick 39 is not; a dead attacker is never returned.
    game.insert_resource(SimTick::new(Tick::new(100)));
    let index = game.resource::<EntityIndex>();
    let mut attackers = RecentAttackers::default();
    // A strike later than the view's tick, as a rollback can leave, is not recent either.
    for (source, tick) in [(near, 39), (recent, 40), (fallen, 100), (bare, 101)] {
        attackers.struck(source, Tick::new(tick), index);
    }
    *game.get_mut::<RecentAttackers>(entity).unwrap() = attackers;

    let read = |game: &mut Match, expression: &str| Units::read(game, expression, of);
    let value = |game: &mut Match, expression: &str| read(game, expression).unwrap();
    assert!(value(&mut game, "of.is_avatar").as_bool().unwrap());
    assert!(value(&mut game, "of.alive").as_bool().unwrap());
    assert_eq!(value(&mut game, "of.owner").as_int(), Some(2));
    assert!(value(&mut game, "of.path").is_unit());
    // With no `vision`, every team sees every unit.
    assert!(value(&mut game, "of.can_see(of)").as_bool().unwrap());
    // Its body's radius, 0.75 m; a unit with no body has none.
    let radius = |game: &mut Match, unit| {
        let source = "fn probe(ctx, of) { of.radius }";
        Units::probe(game, source, unit).unwrap().cast::<Num>()
    };
    assert_eq!(radius(&mut game, of), body.radius().unwrap());
    assert_eq!(radius(&mut game, near), Num::ZERO);
    // Where it spawned, which a delivery has none of.
    let spawn_pos = |game: &mut Match, unit| {
        let source = "fn probe(ctx, of) { of.spawn_pos }";
        Units::probe(game, source, unit).unwrap()
    };
    assert_eq!(spawn_pos(&mut game, of).cast::<Vec3>(), spawn.get());
    let unit_type = Units::type_named(&game, "unit").unwrap();
    let delivery = Deliveries::spawn_of(&mut game, of, unit_type, (1, 0, 0), ());
    assert!(spawn_pos(&mut game, delivery).is_unit());
    assert_eq!(
        value(&mut game, "of.params.help_window_ms").as_int(),
        Some(2000)
    );
    assert_eq!(
        value(&mut game, "of.params.aggro_range").cast::<Num>(),
        Num::int(7)
    );
    assert_eq!(
        value(&mut game, "of.attack_range").cast::<Num>(),
        Num::int(2)
    );
    assert!(
        value(&mut game, "of.target.is_enemy_of(of)")
            .as_bool()
            .unwrap()
    );
    assert!(!value(&mut game, "of.target.is_avatar").as_bool().unwrap());
    assert!(
        value(&mut game, "of.target != () && of != ()")
            .as_bool()
            .unwrap()
    );
    let attackers = value(&mut game, "of.recent_attackers(2000)");
    assert_eq!(Unit::ids(attackers), [recent]);
    // 2034 ms is 61.02 ticks, up to 62: tick 39 is in.
    let attackers = value(&mut game, "of.recent_attackers(2034)");
    assert_eq!(Unit::ids(attackers), [near, recent]);

    let refusals = [
        ("of.params.gold", ApiError::Unknown(NameKind::Param)),
        ("of.recent_attackers(-1)", ApiError::NegativeTime),
    ];
    for (expression, refusal) in refusals {
        let error = read(&mut game, expression).unwrap_err();
        assert_eq!(error.kind(), FailureKind::Api(refusal), "{expression}");
    }
    let error = Units::probe(&mut game, "fn probe(ctx, of) { of.attack_range }", bare).unwrap_err();
    assert_eq!(
        error.kind(),
        FailureKind::Api(ApiError::NoAttack),
        "{error:?}"
    );

    // A target the view did not read is `()`.
    let entity = game.unit_entity(near);
    game.despawn(entity);
    assert!(value(&mut game, "of.target == ()").as_bool().unwrap());
}

#[test]
fn a_handle_reads_its_units_level_pools_and_stats() {
    let mut game = units_match();
    // Level 3, with armor 25 and health 10, the mode's stats; its health pool of 10, 4 taken,
    // and no mana pool. One unit has no pool; a delivery, which is no body, has no stats.
    let rules = ["armor", "health"].map(|name| (Stat::named(name).unwrap(), StatRule::default()));
    Stats::load_rules(&mut game, &BTreeMap::from(rules));
    let melee_type = Units::type_named(&game, "melee").unwrap();
    Stats::give_type_stats(&mut game, melee_type, &[Num::int(25), Num::int(10)]);
    let of = melee(&mut game, (0, 0, 0), Team::new(0), Level::new(3).unwrap());
    let entity = game.unit_entity(of);
    let mut life = game.get_mut::<Life>(entity).unwrap();
    life.take(Num::int(4));
    let shell = spawn_named(&mut game, "unit", (2, 0, 0), Team::new(1));
    let unit_type = Units::type_named(&game, "unit").unwrap();
    let bare = Deliveries::spawn_of(&mut game, of, unit_type, (1, 0, 0), ());
    // The refresh derives the stats.
    game.step();

    let read = |game: &mut Match, unit, expression: &str| Units::read(game, expression, unit);
    let value = |game: &mut Match, expression: &str| read(game, of, expression).unwrap();
    assert_eq!(value(&mut game, "of.level").as_int(), Some(3));
    assert_eq!(
        value(&mut game, r#"of.pool("health")"#).cast::<Num>(),
        Num::int(6)
    );
    assert_eq!(
        value(&mut game, r#"of.pool_max("health")"#).cast::<Num>(),
        Num::int(10)
    );
    assert_eq!(
        value(&mut game, r#"of.stat("armor")"#).cast::<Num>(),
        Num::int(25)
    );
    assert_eq!(
        value(&mut game, r#"of.stat("health")"#).cast::<Num>(),
        Num::int(10)
    );

    let refusals = [
        (
            of,
            r#"of.stat("spirit")"#,
            ApiError::Unknown(NameKind::Stat),
        ),
        (bare, "of.level", StatsRefusal::NoStats.refused()),
        (bare, r#"of.stat("armor")"#, StatsRefusal::NoStats.refused()),
        (of, r#"of.pool("mana")"#, StatsRefusal::NoPool.refused()),
        (
            of,
            r#"of.pool_max("rage")"#,
            ApiError::Unknown(NameKind::Pool),
        ),
        (
            shell,
            r#"of.pool("health")"#,
            StatsRefusal::NoPool.refused(),
        ),
        (
            shell,
            r#"of.pool_max("health")"#,
            StatsRefusal::NoPool.refused(),
        ),
    ];
    for (unit, expression, refusal) in refusals {
        let error = read(&mut game, unit, expression).unwrap_err();
        assert_eq!(
            error.kind(),
            FailureKind::Api(refusal),
            "{expression}: {error:?}"
        );
    }
}

#[test]
fn a_unit_type_name_is_one_types_only_in_its_scope() {
    let mut game = Match::unscripted(&[Capability::Stats, Capability::Combat]);
    let data = UnitTypeCore::default();
    let mut load = |scope, name| Units::load_type(&mut game, scope, name, &data);
    let first = load(TypeScope::Mode, "grunt");
    let second = load(TypeScope::Mode, "tower");
    // A package's own scope holds a `grunt` of its own, which a mode name does not reach.
    let theirs = load(TypeScope::Package(PackageIndex::new(1)), "grunt");
    // A scope names a type once.
    assert_eq!(
        game.resource_mut::<UnitTypeBook>().load(
            TypeScope::Package(PackageIndex::new(1)),
            PackageIndex::new(1),
            "grunt",
            &data
        ),
        Err(TypesError::Repeated("grunt".to_owned()))
    );
    let view = game.non_send::<View>();
    assert_eq!(
        (
            view.names().named((TypeScope::Mode, "grunt")),
            view.names().named((TypeScope::Mode, "tower"))
        ),
        (Some(first), Some(second))
    );
    assert_eq!(
        view.names().named::<UnitType>((TypeScope::Mode, "wolf")),
        None
    );
    let names = view.names();
    let types = names.table::<UnitType>().unwrap();
    assert_eq!(
        types.named((TypeScope::Package(PackageIndex::new(1)), "grunt")),
        Some(theirs)
    );
    assert_eq!(
        types.named((TypeScope::Package(PackageIndex::new(2)), "grunt")),
        None
    );
    assert_ne!(theirs, first);
    drop(names);
    // Each type's origin: its package and its name there.
    let origins = game.resource_mut::<UnitTypeBook>().origin_book();
    let origin = |unit_type| {
        let origin = origins.of(unit_type);
        (origin.package.get(), &*origin.name)
    };
    assert_eq!(
        [first, second, theirs].map(origin),
        [(0, "grunt"), (0, "tower"), (1, "grunt")]
    );
    let every: Vec<UnitType> = origins.iter().map(|(unit_type, _)| unit_type).collect();
    assert_eq!(every, [first, second, theirs]);
}

#[test]
fn an_entity_is_of_the_one_kind_its_marker_names() {
    let mut game = units_match();
    let unit = spawn_named(&mut game, "unit", (0, 0, 0), Team::new(0));
    let entity = game.unit_entity(unit);
    let empty = game.spawn_empty().id();
    let kinds = game.resource::<EntityKinds>();
    let body = game.components().component_id::<BodyUnit>();
    assert_eq!(kinds.of(game.entity(entity)), body);
    assert_eq!(kinds.of(game.entity(empty)), None);
    // A restore puts untrusted values in: the hook leaves the rule to the restore checks, and
    // the body unit's own check refuses a unit of a second kind.
    let restored = spawn_named(&mut game, "unit", (1, 0, 0), Team::new(0));
    let restored = game.unit_entity(restored);
    game.insert_resource(Restoring);
    game.entity_mut(restored).insert(DeliveryEntity);
    game.remove_resource::<Restoring>();
    assert!(BodyUnit.check(&game, entity) && !BodyUnit.check(&game, restored));
    // Outside a restore, a second kind is a logic error.
    let second = panic::catch_unwind(AssertUnwindSafe(|| {
        game.entity_mut(entity).insert(DeliveryEntity);
    }));
    let message = second.unwrap_err();
    let message = message.downcast_ref::<String>().unwrap();
    assert!(message.starts_with("an entity is of one kind"), "{message}");
}

#[test]
fn a_unit_holds_its_types_tags_on_every_path_in_a_match_without_stats() {
    // A match of the core alone, a unit of a type tagged `creep`: spawned, restored, and given its
    // state alone, as a client's replication gives it, it holds its type's tag each way.
    let start = || {
        let mut game = Match::test(&[], Match::TEST_RATE, None);
        let creep = Units::tagged_type(&mut game, &["creep"], &[]);
        let hero = Units::tagged_type(&mut game, &["avatar"], &[]);
        (game, creep, hero)
    };
    let (mut game, creep, hero) = start();
    let spawned = Mode::spawn(&mut game, creep, (0, 0, 0), Team::new(0), ());
    let own = game.resource::<TagBook>().own(creep);
    assert_eq!(own.iter().count(), 1);
    let tags = |game: &Match, id| {
        game.get::<UnitTags>(game.unit_entity(id))
            .map(|tags| tags.tags)
    };
    assert_eq!(tags(&game, spawned), Some(own));
    let (mut restored, ..) = start();
    Release::restore_into(&game, &mut restored);
    assert_eq!(tags(&restored, spawned), Some(own));
    let id = game.resource_mut::<IdAllocator>().allocate();
    let received = (
        id,
        BodyUnit,
        creep,
        Position::from(Vec3::ZERO),
        Team::new(0),
    );
    let received = game.spawn(received).id();
    assert_eq!(
        game.get::<UnitTags>(received).map(|tags| tags.tags),
        Some(own)
    );
    // A unit of a type tagged `avatar` is marked an avatar exactly while its type is one.
    assert!(!game.entity(received).contains::<Avatar>());
    game.entity_mut(received).insert(hero);
    assert!(game.entity(received).contains::<Avatar>());
    game.entity_mut(received).insert(creep);
    assert!(!game.entity(received).contains::<Avatar>());
}

#[test]
fn a_spawn_gives_its_declared_parts_in_one_move() {
    // A melee unit spawns with the parts combat, stats and actions declare for it, `Life`,
    // `Level` and `ActionSlots` among them: written at once, they reach one archetype only, the
    // unit's, where a part put on alone would leave an archetype of each part before the last.
    let mut game = units_match();
    melee(&mut game, (0, 0, 0), Team::new(0), ());
    let holding = |game: &Match, id: Option<ComponentId>| {
        let id = id.expect("a declared part's component");
        let archetypes = game.archetypes().iter();
        archetypes
            .filter(|archetype| archetype.contains(id))
            .count()
    };
    let components = game.components();
    let parts = [
        components.component_id::<Life>(),
        components.component_id::<Level>(),
        components.component_id::<ActionSlots>(),
    ];
    assert_eq!(parts.map(|id| holding(&game, id)), [1, 1, 1]);
}

mod entity_kinds;
mod status_tags;
mod tag_book;
mod unit_grid;
mod unit_types;
