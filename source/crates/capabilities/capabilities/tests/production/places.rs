use std::num::NonZeroI64;

use campfire_airfields::{
    AirfieldData, AirfieldPlace, RunwayData, Runways, Sortie, SortieSpec, StrandedLoss,
};
use campfire_navigation::{FlightData, Navigation, WingsData};
use campfire_sim::MatchRate;
use campfire_sim::Position;
use std::num::NonZeroU16;

use campfire_actions::ActionRange;
use campfire_combat::internals::{Armed, TestWeapon};
use campfire_combat::{
    ClipRefill, ClipRule, ClipShots, Combat, DamageCause, OnDeath, Rate, WeaponDamage,
};
use campfire_script::MatchNames;
use campfire_stats::Stats;
use campfire_units::FlightGoal;
use campfire_values::{DamageKind, FilterData};

use super::*;

/// A point of a producer's frame.
fn frame(x: i64, z: i64) -> FramePoint {
    FramePoint::new([Num::int(x), Num::int(z)]).unwrap()
}

/// The shop's barracks as a producer of two places, at (0, 4) and (0, −4) of its frame; its grunt
/// a winged flier at 8 m that climbs a meter a tick, airborne past 3 m, of a sortie that taxis
/// half a meter a tick and waits 2 ticks at a runway's start.
fn places(game: &mut Match, shop: Shop, idle: Option<Ticks>) {
    let at = [4, -4].map(|z| PlaceData {
        at: frame(0, z),
        angle: Num::ZERO,
    });
    let production = ProducerSpec {
        queue: NonZeroU8::new(3).unwrap(),
        exit: None,
        rally: None,
        gathers: false,
        free: Box::new([]),
        places: Box::new(at),
    };
    Shop::regive(game, shop.barracks, production);
    let hz = NonZeroI64::from(game.resource::<MatchRate>().hz()).get();
    let wings = WingsData::new(Num::int(hz), Num::int(4)).unwrap();
    let flight = FlightData::new(Num::int(8), Num::int(hz), Num::int(3)).unwrap();
    Navigation::load_flight(game, shop.grunt, flight.winged(wings));
    let sortie = SortieSpec {
        pause: Ticks::new(2),
        taxi_step: Num::HALF,
        idle,
        stranded: None,
    };
    Shop::give(game, shop.grunt, sortie);
}

/// The places of `places` an airfield's, each waiting at (2, 4) and (2, −4) for its one runway
/// from (4, 0) to (14, 0).
fn airfield(game: &mut Match, shop: Shop, idle: Option<Ticks>) {
    places(game, shop, idle);
    let airfield = AirfieldData {
        places: [4, -4]
            .map(|z| AirfieldPlace {
                prep: frame(2, z),
                runway: 0,
            })
            .into(),
        runways: vec![RunwayData {
            start: frame(4, 0),
            end: frame(14, 0),
        }],
        heal: None,
    };
    Shop::give(game, shop.barracks, airfield);
}

/// A match of production, navigation and airfields.
fn sortie_match() -> (Match, Shop) {
    let mut game = shop_match_of(&[
        Capability::Stats,
        Capability::Production,
        Capability::Navigation,
        Capability::Airfields,
    ]);
    let shop = Shop::place(&mut game);
    (game, shop)
}

/// A match of production, navigation and airfields, with the shop's barracks an airfield whose
/// grunt goes home after `idle` ticks idle in the air.
fn idle_airfield_match(idle: Option<Ticks>) -> (Match, Shop) {
    let (mut game, shop) = sortie_match();
    airfield(&mut game, shop, idle);
    (game, shop)
}

/// A match of production, navigation and airfields, with the shop's barracks an airfield.
fn airfield_match() -> (Match, Shop) {
    idle_airfield_match(None)
}

