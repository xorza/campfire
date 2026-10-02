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
    let vision = *game.sim.world.resource::<VisionGrid>();
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
    // (−10, 3, 0); b from its end, (10, 3, 0).
    let script = r#"
fn on_match_start(ctx) {
    ctx.spawn_group("a", "mid", "start", ["grunt"]);
    ctx.spawn_group("b", "mid", "end", ["grunt"]);
    ctx.spawn_group("neutral", "mid", "start", ["grunt"]);
}

fn on_mode_input(ctx, player, name, value) {
    ctx.spawn_group("a", "mid", "middle", ["grunt"]);
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
    // An end other than `start` and `end` fails.
    game.tick(&[(0, input("phase", "x"))]);
    assert_eq!(
        game.failures(),
        [FailureKind::Api(ApiError::UnknownPathEnd)]
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
    let resolve = |count| ModeMap::resolve(&files.map, &teams(count), &[], |_| None).err();
    assert_ne!(resolve(64), Some(ModeError::TooManyVisionTeams));
    assert_eq!(resolve(65), Some(ModeError::TooManyVisionTeams));
}
