use super::*;

#[test]
fn the_start_spawns_the_map_then_runs_on_match_start_and_timers_never_fire_early() {
    let mut game = Game::new(SCRIPT, ScriptLimits::ROOMY);
    // Before tick 0: the map's tower, 0, then the match start's spawns in order: the camp's
    // grunt, 1, at its marker; team a's spawn group of two, 2 and 3, at the path's start; team
    // b's spawn group of one, 4, at its end. Teams a and b are 0 and 1, neutral 2.
    assert_eq!(
        game.units(),
        [
            (0, at(-8, 0), 0, None),
            (1, at(0, 0), 2, None),
            (2, at(-10, 0), 0, Some(PathEnd::Start)),
            (3, at(-10, 0), 0, Some(PathEnd::Start)),
            (4, at(10, 0), 1, Some(PathEnd::End)),
        ]
    );
    let tower = game.entity(0);
    assert_eq!(
        game.sim.world.get::<OnPath>(tower).map(|path| path.get()),
        Some(PathId::new(0))
    );
    // The map's grid is the match's, for its 3 teams: a, b and the neutral one.
    let vision = game.sim.world.resource::<VisionGrid>();
    let grid = Grid::new(Num::int(1), map().bounds).unwrap();
    assert_eq!((vision.grid, vision.teams), (grid, 3));
    assert_eq!(*game.sim.world.resource::<Bounds>(), map().bounds);
    // Each unit type has the tag of the layer it moves on: the tower, of the second layer, `air`;
    // the grunt, with no body, the first's, `ground`.
    let tags = |id| {
        game.sim
            .world
            .get::<UnitTags>(game.entity(id))
            .unwrap()
            .tags
    };
    let tag = |name| {
        game.sim
            .world
            .non_send::<View>()
            .types_mut()
            .tag_named(name)
            .unwrap()
    };
    let layer_tags = [0, 1].map(|id| [tag("ground"), tag("air")].map(|tag| tags(id).contains(tag)));
    assert_eq!(layer_tags, [[false, true], [true, false]]);
    assert_eq!(game.state(), state("start", 0, 0, 0));

    // Set at the start, time 0: "every" is due at 1, the end of tick 0, and every tick after;
    // "once", 250 ms, 2.5 ticks rounded up to 3, at the end of tick 2, with its data.
    let mut seen = Vec::new();
    for _ in 0..4 {
        game.tick(&[]);
        seen.push(game.state());
    }
    assert_eq!(
        seen,
        [
            state("start", 0, 1, 0),
            state("start", 0, 2, 0),
            state("start", 7, 3, 0),
            state("start", 7, 4, 0),
        ]
    );
    assert!(game.failures().is_empty());
}

#[test]
fn a_mode_whose_start_fails_starts_no_match() {
    for unit_type in ["ghost", "bolt"] {
        let failing = format!(
            "fn on_match_start(ctx) {{ ctx.spawn_unit(\"{unit_type}\", \"a\", ctx.map.markers(\"camp\")[0].pos); }}"
        );
        let failed = Game::start(&failing, ScriptLimits::ROOMY, mode_files()).err();
        assert_eq!(
            failed.as_ref().map(CallError::kind),
            Some(FailureKind::Api(ApiError::UnknownUnitType)),
            "{unit_type}: {failed:?}"
        );
    }
}

#[test]
fn three_teams_walk_a_path_each_from_the_end_it_names() {
    // On the test map made spatial, 3 m up: team a and the neutral team walk `mid` from its start,
    // (−10, 3, 0); b from its end, (10, 3, 0). The neutral team's end is the member data names
    // `start`.
    let script = r#"
fn on_match_start(ctx) {
    ctx.spawn_group("a", "mid", PathEnd::Start, ["grunt"]);
    ctx.spawn_group("b", "mid", PathEnd::End, ["grunt"]);
    ctx.spawn_group("neutral", "mid", PathEnd::named("start"), ["grunt"]);
}

fn on_mode_input(ctx, player, name, value) {
    if value == "named" {
        ctx.spawn_group("a", "mid", PathEnd::named("middle"), ["grunt"]);
    } else {
        ctx.spawn_group("a", "mid", "start", ["grunt"]);
    }
}
"#;
    let mut files = mode_files();
    files.map = raised(files.map, 3);
    let mut game = Game::start(script, ScriptLimits::ROOMY, files).unwrap();
    assert_eq!(*game.sim.world.resource::<Metric>(), Metric::Spatial);
    let up = |x| Position::new(Vec3::new(Num::int(x), Num::int(3), Num::ZERO)).unwrap();
    let walkers = |game: &Game| <[_; 3]>::try_from(&game.units()[1..]).unwrap();
    assert_eq!(
        walkers(&game),
        [
            (1, up(-10), 0, Some(PathEnd::Start)),
            (2, up(10), 1, Some(PathEnd::End)),
            (3, up(-10), 2, Some(PathEnd::Start)),
        ]
    );
    // Each walks to the last waypoint from its end: a and the neutral team to (10, 3, 0), b to
    // (−10, 3, 0).
    let paths = game.sim.world.resource::<Paths>();
    let last = walkers(&game).map(|unit| paths.waypoint(PathId::new(0), 2, unit.3.unwrap()));
    assert_eq!(last, [Some(up(10)), Some(up(-10)), Some(up(10))]);
    // A text that names no end fails the call of `named`, and a string in place of a member
    // fails the call of `spawn_group`, as no form of it takes one.
    game.tick(&[(0, input("phase", "named")), (0, input("phase", "text"))]);
    assert_eq!(
        game.failures(),
        [
            FailureKind::Api(ApiError::UnknownMember(EngineEnum::PathEnd)),
            FailureKind::Runtime
        ]
    );
}