#[test]
fn a_producer_of_places_makes_each_unit_at_a_free_one_and_its_next_waits_for_one() {
    // Three trains of no time at the barracks of two places, at x = 10: the first two are made in
    // tick 0 at the places, (10, 0, 4) and (10, 0, −4), and the third waits until the first dies
    // after tick 2, then is made at its place in tick 3 (04-capabilities/production.md, A jet holds
    // a place).
    let (mut game, shop) = airfield_match();
    let train = Production::add_train_at_place(&mut game, shop.grunt, Ticks::ZERO);
    let barracks = Production::allocate_id(&mut game);
    shop.producer(
        &mut game,
        barracks,
        10,
        Team::new(0),
        None,
        train,
        queue(train, &[0, 0, 0]),
    );
    let places = |game: &mut Match| {
        let spawned = Shop::spawned(game, shop.grunt);
        spawned
            .iter()
            .map(|spawn| (spawn.id, spawn.position))
            .collect::<Vec<_>>()
    };
    let at = |z: i32| Vec3::from((10, 0, z));
    for _ in 0..3 {
        game.step();
    }
    let made = places(&mut game);
    let [(first, _), (second, _)] = made[..] else {
        panic!("two units made: {made:?}");
    };
    assert_eq!(made, [(first, at(4)), (second, at(-4))]);
    let held = |game: &Match| game.part::<HeldPlaces>(barracks).clone();
    assert_eq!(
        (held(&game).holder(0), held(&game).holder(1)),
        (Some(first), Some(second))
    );
    assert_eq!(
        game.part::<MadeAt>(first),
        &MadeAt {
            producer: barracks,
            place: 0
        }
    );
    game.insert_parts(first, Dead);
    game.step();
    let made = places(&mut game);
    assert_eq!(made.len(), 3);
    assert_eq!(made[2].1, at(4));
    assert_eq!(held(&game).holder(0), Some(made[2].0));
}

#[test]
fn a_jet_of_a_producer_of_no_runways_flies_from_its_place() {
    // The barracks of two places but no airfield makes a jet at (10, 0, 4) in tick 0; with no
    // runway to take off down, it is not parked but climbs its meter a tick from there, and keeps
    // its place, as its producer lives (04-capabilities/production.md, A jet holds a place).
    let (mut game, shop) = sortie_match();
    places(&mut game, shop, None);
    let train = Production::add_train_at_place(&mut game, shop.grunt, Ticks::ZERO);
    let barracks = Production::allocate_id(&mut game);
    shop.producer(
        &mut game,
        barracks,
        10,
        Team::new(0),
        None,
        train,
        queue(train, &[0]),
    );
    game.step();
    let jet = Shop::spawned(&mut game, shop.grunt)[0].id;
    for _ in 0..3 {
        game.step();
    }
    // Idle in the air since it was made, as no order holds it.
    assert_eq!(
        game.part::<Sortie>(jet),
        &Sortie::Flying {
            idle_since: Some(Tick::new(1))
        }
    );
    assert_eq!(game.part::<FlightGoal>(jet), &FlightGoal::Height);
    assert_eq!(game.part::<Position>(jet).get().y, Num::int(3));
    assert_eq!(game.part::<HeldPlaces>(barracks).holder(0), Some(jet));
    assert_eq!(
        game.part::<MadeAt>(jet),
        &MadeAt {
            producer: barracks,
            place: 0
        }
    );
}

