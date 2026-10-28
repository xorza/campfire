use std::collections::BTreeMap;

use campfire_capabilities::internals::StatsFixtures;
use campfire_capabilities::{Match, Release};
use campfire_codec::Toml;
use campfire_common::{DeclaredName, PlayerSlot};
use campfire_declarations::Role;
use campfire_declarations::internals::BookLoad;
use campfire_math::{Num, Vec3};
use campfire_players::{Players, PlayersWorld};
use campfire_progression::*;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, SimComponent, SimWorld, SnapshotError};
use campfire_stats::{
    Level, ModifierData, PlayerModifiers, StatBook, StatValue, Stats, StatsData, UnitStats,
};
use campfire_units::internals::Spawn;
use campfire_units::{TrackLevels, UnitTypeCore, Units};
use campfire_values::{NameId, PackageIndex, Stat, TrackId};

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
            let set = TrackSet::from_iter(tracks.iter().copied());
            Units::give_type(game, unit_type, TypeTracks::from(set));
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
    let mut points = Points::at_spawn(both, &book).unwrap();
    experience.add(track(0), Num::int(150), book.thresholds(track(0)));
    experience.add(track(1), Num::int(10), book.thresholds(track(1)));
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
        .add(track(1), Num::int(40), book.thresholds(track(1)));
    game.step();
    assert_eq!(shown(&game), [2, 2]);
    // Only a unit with the `level` track has points: one with `valor` alone has none at its
    // spawn, and points on it fail the check.
    let valor = TrackSet::from([track(1)]);
    assert_eq!(Points::at_spawn(valor, &book), None);
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

#[test]
fn a_unit_holds_the_modifier_of_its_level_alone_and_its_stats_follow() {
    // A match of stats and progression whose `level` track reaches level 2 at 100 and 3 at 300,
    // of the modifiers `veteran`, 10 more attack damage, at level 2, and `elite`, 20 more, at 3;
    // a type on it of base attack damage 50.
    let declared = [Capability::Stats, Capability::Progression];
    let mut game = Match::builder(&declared).role(Role::Rules).build();
    let damage = Stat::named("attack_damage").unwrap();
    let base = StatsData::from([(damage.clone(), StatValue::flat(Num::int(50)))]);
    let [unit_type] = Stats::load_typed_book(&mut game, &[base])[..] else {
        unreachable!("one type loaded")
    };
    // A test's modifiers load by name, as a package's.
    for (name, add) in [("elite", 20), ("veteran", 10)] {
        let data = Toml::parse::<ModifierData>(&format!(
            "stats = {{ attack_damage = {{ add = {add} }} }}"
        ))
        .unwrap();
        Stats::load_modifier(&mut game, PackageIndex::MODE, name, &data, None);
    }
    // Its levels give no point, so its unit holds none, as a level rises.
    let level = TrackData {
        points: Some(vec![0, 0, 0]),
        modifiers: ["veteran", "elite"]
            .map(|name| DeclaredName::new(name).unwrap())
            .to_vec(),
        ..TrackData::new(Thresholds::new([100, 300].map(Num::int)).unwrap(), true)
    };
    let tracks = BTreeMap::from([(DeclaredName::new("level").unwrap(), level)]);
    Progression::load_tracks(&mut game, &tracks);
    let set = TrackSet::from([track(0)]);
    Units::give_type(&mut game, unit_type, TypeTracks::from(set));
    let id = Spawn::of(unit_type, Vec3::ZERO)
        .parts(Experience::new(set))
        .place(&mut game);
    let [veteran, elite] =
        ["veteran", "elite"].map(|name| Stats::modifier(&game, PackageIndex::MODE, name).unwrap());
    let book = game.resource::<TrackBook>().clone();
    let attack_damage = |game: &Match| {
        let at = game.resource::<StatBook>().named(&damage).unwrap();
        game.part::<UnitStats>(id).values()[at.index()]
    };
    let gain = |game: &mut Match, amount| {
        game.part_mut::<Experience>(id)
            .add(track(0), Num::int(amount), book.thresholds(track(0)));
        game.step();
    };
    // Level 1 holds neither: 50. At 100, level 2, `veteran` alone: 60. At 300, level 3, `elite`
    // alone, from the unit itself: 50 + 20 = 70.
    game.step();
    assert_eq!(Stats::carried_modifiers(&game, id), []);
    assert_eq!(attack_damage(&game), Num::int(50));
    gain(&mut game, 100);
    assert_eq!(Stats::carried_modifiers(&game, id), [(veteran, Some(id))]);
    assert_eq!(attack_damage(&game), Num::int(60));
    gain(&mut game, 200);
    assert_eq!(Stats::carried_modifiers(&game, id), [(elite, Some(id))]);
    assert_eq!(attack_damage(&game), Num::int(70));
    assert!(game.get::<Points>(game.unit_entity(id)).is_none());
}

#[test]
fn a_unit_of_a_player_of_the_start_s_modifier_spawns_at_its_level() {
    // The `level` track of thresholds 100 and 300, and a type that starts at level 2 always and
    // at level 3 while its player holds `training`: player 0's unit spawns at the higher, 3, of
    // experience 300, not 100 and 300, its points 3 and its level 3; player 1's, of no modifier,
    // at 2, of experience 100.
    let declared = [Capability::Stats, Capability::Progression];
    let mut game = Match::builder(&declared).role(Role::Rules).build();
    let [unit_type] = Stats::load_typed_book(&mut game, &[StatsData::default()])[..] else {
        unreachable!("one type loaded")
    };
    Stats::load_modifier(
        &mut game,
        PackageIndex::MODE,
        "training",
        &ModifierData::default(),
        None,
    );
    let level = TrackData::new(Thresholds::new([100, 300].map(Num::int)).unwrap(), true);
    let tracks = BTreeMap::from([(DeclaredName::new("level").unwrap(), level)]);
    Progression::load_tracks(&mut game, &tracks);
    let entry = TypeTracksData(vec![TypeTrackData {
        starts: vec![
            StartData {
                level: Level::new(2).unwrap(),
                requires: None,
            },
            StartData {
                level: Level::new(3).unwrap(),
                requires: DeclaredName::new("training"),
            },
        ],
        ..TypeTrackData::from(DeclaredName::new("level").unwrap())
    }]);
    Progression::give_type_tracks(&mut game, unit_type, &entry);
    Players::spawn(&mut game, 2, 0);
    let training = Stats::modifier(&game, PackageIndex::MODE, "training").unwrap();
    for at in [0, 1] {
        let player = game.player_entity(PlayerSlot::new(at)).unwrap();
        let mut held = PlayerModifiers::default();
        if at == 0 {
            held.add(training);
        }
        game.entity_mut(player).insert(held);
    }
    let spawn = |game: &mut Match, at| {
        Spawn::of(unit_type, Vec3::ZERO)
            .owner(PlayerSlot::new(at))
            .place(game)
    };
    let [trained, plain] = [0, 1].map(|at| spawn(&mut game, at));
    let held = |game: &Match, id| {
        let xp = game.part::<Experience>(id).get(track(0)).unwrap();
        (
            xp.xp,
            xp.level.get(),
            game.part::<Points>(id).get(),
            game.part::<Level>(id).get(),
        )
    };
    assert_eq!(held(&game, trained), (Num::int(300), 3, 3, 3));
    assert_eq!(held(&game, plain), (Num::int(100), 2, 2, 2));
}
