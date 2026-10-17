use campfire_capabilities::{Match, Release};
use campfire_codec::{Binary, Toml};
use campfire_declare::Book;
use campfire_math::{Num, Vec3};
use campfire_mode::Mode;
use campfire_progression::*;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, SimComponent, SimWorld};
use campfire_stats::Level;
use campfire_units::{UnitTypeCore, Units};
use campfire_values::{DeclaredName, Team, TrackId};
use serde::Serialize;

const fn track(at: u8) -> TrackId {
    TrackId::new(at)
}

/// `level`, levels 2 at 100 and 3 at 300; `valor`, level 2 at 50.
fn book() -> TrackBook {
    let data = |levels: &[i64], level| TrackData {
        levels: Thresholds::new(levels.iter().map(|&value| Num::int(value))).unwrap(),
        level,
    };
    let tracks = [
        ("level", data(&[100, 300], true)),
        ("valor", data(&[50], false)),
    ]
    .map(|(name, data)| (DeclaredName::new(name).unwrap(), data));
    TrackBook::new(&tracks.into()).unwrap()
}

#[test]
fn a_level_is_reached_exactly_at_its_threshold_and_never_falls() {
    let book = book();
    assert_eq!(book.level_track(), Some(track(0)));
    assert_eq!(book.names().named("valor"), Some(track(1)));
    assert_eq!(book.names().name(track(1)), Some("valor"));
    // A threshold met is a level: 99.99… is 1, 100 is 2, 299 is 2, 300 and anything past are 3.
    let level = |xp| book.level_at(track(0), xp).get();
    let below = Num::int(100) - Num::EPSILON;
    let levels = [below, Num::int(100), Num::int(299), Num::int(300), Num::MAX].map(level);
    assert_eq!(levels, [1, 2, 2, 3, 3]);
    assert_eq!(book.level_at(track(1), Num::int(50)).get(), 2);

    // Experience adds up to the largest number at most, and a level set past what it reaches
    // stays: a level is state. Each track holds its own level, the `level` track's too.
    let mut experience = Experience::new(TrackSet::from([track(0), track(1)]));
    let level_of = |experience: &Experience, at| experience.get(track(at)).unwrap().level.get();
    assert_eq!([0, 1].map(|at| level_of(&experience, at)), [1, 1]);
    let raised = experience.add(track(0), Num::int(150), &book);
    assert_eq!((raised.from.get(), raised.to.get()), (1, 2));
    let raised = experience.add(track(0), Num::MAX, &book);
    assert_eq!((raised.from.get(), raised.to.get()), (2, 3));
    assert_eq!(experience.get(track(0)).unwrap().xp, Num::MAX);
    assert_eq!([0, 1].map(|at| level_of(&experience, at)), [3, 1]);
    let mut ahead = Experience::new(TrackSet::from([track(1)]));
    ahead.tracks_mut()[0].level = Level::new(5).unwrap();
    let raised = ahead.add(track(1), Num::int(60), &book);
    assert_eq!((raised.from.get(), raised.to.get()), (5, 5));
    assert_eq!(ahead.get(track(0)), None);
}

#[test]
fn a_snapshot_with_tracks_out_of_order_or_negative_experience_fails_to_decode() {
    #[derive(Debug, Serialize)]
    struct Fields {
        tracks: Vec<TrackXp>,
    }
    let held = |at, xp| TrackXp {
        track: track(at),
        xp,
        level: Level::default(),
    };
    let decode = |tracks: Vec<TrackXp>| {
        let bytes = Binary::encode(&Fields { tracks });
        Binary::decode::<Experience>(&bytes)
    };
    assert!(decode(vec![held(0, Num::int(5)), held(3, Num::ZERO)]).is_ok());
    assert!(decode(vec![held(3, Num::int(5)), held(0, Num::int(5))]).is_err());
    assert!(decode(vec![held(1, Num::int(5)), held(1, Num::int(5))]).is_err());
    assert!(decode(vec![held(0, -Num::EPSILON)]).is_err());
}