#[test]
fn jets_take_off_down_their_runway_one_at_a_time() {
    // Two jets made in tick 0 at the places (10, 0, 4) and (10, 0, −4), both ordered away in
    // tick 1. The first takes the runway: it taxis half a meter a tick to its prep point, 2 m
    // off, in 4 ticks, then to the runway's start at (14, 0, 0), √20 ≈ 4.47 m off, in 9; waits 2
    // ticks; rolls a meter a tick to the end at (24, 0, 0) in 10; and climbs from tick 27. The
    // second waits at its place until the first lets the runway go in tick 27, then goes as it
    // did, 26 ticks later (04-capabilities/airfields.md, A jet takes off and lands on its runway).
    let (mut game, shop) = airfield_match();
    let train = Production::add_train_at_place(&mut game, shop.grunt, Ticks::ZERO);
    let barracks = Production::allocate_id(&mut game);
    shop.producer(
        &mut game,
        barracks,
        10,
        Team::new(0),
        None,
        train,
        queue(train, &[0, 0]),
    );
    game.step();
    let jets: Vec<StableId> = Shop::spawned(&mut game, shop.grunt)
        .iter()
        .map(|spawn| spawn.id)
        .collect();
    for &jet in &jets {
        assert_eq!(game.part::<Sortie>(jet), &Sortie::Parked);
        assert_eq!(game.part::<FlightGoal>(jet), &FlightGoal::Ground);
        *game.part_mut::<Destination>(jet) = Destination::to(Some(Vec3::from((80, 0, 0))));
    }
    let mut phases = vec![Vec::new(); jets.len()];
    let mut places = vec![Vec::new(); jets.len()];
    for tick in 1..60 {
        game.step();
        for (at, &jet) in jets.iter().enumerate() {
            let phase = mem::discriminant(game.part::<Sortie>(jet));
            if phases[at].last().is_none_or(|&(_, last)| last != phase) {
                phases[at].push((tick, phase));
            }
            places[at].push(game.part::<Position>(jet).get());
        }
    }
    let ticks = |at: usize| phases[at].iter().map(|&(tick, _)| tick).collect::<Vec<_>>();
    // Waiting, to the prep point, to the start, the pause, the roll, and the air.
    assert_eq!(ticks(0), [1, 2, 6, 15, 17, 27]);
    assert_eq!(ticks(1), [1, 27, 31, 40, 42, 52]);
    // The first stands at the runway's start in tick 14, at its end on the ground in tick 26,
    // and a meter up in tick 27; the second stands at its place until then.
    let place = |at: usize, tick: usize| places[at][tick - 1];
    assert_eq!(place(0, 14), Vec3::from((14, 0, 0)));
    assert_eq!(place(0, 26), Vec3::from((24, 0, 0)));
    assert_eq!(place(0, 27), Vec3::from((25, 1, 0)));
    assert_eq!(place(1, 26), Vec3::from((10, 0, -4)));
    assert_eq!(game.part::<Runways>(barracks).holder(0), None);
}

#[test]
fn a_jet_that_dies_on_its_runway_lets_it_go() {
    // The two jets of `jets_take_off_down_their_runway_one_at_a_time`; the first dies as tick 20
    // starts, as it rolls, holding the runway, so the second takes it in tick 20, 18 ticks after
    // the first took it in tick 2, and goes as the first did, 18 ticks later, where it took the
    // runway in tick 27 as the first lived (04-capabilities/airfields.md, A jet takes off and lands
    // on its runway).
    let (mut game, shop) = airfield_match();
    let train = Production::add_train_at_place(&mut game, shop.grunt, Ticks::ZERO);
    let barracks = Production::allocate_id(&mut game);
    shop.producer(
        &mut game,
        barracks,
        10,
        Team::new(0),
        None,
        train,
        queue(train, &[0, 0]),
    );
    game.step();
    let jets: Vec<StableId> = Shop::spawned(&mut game, shop.grunt)
        .iter()
        .map(|spawn| spawn.id)
        .collect();
    let [first, second] = jets[..] else {
        panic!("two jets made: {jets:?}");
    };
    for jet in [first, second] {
        *game.part_mut::<Destination>(jet) = Destination::to(Some(Vec3::from((80, 0, 0))));
    }
    for _ in 1..20 {
        game.step();
    }
    assert_eq!(game.part::<Sortie>(first), &Sortie::Roll);
    assert_eq!(game.part::<Runways>(barracks).holder(0), Some(first));
    game.insert_parts(first, Dead);
    let (phases, _) = phases(&mut game, second, 40);
    let ticks: Vec<u64> = phases.iter().map(|&(tick, _)| tick + 19).collect();
    // To the prep point, to the start, the pause, the roll, and the air.
    assert_eq!(ticks, [20, 24, 33, 35, 45]);
    assert_eq!(phases[0].1, Sortie::ToPrep);
}

/// The first tick of each of the phases `jet` of `game` steps through in `ticks` ticks from the
/// next, with the phase, and its place at each tick, the first at index 0.
fn phases(game: &mut Match, jet: StableId, ticks: u64) -> (Vec<(u64, Sortie)>, Vec<Vec3>) {
    let mut phases: Vec<(u64, Sortie)> = Vec::new();
    let mut places = Vec::new();
    for tick in 1..=ticks {
        game.step();
        let sortie = *game.part::<Sortie>(jet);
        let new = phases
            .last()
            .is_none_or(|&(_, last)| mem::discriminant(&last) != mem::discriminant(&sortie));
        if new {
            phases.push((tick, sortie));
        }
        places.push(game.part::<Position>(jet).get());
    }
    (phases, places)
}

