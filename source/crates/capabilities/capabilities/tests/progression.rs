use campfire_capabilities::{Match, Release};
use campfire_declarations::Role;
use campfire_declarations::internals::BookLoad;
use campfire_math::{Num, Vec3};
use campfire_progression::*;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, SimComponent, SimWorld, SnapshotError};
use campfire_stats::Level;
use campfire_units::internals::Spawn;
use campfire_units::{TrackLevels, UnitTypeCore, Units};
use campfire_values::TrackId;

const fn track(at: u8) -> TrackId {
    TrackId::new(at)
}

#[test]
fn experience_is_state_and_restores() {
    // A match of stats and progression with the tracks of `book`, the same in both, and a type
    // on both tracks and one on `valor` alone.
    let tracks = |game: &mut Match| {
        TrackBook::sample().load_into(game);
        let core = UnitTypeCore::default();
        [[track(0), track(1)].as_slice(), &[track(1)]].map(|tracks| {
            let unit_type = Units::load_next_type(game, &core);
            Units::give_type(game, unit_type, TrackSet::from_iter(tracks.iter().copied()));
            unit_type
        })
    };
    let declared = [Capability::Stats, Capability::Progression];
    let mut game = Match::builder(&declared).role(Role::Rules).build();
    let [on_both, on_valor] = tracks(&mut game);
    // A unit 150 into `level`, at level 2 with a point more than at its spawn, and 10 into
    // `valor`, still at level 1.
    let book = TrackBook::sample();
    let both = TrackSet::from([track(0), track(1)]);
    let mut experience = Experience::new(both);
    let mut points = Points::at_spawn(both, book.level_track()).unwrap();
    experience.add(track(0), Num::int(150), &book);
    experience.add(track(1), Num::int(10), &book);
    let level = experience.get(track(0)).unwrap().level;
    points.gain(1);
    let id = Spawn::of(on_both, Vec3::ZERO)
        .parts((level, experience, points))
        .place(&mut game);
    let mut restored = Match::builder(&declared).role(Role::Rules).build();
    tracks(&mut restored);
    Release::restore_into(&game, &mut restored);
    assert_eq!(restored.part::<Experience>(id), game.part::<Experience>(id));
    assert_eq!(restored.part::<Level>(id).get(), 2);
    assert_eq!(restored.part::<Points>(id).get(), 2);
    // A step shows each unit's levels once its experience changed, on the match and on its
    // restore: 2 on `level`, 1 on `valor`; before it, 1 on each, as on a track a unit lacks.
    // Then 40 more into `valor`, 50 in all, reach its level 2.
    let shown = |game: &Match| {
        let levels = game.part::<TrackLevels>(id);
        [track(0), track(1)].map(|track| levels.level(track).get())
    };
    assert_eq!(shown(&game), [1, 1]);
    for game in [&mut game, &mut restored] {
        game.step();
        assert_eq!(shown(game), [2, 1]);
    }
    game.part_mut::<Experience>(id)
        .add(track(1), Num::int(40), &book);
    game.step();
    assert_eq!(shown(&game), [2, 2]);
    // Only a unit with the `level` track has points: one with `valor` alone has none at its
    // spawn, and points on it fail the check.
    let valor = TrackSet::from([track(1)]);
    assert_eq!(Points::at_spawn(valor, book.level_track()), None);
    let held = Spawn::of(on_valor, Vec3::ZERO)
        .parts((Experience::new(valor), points))
        .place(&mut game);
    assert!(!points.check(&game, game.unit_entity(held)));
    assert!(points.check(&game, game.unit_entity(id)));
    // At level 2 it holds two points at most: a third, which no level gave, fails the check, as
    // its next level would pass the most a `u32` counts.
    let mut third = points;
    third.gain(1);
    assert!(!third.check(&game, game.unit_entity(id)));
    // A unit on the `level` track whose points are gone fails the restore, as its next level
    // would find no points to raise.
    let (held, unit) = (game.unit_entity(held), game.unit_entity(id));
    game.despawn(held);
    game.entity_mut(unit).remove::<Points>();
    let mut snapshot = Vec::new();
    game.snapshot(&mut snapshot);
    let mut target = Match::builder(&declared).role(Role::Rules).build();
    tracks(&mut target);
    let refused = SnapshotError::Invalid {
        ty: "progression.points",
        id: Some(id),
    };
    assert_eq!(target.restore_in_place(&snapshot), Err(refused));
}
