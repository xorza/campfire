use campfire_math::Num;

use crate::track_data::{Thresholds, TrackData};
use crate::track_set::TrackSet;

use super::*;

fn name(text: &str) -> DeclaredName {
    DeclaredName::new(text).unwrap()
}

/// The tracks `rank`, a player's of level 2 at 50 whose levels give `points`, and `valor`, a
/// unit's of level 2 at 50.
fn tracks(points: [u32; 2]) -> TrackBook {
    let levels = || Thresholds::new([Num::int(50)]).unwrap();
    let rank = TrackData {
        holder: TrackHolder::Player,
        points: Some(points.to_vec()),
        ..TrackData::new(levels(), false)
    };
    let valor = TrackData::new(levels(), false);
    TrackBook::new(&[(name("rank"), rank), (name("valor"), valor)].into()).unwrap()
}

/// The mode's modifiers `aim`, `drill`, `free` and `march`.
fn modifiers() -> Names<ModifierId> {
    let mut names = Names::default();
    for modifier in ["aim", "drill", "free", "march"] {
        names.add((PackageIndex::MODE, modifier));
    }
    names
}

fn perk(cost: Option<u32>, modifier: &str) -> PerkData {
    PerkData {
        cost: cost.map(|cost| NonZeroU32::new(cost).unwrap()),
        requires: Vec::new(),
        track: None,
        level: None,
        modifier: name(modifier),
    }
}

/// `drill` of cost 1; `free` of no cost; `march` of cost 2, which requires `drill` and level 2
/// of `rank`.
fn perks() -> BTreeMap<DeclaredName, PerkData> {
    let march = PerkData {
        requires: vec![name("drill")],
        track: Some(name("rank")),
        level: Level::new(2),
        ..perk(Some(2), "march")
    };
    [
        (name("drill"), perk(Some(1), "drill")),
        (name("free"), perk(None, "free")),
        (name("march"), march),
    ]
    .into()
}

#[test]
fn a_perk_costs_its_points_once_its_player_has_its_level_and_its_requires() {
    let tracks = tracks([1, 2]);
    let book = PerkBook::new(&perks(), &tracks, &modifiers()).unwrap();
    let named = |text| book.names().named(text).unwrap();
    let [drill, free, march] = ["drill", "free", "march"].map(named);
    let rank = tracks.names().named("rank").unwrap();
    let at_one = Experience::new(TrackSet::from([rank]));
    let mut at_two = at_one.clone();
    at_two.add(rank, Num::int(50), tracks.thresholds(rank));
    // `march` with every need met: `drill`, 2 points and level 2.
    let met = Learner {
        perks: &[drill],
        points: Some(2),
        experience: Some(&at_two),
    };
    assert_eq!(book.learn_cost(march, met), Some(2));
    // Each need short alone drops the learn: level 1, no `drill`, 1 point, no points, and
    // `march` held already.
    let short = [
        Learner {
            experience: Some(&at_one),
            ..met
        },
        Learner { perks: &[], ..met },
        Learner {
            points: Some(1),
            ..met
        },
        Learner {
            points: None,
            ..met
        },
        Learner {
            perks: &[drill, march],
            ..met
        },
    ];
    for held in short {
        assert_eq!(book.learn_cost(march, held), None, "{held:?}");
    }
    // `drill` needs no level and no perk; `free`, of no cost, no player learns.
    let fresh = Learner {
        perks: &[],
        points: Some(1),
        experience: None,
    };
    assert_eq!(book.learn_cost(drill, fresh), Some(1));
    assert_eq!(book.learn_cost(free, fresh), None);
    // Each perk's modifier is the one of its name.
    let modifier = |text| modifiers().named((PackageIndex::MODE, text)).unwrap();
    assert_eq!(book.modifier(march), modifier("march"));
    assert_eq!(book.modifier(free), modifier("free"));
}

#[test]
fn a_perk_names_what_the_mode_has_and_a_level_of_a_player_track() {
    let entry = |perk: &str| BookPlace::Entry {
        table: ModePerksData::KEY,
        name: name(perk),
    };
    let rule = |error| BookError::rule(PackageIndex::MODE, entry("march"), error);
    let flawed = |march: PerkData, tracks: &TrackBook| {
        let mut perks = perks();
        perks.insert(name("march"), march);
        PerkBook::new(&perks, tracks, &modifiers()).err()
    };
    let paid = tracks([1, 2]);
    let march = || perks()[&name("march")].clone();
    // A `track` with no `level`, and a `level` with no `track`.
    let no_level = PerkData {
        level: None,
        ..march()
    };
    let no_track = PerkData {
        track: None,
        ..march()
    };
    for data in [no_level, no_track] {
        assert_eq!(flawed(data, &paid), Some(rule(PerkError::LevelTrack)));
    }
    // A level of a unit's track, and one past the player's track's 2.
    let valor = PerkData {
        track: Some(name("valor")),
        ..march()
    };
    assert_eq!(
        flawed(valor, &paid),
        Some(rule(PerkError::UnitTrack(name("valor"))))
    );
    let third = PerkData {
        level: Level::new(3),
        ..march()
    };
    assert_eq!(
        flawed(third, &paid),
        Some(rule(PerkError::Level {
            level: 3,
            levels: 2
        }))
    );
    // A cost where the player tracks give no point, `drill`'s the first.
    let unpaid = PerkBook::new(&perks(), &tracks([0, 0]), &modifiers()).err();
    let drill = BookError::rule(PackageIndex::MODE, entry("drill"), PerkError::NoPoints);
    assert_eq!(unpaid, Some(drill));
    // A perk, a track and a modifier the mode lacks.
    let unknown = |kind, text| {
        let at = entry("march");
        BookError::unknown(PackageIndex::MODE, at, UnknownName::new(kind, &name(text)))
    };
    let requires = PerkData {
        requires: vec![name("parade")],
        ..march()
    };
    let track = PerkData {
        track: Some(name("glory")),
        ..march()
    };
    let modifier = PerkData {
        modifier: name("parade"),
        ..march()
    };
    assert_eq!(
        flawed(requires, &paid),
        Some(unknown(PerkId::NAMES, "parade"))
    );
    assert_eq!(flawed(track, &paid), Some(unknown(TrackId::NAMES, "glory")));
    assert_eq!(
        flawed(modifier, &paid),
        Some(unknown(ModifierId::NAMES, "parade"))
    );
}