#[test]
fn an_idle_jet_flies_home_lands_and_taxis_back_to_its_place() {
    // A jet sent to (40, 0, 0) takes off as the first of `jets_take_off_down_their_runway_one_at_a_
    // time` does, arrives within its 4 m of its goal in tick 39, stands idle in the air its 10
    // ticks and goes home in tick 49: to its approach past the runway's end, as far as it flies
    // while it sinks its 8 m at a meter a tick, (32, 0, 0). It arrives and takes the runway in tick
    // 57, sinks to the runway's end, on the ground there in tick 69, rolls to its start in tick 79,
    // lets it go and taxis back to its place, parked in tick 93 (04-capabilities/airfields.md, A
    // jet goes home).
    let (mut game, shop) = idle_airfield_match(Some(Ticks::new(10)));
    let train = Production::add_train_at_place(&mut game, shop.grunt, Ticks::ZERO);
    let barracks = Production::allocate_id(&mut game);
    shop.producer(
        &mut game,
        barracks,
        10,
        Team::new(0),
        None,
        train,
        queue(train, &[0]),
    );
    game.step();
    let jet = Shop::spawned(&mut game, shop.grunt)[0].id;
    *game.part_mut::<Destination>(jet) = Destination::to(Some(Vec3::from((40, 0, 0))));
    let (phases, places) = phases(&mut game, jet, 100);
    let ticks: Vec<u64> = phases.iter().map(|&(tick, _)| tick).collect();
    assert_eq!(ticks, [1, 2, 6, 15, 17, 27, 49, 57, 58, 70, 80, 89, 93]);
    assert_eq!(phases[6].1, Sortie::Home);
    assert_eq!(phases[12].1, Sortie::Parked);
    let place = |tick: usize| places[tick - 1];
    assert_eq!(place(69), Vec3::from((24, 0, 0)));
    assert_eq!(place(79), Vec3::from((14, 0, 0)));
    assert_eq!(place(93), Vec3::from((10, 0, 4)));
    assert_eq!(game.part::<Destination>(jet).get(), None);
    assert_eq!(game.part::<Runways>(barracks).holder(0), None);
    assert_eq!(game.part::<FlightGoal>(jet), &FlightGoal::Ground);
}

