use campfire_script::CapabilityRefusal;
use campfire_sim::Position;
use campfire_units::{CellSplit, CoarseSpan, Ground, GroundData, GroundError, HeightGrid, MapBook};
use campfire_vision::VisionBook;

use super::*;

#[test]
fn the_start_spawns_the_map_then_runs_on_match_start_and_timers_never_fire_early() {
    let mut game = mode_match(SCRIPT, ScriptLimits::ROOMY);
    // Before tick 0: the map's tower, 0, then the match start's spawns in order: the camp's
    // grunt, 1, at its marker; team a's spawn group of two, 2 and 3, at the path's start; team
    // b's spawn group of one, 4, at its end. Teams a and b are 0 and 1, neutral 2.
    assert_eq!(
        Mode::placed_units(&game),
        [
            (0, Vec3::from((-8, 0, 0)), 0, None),
            (1, Vec3::from((0, 0, 0)), 2, None),
            (2, Vec3::from((-10, 0, 0)), 0, Some(PathEnd::Start)),
            (3, Vec3::from((-10, 0, 0)), 0, Some(PathEnd::Start)),
            (4, Vec3::from((10, 0, 0)), 1, Some(PathEnd::End)),
        ]
    );
    let tower = Mode::entity_by_id(&game, 0);
    assert_eq!(
        game.get::<OnPath>(tower).map(|path| path.get()),
        Some(PathId::new(0))
    );
    // Each holds its stats from its spawn, before any refresh: a value for each stat the mode
    // declares, its type's at level 1, with no modifiers, as its first refresh derives them.
    let tower_type = *game.get::<UnitType>(tower).unwrap();
    let base = game.resource::<StatBook>().base_values(tower_type, 1);
    assert_eq!(base.len(), STATS_3V3.len());
    assert_eq!(game.get::<UnitStats>(tower).unwrap().values(), base);
    // Beside them, the match's entity and each player's, of their named ids: players 0 and 1 of
    // team a, player 2 of b.
    let named: Vec<_> = game
        .resource::<EntityIndex>()
        .iter()
        .filter(|(id, _)| !id.is_allocated())
        .map(|(id, entity)| {
            (
                id,
                game.get::<Player>(entity).copied(),
                game.get::<Team>(entity),
            )
        })
        .collect();
    let player = |slot| {
        (
            StableId::of_player(PlayerSlot::new(slot)),
            Some(Player::new(PlayerSlot::new(slot))),
        )
    };
    let [zero, one, two] = [0, 1, 2].map(player);
    assert_eq!(
        named,
        [
            (StableId::MATCH, None, None),
            (zero.0, zero.1, Some(&Team::new(0))),
            (one.0, one.1, Some(&Team::new(0))),
            (two.0, two.1, Some(&Team::new(1))),
        ]
    );
    assert!(
        game.entity(game.match_entity().unwrap())
            .contains::<MatchEntity>()
    );
    assert_eq!(game.player_entity(PlayerSlot::new(3)), None);
    // The map's grid is the match's, for its 3 teams: a, b and the neutral one.
    let vision = game.resource::<VisionBook>().grid().unwrap();
    let grid = Grid::new(Num::int(1), map().bounds()).unwrap();
    assert_eq!((vision.grid, vision.teams), (grid, 3));
    assert_eq!(game.resource::<MapBook>().bounds, map().bounds());
    // Each unit type has the tag of the layer it moves on: the tower, of the second layer, `air`;
    // the grunt, with no body, the first's, `ground`.
    let tags = |id| {
        game.get::<UnitTags>(Mode::entity_by_id(&game, id))
            .unwrap()
            .tags
    };
    let tag = |name| game.non_send::<View>().names().tags().named(name).unwrap();
    let layer_tags = [0, 1].map(|id| [tag("ground"), tag("air")].map(|tag| tags(id).contains(tag)));
    assert_eq!(layer_tags, [[false, true], [true, false]]);
    assert_eq!(Mode::tracked_state(&game), state("start", 0, 0, 0));

    // Set at the start, time 0: "every" is due at 1, the end of tick 0, and every tick after;
    // "once", 250 ms, 2.5 ticks rounded up to 3, at the end of tick 2, with its data.
    let mut seen = Vec::new();
    for _ in 0..4 {
        Mode::tick_inputs(&mut game, &[]);
        seen.push(Mode::tracked_state(&game));
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
    assert!(Mode::failures(&game).is_empty());
}

#[test]
fn a_mode_whose_start_fails_starts_no_match() {
    for unit_type in ["ghost", "bolt"] {
        let failing = format!(
            "fn on_match_start(ctx) {{ ctx.spawn_unit(\"{unit_type}\", \"a\", ctx.map.markers(\"camp\")[0].pos); }}"
        );
        let failed = start_match(&failing, ScriptLimits::ROOMY, &mode_files()).err();
        assert_eq!(
            failed.as_ref().map(CallError::kind),
            Some(FailureKind::Api(ApiError::Unknown(NameKind::UnitType))),
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
    let mut game = start_match(script, ScriptLimits::ROOMY, &files).unwrap();
    assert_eq!(game.resource::<MapBook>().metric, Metric::Spatial);
    let up = |x| Vec3::new(Num::int(x), Num::int(3), Num::ZERO);
    let walkers = |game: &Match| <[_; 3]>::try_from(&Mode::placed_units(game)[1..]).unwrap();
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
    let paths = &game.resource::<NavigationBook>().paths;
    let last = walkers(&game).map(|unit| paths.waypoint(PathId::new(0), 2, unit.3.unwrap()));
    assert_eq!(last, [Some(up(10)), Some(up(-10)), Some(up(10))]);
    // A text that names no end fails the call of `named`, and a string in place of a member
    // fails the call of `spawn_group`, as no form of it takes one.
    Mode::tick_inputs(
        &mut game,
        &[(0, input("phase", "named")), (0, input("phase", "text"))],
    );
    assert_eq!(
        Mode::failures(&game),
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
    let layers = Names::default();
    let resolve = |count| {
        let teams = teams(count);
        let input = MapInput {
            map: &files.map,
            heights: None,
            teams: &teams,
            relations: &[],
            layers: &layers,
        };
        ModeMap::resolve(input, |_| None).err()
    };
    assert_ne!(resolve(64), Some(ModeError::TooManyVisionTeams));
    assert_eq!(resolve(65), Some(ModeError::TooManyVisionTeams));
}

#[test]
fn a_maps_ground_is_its_height_grid_as_its_ground_section_reads_it() {
    // A map of 3 × 3 samples 10 m apart, from the origin. With no grid it is flat, and a
    // `[ground]` with no grid is refused; with one, its split and coarse cells are those of its
    // `[ground]`, the grid's own cells with none, and coarse cells smaller than its cells are
    // refused.
    let files = mode_files();
    let layers = Names::default();
    let grid = HeightGrid::new(
        [Num::ZERO; 2],
        Num::int(10),
        Num::ONE,
        3,
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8],
    )
    .unwrap();
    let span = |cell: i64| CoarseSpan {
        cell: Num::int(cell),
        from: [Num::ZERO; 2],
        to: [Num::int(20); 2],
    };
    let resolve = |ground: Option<GroundData>, heights: Option<&HeightGrid>| {
        let map = MapData::planar(Bounds::new([Num::int(-50); 2], [Num::int(50); 2]).unwrap());
        let map = match ground {
            Some(ground) => map.with(ground),
            None => map,
        };
        let input = MapInput {
            map: &map,
            heights,
            teams: &files.teams,
            relations: &[],
            layers: &layers,
        };
        ModeMap::resolve(input, |_| None).map(|resolved| resolved.ground.ground)
    };
    let read = |split, cell| GroundData {
        split,
        high: Some(span(cell)),
    };
    assert_eq!(resolve(None, None), Ok(Ground::default()));
    assert_eq!(
        resolve(Some(GroundData::default()), None),
        Err(ModeError::NoHeights)
    );
    let own = Ground::new(grid.clone(), CellSplit::ZeroTwo, None).unwrap();
    assert_eq!(resolve(None, Some(&grid)), Ok(own));
    let coarse = Ground::new(grid.clone(), CellSplit::OneThree, Some(span(20))).unwrap();
    assert_eq!(
        resolve(Some(read(CellSplit::OneThree, 20)), Some(&grid)),
        Ok(coarse)
    );
    assert_eq!(
        resolve(Some(read(CellSplit::OneThree, 5)), Some(&grid)),
        Err(ModeError::Ground(GroundError::CoarseCell))
    );
}

#[test]
fn a_box_spawns_only_where_it_has_room() {
    // Crates of 2 by 2 m on the first layer, where the map's tower is not, at markers. The start
    // spawns one at (0, 2). An input then asks for one at (1, 2), over it; for two in one call, at
    // (4, 2) and (5, 2), the second over the first; for one at (10, 0), past the bounds at
    // x = 10; and for one at (2, 2), which touches the first, and has room. Each refused call
    // fails, and spawns nothing. A vector moves a marker's point: c0's 3 m along +z is (0, 5),
    // clear of the first. A point 2,000,000 m off is past the world's bound of 2²⁰ m, which
    // the call refuses before it reads the point.
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
    } else if value == "shifted" {
        ctx.spawn_unit("crate", "a", ctx.map.markers("c0")[0].pos + vector(0, 0, 3));
    } else if value == "far" {
        ctx.spawn_unit("crate", "a", vector(0, 0, 2000000));
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
    let mut game = start_match(&format!("{script}{PICKING}"), ScriptLimits::ROOMY, &files).unwrap();
    let crates = |game: &Match| {
        let mut at: Vec<Vec3> = game
            .resource::<EntityIndex>()
            .iter()
            .map(|(_, entity)| game.entity(entity))
            .filter(|unit| {
                unit.get::<Body>()
                    .is_some_and(|body| body.half_edges().is_some())
            })
            .map(|unit| unit.get::<Position>().unwrap().get())
            .collect();
        at.sort_unstable_by_key(|pos| (pos.x, pos.z));
        at
    };
    let ground = |x: i64, z: i64| Vec3::new(Num::int(x), Num::ZERO, Num::int(z));
    assert_eq!(crates(&game), [ground(0, 2)]);
    for value in ["c1", "twice", "c10"] {
        Mode::tick_inputs(&mut game, &[(0, input("phase", value))]);
        assert_eq!(crates(&game), [ground(0, 2)], "{value}");
        assert_eq!(
            Mode::failures(&game).last(),
            Some(&FailureKind::Api(ModeRefusal::NoRoom.refused())),
            "{value}"
        );
    }
    Mode::tick_inputs(&mut game, &[(0, input("phase", "c2"))]);
    assert_eq!(crates(&game), [ground(0, 2), ground(2, 2)]);
    assert!(Mode::failures(&game).is_empty());
    Mode::tick_inputs(&mut game, &[(0, input("phase", "shifted"))]);
    assert_eq!(crates(&game), [ground(0, 2), ground(0, 5), ground(2, 2)]);
    Mode::tick_inputs(&mut game, &[(0, input("phase", "far"))]);
    assert_eq!(crates(&game), [ground(0, 2), ground(0, 5), ground(2, 2)]);
    assert_eq!(
        Mode::failures(&game).last(),
        Some(&FailureKind::Api(ApiError::PastBound))
    );
    // Two calls of one batch, two players' inputs of one tick, each place a crate at c4: the
    // first takes the room, which the second finds taken, though the batch's view, read before
    // the first, holds no crate there.
    let both = [(0, input("phase", "c4")), (1, input("phase", "c4"))];
    Mode::tick_inputs(&mut game, &both);
    let placed = [ground(0, 2), ground(0, 5), ground(2, 2), ground(4, 2)];
    assert_eq!(crates(&game), placed);
    assert_eq!(
        Mode::failures(&game),
        [FailureKind::Api(ModeRefusal::NoRoom.refused())]
    );
}

#[test]
fn a_placed_unit_stands_at_its_height_and_faces_its_angle() {
    // The tower placed 3 m above the ground at (-8, 0), turned by -90°: it faces 270°, and every
    // unit the start spawns faces 0°.
    let mut files = mode_files();
    files.map.units[0].height = Num::int(3);
    files.map.units[0].angle = Num::int(-90);
    let game = start_match(SCRIPT, ScriptLimits::ROOMY, &files).unwrap();
    let tower = game.entity(Mode::entity_by_id(&game, 0));
    let raised = Vec3::new(Num::int(-8), Num::int(3), Num::ZERO);
    assert_eq!(tower.get::<Position>().map(|pos| pos.get()), Some(raised));
    assert_eq!(
        tower.get::<Facing>().map(|facing| facing.degrees()),
        Some(Num::int(270))
    );
    let grunt = game.entity(Mode::entity_by_id(&game, 1));
    assert_eq!(
        grunt.get::<Facing>().map(|facing| facing.degrees()),
        Some(Num::ZERO)
    );

    // A height past the world's bound; and on a spatial map, whose points hold their own, any
    // height but 0.
    let map = mode_files().map;
    let ground = MapPoint::ground(0, 0);
    assert_eq!(
        map.placed_at(&ground, Num::int(1 << 21)),
        Err(ModeError::OutOfBounds)
    );
    let map = map.with(MetricData(Metric::Spatial));
    let space = MapPoint::Space([Num::ZERO, Num::ONE, Num::ZERO]);
    let lifted = Vec3::new(Num::ZERO, Num::int(1), Num::ZERO);
    assert_eq!(map.placed_at(&space, Num::ZERO), Ok(lifted));
    assert_eq!(
        map.placed_at(&space, Num::int(3)),
        Err(ModeError::SpatialHeight)
    );
}
