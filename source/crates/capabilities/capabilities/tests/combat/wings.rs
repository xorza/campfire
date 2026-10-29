use campfire_navigation::WingsData;
use campfire_units::{Facing, MoveStep, TurnStep};

use super::*;

/// A match of the capabilities a winged attacker needs: its stats and weapons, the orders that
/// chase its target, and the navigation that flies it.
fn wings_match() -> Match {
    let mut game = Match::builder(&[
        Capability::Stats,
        Capability::Combat,
        Capability::Orders,
        Capability::Navigation,
    ])
    .build();
    Combat::load_weapon_stats(&mut game);
    game
}

/// A winged type of `weapon` in its one slot, of a flight at 8 m whose wings circle 4 m round the
/// place they hold.
fn jet_type(game: &mut Match, weapon: TestWeapon) -> UnitType {
    let action = Combat::add_weapon(game, weapon);
    let slots = ActionSlots::new([(action, SlotKind::new(0), Rank::new(1))]);
    let jet = wielder(game, slots, &[]);
    let hz = NonZeroI64::from(game.resource::<MatchRate>().hz()).get();
    let wings = WingsData::new(Num::int(hz), Num::int(4)).unwrap();
    let flight = FlightData::new(Num::int(8), Num::int(hz), Num::int(3)).unwrap();
    Navigation::load_flight(game, jet, flight.winged(wings));
    jet
}

/// A unit of the winged type `jet`, a meter's step a tick, turning 15° a tick, facing `x` at
/// (0, 8, 0), in the air from its spawn.
fn jet(game: &mut Match, jet: UnitType) -> StableId {
    let step = MoveStep::new(Num::ONE).unwrap();
    let turn = TurnStep::new(Num::int(15)).unwrap();
    Spawn::of(jet, (0, 8, 0))
        .parts((Navigation::walker(step), turn))
        .place(game)
}

/// A weapon of the enemies, of 6 m and a windup of a tick, every 10 ticks, of 5 damage, that
/// starts within 30° of its target's bearing.
fn rockets(game: &Match) -> TestWeapon {
    let aim = game
        .resource::<MatchNames>()
        .resolve_filter(&FilterData::parse("enemies").unwrap())
        .unwrap();
    TestWeapon {
        rate: Rate::Period(Box::new([Ticks::new(10)])),
        damage: WeaponDamage::Amount(Box::new([Num::int(5)])),
        aim_deg: Some(Num::int(30)),
        ..TestWeapon::new(aim, ActionRange::meters(Num::int(6)), Ticks::new(1))
    }
}

#[test]
fn a_winged_attacker_aims_by_flying_at_its_target_and_comes_round_for_the_next() {
    // The jet flies along x at the dummy at (20, 0, 0), a meter a tick from tick 0: 6 m from it,
    // straight at it, it starts in tick 14 and strikes for 5. It flies on over it and turns its 15°
    // a tick round, as it never turns in place, and comes back: in tick 41, at (17.46, −1.47)
    // facing 300°, the dummy bears 330°, 30° off, within its aim, and it starts again; through its
    // turn its facing is never the bearing (04-capabilities/combat.md, A jet aims by flying).
    let mut game = wings_match();
    let weapon = rockets(&game);
    let jet_type = jet_type(&mut game, weapon);
    let [dummy] = load_types(&mut game, [dummy()]);
    let id = jet(&mut game, jet_type);
    let target = Spawn::of(dummy, (20, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    Combat::attack(&mut game, id, target);
    let mut facings = vec![Num::ZERO];
    let mut starts = Vec::new();
    for _ in 0..60 {
        game.step();
        facings.push(game.part::<Facing>(id).degrees());
        let started = Attack::of(&game, id).started().map(Tick::get);
        if started.is_some() && starts.last() != started.as_ref() {
            starts.extend(started);
        }
    }
    assert_eq!(starts, [14, 41]);
    assert_eq!(facings[41], Num::int(300));
    assert_eq!(Stats::health(&game, target), 90);
    for tick in 1..facings.len() {
        let turned = Facing::of(facings[tick - 1]).apart(Facing::of(facings[tick]));
        assert!(turned <= Num::int(15), "tick {tick}: {turned}");
    }
}

#[test]
fn a_clip_that_refills_at_home_starts_nothing_once_empty_until_its_home_refills_it() {
    // A clip of 2 that reloads in 21 ticks at home, every 3 ticks: it starts in ticks 0 and 3,
    // then stays empty well past its reload's time; refilled by its home's contract in tick 40,
    // it starts again in tick 40 (04-capabilities/combat.md, A clip may reload at home).
    let mut game = fight_match();
    let aim = game
        .resource::<MatchNames>()
        .resolve_filter(&FilterData::parse("enemies").unwrap())
        .unwrap();
    let weapon = TestWeapon {
        rate: Rate::Period(Box::new([Ticks::new(3)])),
        damage: WeaponDamage::Amount(Box::new([Num::int(5)])),
        clip: Some(ClipRule {
            shots: NonZeroU16::new(2).unwrap(),
            reload: Ticks::new(21),
            refill: ClipRefill::Home,
        }),
        ..TestWeapon::new(aim, ActionRange::meters(Num::int(5)), Ticks::new(1))
    };
    let action = Combat::add_weapon(&mut game, weapon);
    let slots = ActionSlots::new([(action, SlotKind::new(0), Rank::new(1))]);
    let shooter = wielder(&mut game, slots.clone(), &[]);
    let [dummy] = load_types(&mut game, [dummy()]);
    let id = Spawn::of(shooter, (0, 0, 0)).place(&mut game);
    let target = Spawn::of(dummy, (2, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    Combat::attack(&mut game, id, target);
    let mut starts = Vec::new();
    for tick in 0..50 {
        if tick == 40 {
            // Its home refills its clip whole, as a reload that ran its time does.
            let weapons = game.resource::<ById<ActionId, Weapon>>().clone();
            let reload = HomeReload {
                passed: Ticks::new(1),
                total: Ticks::new(1),
            };
            game.part_mut::<ClipShots>(id)
                .reload_at_home(&slots, &weapons, reload);
        }
        game.step();
        let started = Attack::of(&game, id).started().map(Tick::get);
        if started.is_some() && starts.last() != started.as_ref() {
            starts.extend(started);
        }
        if (4..40).contains(&tick) {
            assert_eq!(game.part::<ClipShots>(id).fired(action), 2, "tick {tick}");
        }
    }
    assert_eq!(starts, [0, 3, 40, 43]);
    assert_eq!(Stats::health(&game, target), 80);
}
