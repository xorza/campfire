use campfire_math::Num;
use serde::Serialize;

use super::*;
use crate::progression::experience::TrackXp;
use crate::progression::track_data::Thresholds;
use crate::progression::track_data::TrackData;
use crate::progression::track_id::TrackId;
use crate::values::declared_name::DeclaredName;

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn track(at: usize) -> TrackId {
    TrackId::new(at).unwrap()
}

/// `level`, levels 2 at 100 and 3 at 300; `valor`, level 2 at 50.
fn book() -> TrackBook {
    let data = |levels: &[i64], level| TrackData {
        levels: Thresholds::new(levels.iter().map(|&value| num(value))).unwrap(),
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
    assert_eq!(
        book.id(&DeclaredName::new("valor").unwrap()),
        Some(track(1))
    );
    // A threshold met is a level: 99.99… is 1, 100 is 2, 299 is 2, 300 and anything past are 3.
    let level = |xp| book.level_at(track(0), xp).get();
    let below = num(100) - Num::EPSILON;
    let levels = [below, num(100), num(299), num(300), Num::MAX].map(level);
    assert_eq!(levels, [1, 2, 2, 3, 3]);
    assert_eq!(book.level_at(track(1), num(50)).get(), 2);

    // Experience adds up to the largest number at most, and a level set past what it reaches
    // stays: a level is state.
    let mut experience = Experience::new(TrackSet::of([track(0), track(1)]));
    let raised = experience.add(track(0), num(150), &book);
    assert_eq!((raised.from.get(), raised.to.get()), (1, 2));
    let raised = experience.add(track(0), Num::MAX, &book);
    assert_eq!((raised.from.get(), raised.to.get()), (2, 3));
    assert_eq!(experience.get(track(0)).unwrap().xp, Num::MAX);
    let mut ahead = Experience::new(TrackSet::of([track(1)]));
    ahead.tracks_mut()[0].level = Level::new(5).unwrap();
    let raised = ahead.add(track(1), num(60), &book);
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
        level: Level::default(),
    };
    let decode = |tracks: Vec<TrackXp>| {
        let bytes = postcard::to_allocvec(&Fields { tracks }).unwrap();
        postcard::from_bytes::<Experience>(&bytes)
    };
    assert!(decode(vec![held(0, num(5)), held(3, Num::ZERO)]).is_ok());
    assert!(decode(vec![held(3, num(5)), held(0, num(5))]).is_err());
    assert!(decode(vec![held(1, num(5)), held(1, num(5))]).is_err());
    assert!(decode(vec![held(0, -Num::EPSILON)]).is_err());
}