#[test]
fn thresholds_are_positive_and_strictly_ascending_as_built_and_as_read() {
    let thresholds = |levels: &[i64]| Thresholds::new(levels.iter().map(|&value| Num::int(value)));
    for refused in [
        &[][..],
        &[0],
        &[-5, 100],
        &[100, 100],
        &[300, 100],
        &[100, 300, 200],
    ] {
        assert_eq!(thresholds(refused), None, "{refused:?}");
    }
    assert_eq!(thresholds(&[1]).unwrap().get(), [Num::int(1)]);
    assert_eq!(
        thresholds(&[100, 101, 300]).unwrap().get(),
        [Num::int(100), Num::int(101), Num::int(300)]
    );
    let read = |text: &str| Toml::parse::<TrackData>(text);
    let refusal = |text: &str| read(text).unwrap_err().message().to_owned();
    assert_eq!(
        read("levels = [\"0.5\", 300]\nlevel = true").unwrap(),
        TrackData {
            levels: Thresholds::new([Num::HALF, Num::int(300)]).unwrap(),
            level: true,
        }
    );
    for refused in ["levels = []", "levels = [0]", "levels = [300, 100]"] {
        assert!(
            refusal(refused).starts_with("a track has a level 2"),
            "{refused}"
        );
    }
    // An integer past what a number holds fails before the order is checked.
    assert!(
        refusal("levels = [9223372036854775807]")
            .starts_with("9223372036854775807 is past what a number holds")
    );
}

#[test]
fn experience_is_state_and_restores() {
    // A match of stats and progression with the tracks of `book`, the same in both, and a type
    // on both tracks and one on `valor` alone.
    let tracks = |game: &mut Match| {
        book().load_into(game);
        let core = UnitTypeCore::default();
        [[track(0), track(1)].as_slice(), &[track(1)]].map(|tracks| {
            let unit_type = Units::load_next_type(game, &core);
            Units::give_type(game, unit_type, TrackSet::from_iter(tracks.iter().copied()));
            unit_type
        })
    };
    let declared = [Capability::Stats, Capability::Progression];
    let mut game = Match::unscripted(&declared);
    let [on_both, on_valor] = tracks(&mut game);
    // A unit 150 into `level`, at level 2 with a point more than at its spawn, and 10 into
    // `valor`, still at level 1.
    let book = book();
    let both = TrackSet::from([track(0), track(1)]);
    let mut experience = Experience::new(both);
    let mut points = Points::at_spawn(both, book.level_track()).unwrap();
    experience.add(track(0), Num::int(150), &book);
    experience.add(track(1), Num::int(10), &book);
    let level = experience.get(track(0)).unwrap().level;
    points.gain(1);
    let id = Mode::spawn(
        &mut game,
        on_both,
        Vec3::ZERO,
        Team::new(0),
        (level, experience, points),
    );
    let mut restored = Match::unscripted(&declared);
    tracks(&mut restored);
    Release::restore_into(&game, &mut restored);
    assert_eq!(restored.part::<Experience>(id), game.part::<Experience>(id));
    assert_eq!(restored.part::<Level>(id).get(), 2);
    assert_eq!(restored.part::<Points>(id).get(), 2);
    // Only a unit with the `level` track has points: one with `valor` alone has none at its
    // spawn, and points on it fail the check.
    let valor = TrackSet::from([track(1)]);
    assert_eq!(Points::at_spawn(valor, book.level_track()), None);
    let held = Mode::spawn(
        &mut game,
        on_valor,
        Vec3::ZERO,
        Team::new(0),
        (Experience::new(valor), points),
    );
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
    let mut target = Match::unscripted(&declared);
    tracks(&mut target);
    assert!(target.restore_in_place(&snapshot).is_err());
}