#[test]
fn an_empty_jet_flies_home_and_reloads_its_clip_at_its_place() {
    // A jet of a clip of 2 that refills at home in 20 ticks, ordered to attack a dummy at (50, 0,
    // 0): it takes off as an idle one does, fires its 2 shots, for 5 each, in ticks 47 and 50, and
    // goes home empty in tick 52, its order dropped. It lands and taxis back to its place by tick
    // 100, and reloads both shots in 20 ticks, its first back after 10, half the time, its second
    // at the end, then stands parked, as no order takes it away (04-capabilities/airfields.md, A
    // jet goes home; 04-capabilities/combat.md, A clip may reload at home).
    let mut game = shop_match_of(&[
        Capability::Stats,
        Capability::Combat,
        Capability::Orders,
        Capability::Production,
        Capability::Navigation,
        Capability::Airfields,
    ]);
    Combat::load_weapon_stats(&mut game);
    let shop = Shop::place(&mut game);
    airfield(&mut game, shop, None);
    let aim = game
        .resource::<MatchNames>()
        .resolve_filter(&FilterData::parse("enemies").unwrap())
        .unwrap();
    let weapon = TestWeapon {
        rate: Rate::Period(Box::new([Ticks::new(3)])),
        damage: WeaponDamage::Amount(Box::new([Num::int(5)])),
        aim_deg: Some(Num::int(30)),
        clip: Some(ClipRule {
            shots: NonZeroU16::new(2).unwrap(),
            reload: Ticks::new(20),
            refill: ClipRefill::Home,
        }),
        ..TestWeapon::new(aim, ActionRange::meters(Num::int(6)), Ticks::new(1))
    };
    let action = Combat::add_weapon(&mut game, weapon);
    let slots = ActionSlots::new([(action, SlotKind::new(0), Rank::new(1))]);
    Shop::give(&mut game, shop.grunt, slots);
    let dummy = Armed {
        life: Num::int(100),
        on_death: OnDeath::Despawn,
        arms: None,
    }
    .load(&mut game);
    let train = Production::add_train_at_place(&mut game, shop.grunt, Ticks::ZERO);
    let barracks = Production::allocate_id(&mut game);
    shop.producer(
        &mut game,
        barracks,
        10,
        Team::new(0),
        None,
        train,
        queue(train, &[0]),
    );
    let target = Spawn::of(dummy, (50, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    game.step();
    let jet = Shop::spawned(&mut game, shop.grunt)[0].id;
    Combat::attack(&mut game, jet, target);
    let mut fired = Vec::new();
    let mut phases: Vec<(u64, Sortie)> = Vec::new();
    for tick in 1..140 {
        game.step();
        let sortie = *game.part::<Sortie>(jet);
        let new = phases
            .last()
            .is_none_or(|&(_, last)| mem::discriminant(&last) != mem::discriminant(&sortie));
        if new {
            phases.push((tick, sortie));
        }
        fired.push(game.part::<ClipShots>(jet).fired(action));
    }
    let ticks: Vec<u64> = phases.iter().map(|&(tick, _)| tick).collect();
    assert_eq!(
        ticks,
        [1, 2, 6, 15, 17, 27, 52, 64, 65, 77, 87, 96, 100, 120]
    );
    assert_eq!(
        phases[12].1,
        Sortie::Reload {
            from: Tick::new(100),
            until: Tick::new(120)
        }
    );
    let fired_at = |tick: usize| fired[tick - 1];
    assert_eq!(
        [47, 48, 51, 109, 110, 119, 120].map(fired_at),
        [0, 1, 2, 2, 1, 1, 0]
    );
    assert_eq!(Stats::health(&game, target), 90);
    assert_eq!(game.part::<Sortie>(jet), &Sortie::Parked);
}

#[test]
fn a_jet_whose_airfield_is_gone_goes_home_to_another_of_its_team() {
    // A jet of the airfield at x = 10 takes off as an idle one does; its airfield dies in tick 30,
    // so it takes the free place 0 of its team's other airfield, at x = −30, and, as it goes home
    // after its idle time, lands and parks there (04-capabilities/airfields.md, A jet goes home).
    let (mut game, shop) = idle_airfield_match(Some(Ticks::new(10)));
    let train = Production::add_train_at_place(&mut game, shop.grunt, Ticks::ZERO);
    let [first, second] = [(); 2].map(|()| Production::allocate_id(&mut game));
    shop.producer(
        &mut game,
        first,
        10,
        Team::new(0),
        None,
        train,
        queue(train, &[0]),
    );
    shop.producer(
        &mut game,
        second,
        -30,
        Team::new(0),
        None,
        train,
        queue(train, &[]),
    );
    game.step();
    let jet = Shop::spawned(&mut game, shop.grunt)[0].id;
    *game.part_mut::<Destination>(jet) = Destination::to(Some(Vec3::from((40, 0, 0))));
    for _ in 1..30 {
        game.step();
    }
    game.insert_parts(first, Dead);
    let (phases, _) = phases(&mut game, jet, 370);
    let tick = |phase: Sortie| {
        let found = phases
            .iter()
            .find(|&&(_, found)| mem::discriminant(&found) == mem::discriminant(&phase));
        found.map(|&(tick, _)| tick + 29)
    };
    // It goes home in tick 49 as the idle jet does, and from the runway's end to its place takes
    // the idle jet's 23 ticks, from tick 70 to 93, as both airfields' frames are alike.
    assert_eq!(tick(Sortie::Home), Some(49));
    let landed = tick(Sortie::Landing).expect("the jet lands");
    assert_eq!(tick(Sortie::Parked), Some(landed + 23));
    assert_eq!(
        game.part::<MadeAt>(jet),
        &MadeAt {
            producer: second,
            place: 0
        }
    );
    assert_eq!(game.part::<HeldPlaces>(second).holder(0), Some(jet));
    assert_eq!(game.part::<Position>(jet).get(), Vec3::from((-30, 0, 4)));
}

/// A match of stats, combat, orders, production, navigation and airfields, with the shop's
/// barracks an airfield that heals `heal` life a tick, and its grunt a jet of twice a second's
/// ticks of life, 60 at 30 ticks a second, that goes home after `idle` ticks idle and stranded
/// loses half its life a second, 1 a tick; and that life.
fn kept_airfield_match(heal: Option<i64>, idle: Option<Ticks>) -> (Match, Shop, i64) {
    let mut game = shop_match_of(&[
        Capability::Stats,
        Capability::Combat,
        Capability::Orders,
        Capability::Production,
        Capability::Navigation,
        Capability::Airfields,
    ]);
    Combat::load_weapon_stats(&mut game);
    let shop = Shop::place(&mut game);
    airfield(&mut game, shop, idle);
    let hz = NonZeroI64::from(game.resource::<MatchRate>().hz()).get();
    let mut airfield = game
        .resource::<ById<UnitType, AirfieldData>>()
        .get(shop.barracks)
        .unwrap()
        .clone();
    airfield.heal = heal.map(|heal| Num::int(heal * hz));
    Shop::regive(&mut game, shop.barracks, airfield);
    let mut sortie = *game
        .resource::<ById<UnitType, SortieSpec>>()
        .get(shop.grunt)
        .unwrap();
    sortie.stranded = Some(StrandedLoss {
        share: Num::HALF,
        kind: DamageKind::new(0),
    });
    Shop::regive(&mut game, shop.grunt, sortie);
    Armed {
        life: Num::int(2 * hz),
        on_death: OnDeath::Despawn,
        arms: None,
    }
    .give(&mut game, shop.grunt);
    (game, shop, 2 * hz)
}

#[test]
fn an_airfield_heals_the_jets_that_rest_at_it_and_kills_them_as_it_dies() {
    // Two jets parked at an airfield that heals 2 life a tick. The first, dealt 9 of its 60 in tick
    // 1, heals from tick 2, as the airfield sees its loss the tick after, to 53, 55, 57, 59 and its
    // whole 60 in tick 6, then no more. The second, ordered away in tick 1, is in the air by tick
    // 27; the airfield dies after tick 30, and kills the first, parked, in tick 31, which the
    // deaths find in tick 32, but not the second, which flies (04-capabilities/airfields.md, A jet
    // goes home).
    let (mut game, shop, life) = kept_airfield_match(Some(2), None);
    let train = Production::add_train_at_place(&mut game, shop.grunt, Ticks::ZERO);
    let barracks = Production::allocate_id(&mut game);
    shop.producer(
        &mut game,
        barracks,
        10,
        Team::new(0),
        None,
        train,
        queue(train, &[0, 0]),
    );
    game.step();
    let jets: Vec<StableId> = Shop::spawned(&mut game, shop.grunt)
        .iter()
        .map(|spawn| spawn.id)
        .collect();
    let [first, second] = jets[..] else {
        panic!("two jets made: {jets:?}");
    };
    assert_eq!(Stats::health(&game, first), life);
    Combat::damage(&mut game, None, first, 9, DamageCause::Effect);
    *game.part_mut::<Destination>(second) = Destination::to(Some(Vec3::from((80, 0, 0))));
    let mut healths = Vec::new();
    for _ in 1..=7 {
        game.step();
        healths.push(Stats::health(&game, first));
    }
    assert_eq!(healths, [51, 53, 55, 57, 59, 60, 60]);
    for _ in 8..=30 {
        game.step();
    }
    assert_eq!(game.part::<Sortie>(first), &Sortie::Parked);
    assert!(game.part::<Sortie>(second).flies());
    game.insert_parts(barracks, Dead);
    game.step();
    assert_eq!(Stats::health(&game, first), 0);
    game.step();
    assert!(game.try_part::<Dead>(first).is_some());
    assert!(game.try_part::<Dead>(second).is_none());
    assert_eq!(Stats::health(&game, second), life);
}

#[test]
fn a_stranded_jet_circles_where_it_stands_and_loses_its_life_until_an_airfield_takes_it() {
    // A jet of an airfield that dies in tick 30, idle in the air from tick 39, goes home in tick 49
    // to no airfield: stranded, it circles where it stands, losing 1 of its 60 a tick, 59 at the
    // end of tick 49 and 48 at the end of tick 60. Its team's airfield at x = −30, spawned then,
    // takes it in tick 61, and it goes home there, losing no more (04-capabilities/airfields.md, A
    // jet goes home).
    let (mut game, shop, _) = kept_airfield_match(None, Some(Ticks::new(10)));
    let train = Production::add_train_at_place(&mut game, shop.grunt, Ticks::ZERO);
    let [first, second] = [(); 2].map(|()| Production::allocate_id(&mut game));
    shop.producer(
        &mut game,
        first,
        10,
        Team::new(0),
        None,
        train,
        queue(train, &[0]),
    );
    game.step();
    let jet = Shop::spawned(&mut game, shop.grunt)[0].id;
    *game.part_mut::<Destination>(jet) = Destination::to(Some(Vec3::from((40, 0, 0))));
    for _ in 1..30 {
        game.step();
    }
    game.insert_parts(first, Dead);
    for _ in 30..49 {
        game.step();
    }
    assert!(matches!(game.part::<Sortie>(jet), Sortie::Flying { .. }));
    game.step();
    assert!(matches!(game.part::<Sortie>(jet), Sortie::Stranded { .. }));
    assert_eq!(Stats::health(&game, jet), 59);
    for _ in 50..=60 {
        game.step();
    }
    assert!(matches!(game.part::<Sortie>(jet), Sortie::Stranded { .. }));
    assert_eq!(Stats::health(&game, jet), 48);
    shop.producer(
        &mut game,
        second,
        -30,
        Team::new(0),
        None,
        train,
        queue(train, &[]),
    );
    game.step();
    assert_eq!(game.part::<Sortie>(jet), &Sortie::Home);
    assert_eq!(
        game.part::<MadeAt>(jet),
        &MadeAt {
            producer: second,
            place: 0
        }
    );
    for _ in 0..5 {
        game.step();
    }
    assert_eq!(Stats::health(&game, jet), 48);
}

/// The jet `shop`'s barracks at x = 10 of `game` makes at its first place, after tick 0.
fn parked_jet(game: &mut Match, shop: Shop) -> StableId {
    let train = Production::add_train_at_place(game, shop.grunt, Ticks::ZERO);
    let barracks = Production::allocate_id(game);
    shop.producer(
        game,
        barracks,
        10,
        Team::new(0),
        None,
        train,
        queue(train, &[0]),
    );
    game.step();
    Shop::spawned(game, shop.grunt)[0].id
}

#[test]
fn an_order_takes_a_jet_off_its_way_home_and_waits_while_it_lands() {
    // The idle jet of `an_idle_jet_flies_home_lands_and_taxis_back_to_its_place` goes home in tick
    // 49; ordered to (80, 0, 0) in tick 50, it takes the order and flies again, as it still holds
    // its shots.
    let (mut game, shop) = idle_airfield_match(Some(Ticks::new(10)));
    let jet = parked_jet(&mut game, shop);
    let to = |x: i32| Destination::to(Some(Vec3::from((x, 0, 0))));
    *game.part_mut::<Destination>(jet) = to(40);
    for _ in 1..=49 {
        game.step();
    }
    assert_eq!(game.part::<Sortie>(jet), &Sortie::Home);
    *game.part_mut::<Destination>(jet) = to(80);
    game.step();
    assert_eq!(game.part::<Sortie>(jet), &Sortie::FLYING);
    assert_eq!(
        game.part::<Destination>(jet).get(),
        Some(Vec3::from((80, 0, 0)))
    );
    // A second jet, home at the same ticks and ordered to (80, 0, 0) in tick 60 as it sinks to
    // the runway, holds the order as it lands, taxis back and parks in tick 93, as the idle one
    // does, then goes for its runway in tick 94 and takes off with it as the first of
    // `jets_take_off_down_their_runway_one_at_a_time` does, 93 ticks later.
    let (mut game, shop) = idle_airfield_match(Some(Ticks::new(10)));
    let jet = parked_jet(&mut game, shop);
    *game.part_mut::<Destination>(jet) = to(40);
    for _ in 1..60 {
        game.step();
    }
    assert_eq!(game.part::<Sortie>(jet), &Sortie::Descent);
    *game.part_mut::<Destination>(jet) = to(80);
    let (phases, _) = phases(&mut game, jet, 50);
    let ticks: Vec<u64> = phases.iter().map(|&(tick, _)| tick + 59).collect();
    assert_eq!(ticks, [60, 70, 80, 89, 93, 94, 95, 99, 108]);
    assert_eq!(phases[4].1, Sortie::Parked);
    assert_eq!(phases[5].1, Sortie::Waiting);
    assert_eq!(
        game.part::<Destination>(jet).get(),
        Some(Vec3::from((80, 0, 0)))
    );
}