#[test]
fn a_map_with_vision_holds_at_most_64_teams() {
    // The test mode's map has a vision grid. Its teams are checked before its units, so a map
    // of 64 teams fails later, on the units of the teams it lacks, and one of 65 at once.
    let files = mode_files();
    let teams = |count: usize| -> Vec<TeamManifest> {
        (0..count)
            .map(|at| TeamManifest {
                name: DeclaredName::new(&format!("team{at}")).unwrap(),
                slots: 0,
            })
            .collect()
    };
    let rules = NavigationRules::default();
    let resolve = |count| ModeMap::resolve(&files.map, &teams(count), &[], &rules, |_| None).err();
    assert_ne!(resolve(64), Some(ModeError::TooManyVisionTeams));
    assert_eq!(resolve(65), Some(ModeError::TooManyVisionTeams));
}

#[test]
fn a_box_spawns_only_where_it_has_room() {
    // Crates of 2 by 2 m on the first layer, where the map's tower is not, at markers. The start
    // spawns one at (0, 2). An input then asks for one at (1, 2), over it; for two in one call, at
    // (4, 2) and (5, 2), the second over the first; for one at (10, 0), past the bounds at
    // x = 10; and for one at (2, 2), which touches the first, and has room. Each refused call
    // fails, and spawns nothing.
    let script = r#"
fn crate_at(ctx, marker) {
    ctx.spawn_unit("crate", "a", ctx.map.markers(marker)[0].pos)
}

fn on_match_start(ctx) {
    crate_at(ctx, "c0");
}

fn on_input(ctx, player, name, value) {
    if value == "twice" {
        crate_at(ctx, "c4");
        crate_at(ctx, "c5");
    } else {
        crate_at(ctx, value);
    }
}
"#;
    let mut files = mode_files();
    for (name, (x, z)) in [("c0", (0, 2)), ("c1", (1, 2)), ("c2", (2, 2))]
        .into_iter()
        .chain([("c4", (4, 2)), ("c5", (5, 2)), ("c10", (10, 0))])
    {
        files
            .map
            .markers
            .push(MarkerData::tagged(name, &[name], MapPoint::ground(x, z)));
    }
    let mut game = Game::start(&format!("{script}{PICKING}"), ScriptLimits::ROOMY, files).unwrap();
    let crates = |game: &Game| {
        let world = &game.sim.world;
        let mut at: Vec<Position> = world
            .resource::<EntityIndex>()
            .iter()
            .map(|(_, entity)| world.entity(entity))
            .filter(|unit| {
                unit.get::<Body>()
                    .is_some_and(|body| body.half_edges().is_some())
            })
            .map(|unit| *unit.get::<Position>().unwrap())
            .collect();
        at.sort_unstable_by_key(|pos| (pos.get().x, pos.get().z));
        at
    };
    let ground =
        |x: i64, z: i64| Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap();
    assert_eq!(crates(&game), [ground(0, 2)]);
    for value in ["c1", "twice", "c10"] {
        game.tick(&[(0, input("phase", value))]);
        assert_eq!(crates(&game), [ground(0, 2)], "{value}");
        assert_eq!(
            game.failures().last(),
            Some(&FailureKind::Api(ApiError::NoRoom)),
            "{value}"
        );
    }
    game.tick(&[(0, input("phase", "c2"))]);
    assert_eq!(crates(&game), [ground(0, 2), ground(2, 2)]);
    assert!(game.failures().is_empty());
}

#[test]
fn a_placed_unit_stands_at_its_height_and_faces_its_angle() {
    // The tower placed 3 m above the ground at (-8, 0), turned by -90°: it faces 270°, and every
    // unit the start spawns faces 0°.
    let mut files = mode_files();
    files.map.units[0].height = Num::int(3);
    files.map.units[0].angle = Num::int(-90);
    let game = Game::start(SCRIPT, ScriptLimits::ROOMY, files).unwrap();
    let tower = game.sim.world.entity(game.entity(0));
    let raised = Position::new(Vec3::new(Num::int(-8), Num::int(3), Num::ZERO)).unwrap();
    assert_eq!(tower.get::<Position>(), Some(&raised));
    assert_eq!(
        tower.get::<Facing>().map(|facing| facing.degrees()),
        Some(Num::int(270))
    );
    let grunt = game.sim.world.entity(game.entity(1));
    assert_eq!(
        grunt.get::<Facing>().map(|facing| facing.degrees()),
        Some(Num::ZERO)
    );

    // A height past the world's bound; and on a spatial map, whose points hold their own, any
    // height but 0.
    let mut map = mode_files().map;
    let ground = MapPoint::Ground([Scalar::Int(0), Scalar::Int(0)]);
    assert_eq!(
        map.placed_at(&ground, Num::int(1 << 21)),
        Err(ModeError::OutOfBounds)
    );
    map.metric = Metric::Spatial;
    let space = MapPoint::Space([Scalar::Int(0), Scalar::Int(1), Scalar::Int(0)]);
    let lifted = Position::new(Vec3::new(Num::ZERO, Num::int(1), Num::ZERO)).unwrap();
    assert_eq!(map.placed_at(&space, Num::ZERO), Ok(lifted));
    assert_eq!(
        map.placed_at(&space, Num::int(3)),
        Err(ModeError::SpatialHeight)
    );
}
