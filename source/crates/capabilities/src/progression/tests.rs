use campfire_math::Num;
use serde::Serialize;

use super::*;
use crate::progression::experience::TrackXp;
use crate::progression::track_data::Thresholds;
use crate::progression::track_data::TrackData;
use crate::units::track_id::TrackId;
use crate::values::declared_name::DeclaredName;
fn track(at: usize) -> TrackId {
    TrackId::new(at).unwrap()
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
    TrackBook::new(&tracks.into())
}

#[test]
fn a_level_is_reached_exactly_at_its_threshold_and_never_falls() {
    let book = book();
    assert_eq!(book.level_track(), Some(track(0)));
    assert_eq!(book.named("valor"), Some(track(1)));
    assert_eq!(book.names().nth(1), Some("valor"));
    // A threshold met is a level: 99.99… is 1, 100 is 2, 299 is 2, 300 and anything past are 3.
    let level = |xp| book.level_at(track(0), xp).get();
    let below = Num::int(100) - Num::EPSILON;
    let levels = [below, Num::int(100), Num::int(299), Num::int(300), Num::MAX].map(level);
    assert_eq!(levels, [1, 2, 2, 3, 3]);
    assert_eq!(book.level_at(track(1), Num::int(50)).get(), 2);

    // Experience adds up to the largest number at most, and a level set past what it reaches
    // stays: a level is state. The `level` track's level is the unit's, which the add raises;
    // another track holds its own.
    let level_track = book.level_track();
    let mut experience = Experience::new(TrackSet::of([track(0), track(1)]), level_track);
    assert_eq!(experience.get(track(0)).unwrap().level, None);
    assert_eq!(
        experience.get(track(1)).unwrap().level,
        Some(Level::default())
    );
    let mut unit_level = Level::default();
    let raised = experience.add(track(0), Num::int(150), &book, Some(&mut unit_level));
    assert_eq!(
        (raised.from.get(), raised.to.get(), unit_level.get()),
        (1, 2, 2)
    );
    let raised = experience.add(track(0), Num::MAX, &book, Some(&mut unit_level));
    assert_eq!(
        (raised.from.get(), raised.to.get(), unit_level.get()),
        (2, 3, 3)
    );
    assert_eq!(experience.get(track(0)).unwrap().xp, Num::MAX);
    let mut ahead = Experience::new(TrackSet::of([track(1)]), level_track);
    ahead.tracks_mut()[0].level = Level::new(5);
    let raised = ahead.add(track(1), Num::int(60), &book, None);
    assert_eq!((raised.from.get(), raised.to.get()), (5, 5));
    assert_eq!(ahead.get(track(0)), None);
}

#[test]
fn a_snapshot_with_tracks_out_of_order_or_negative_experience_fails_to_decode() {
    #[derive(Serialize)]
    struct Fields {
        tracks: Vec<TrackXp>,
    }
    let held = |at, xp| TrackXp {
        track: track(at),
        xp,
        level: Some(Level::default()),
    };
    let decode = |tracks: Vec<TrackXp>| {
        let bytes = postcard::to_allocvec(&Fields { tracks }).unwrap();
        postcard::from_bytes::<Experience>(&bytes)
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
    let read = |text: &str| toml::from_str::<TrackData>(text);
    let refusal = |text: &str| read(text).unwrap_err().message().to_owned();
    assert_eq!(
        read("levels = [\"0.5\", 300]\nlevel = true").unwrap(),
        TrackData {
            levels: Thresholds::new([Num::ONE.checked_div_int(2).unwrap(), Num::int(300)]).unwrap(),
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
            .starts_with("a level's experience is beyond a number")
    );
}
