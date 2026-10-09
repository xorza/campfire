//! The MOBA 3v3's rules for a player who joins after the pick: one who takes an open slot
//! gets the first hero no slot holds, and one who takes a bot's slot keeps the bot's hero.

use campfire_capabilities::UnitType;
use campfire_protocol::SlotPlan;
use campfire_runner::internals::{FixedMatch, MatchUnits, Moba3v3};
use campfire_sim::{EntityIndex, StableId};

use crate::moba::run_to;

/// The MOBA's slots with slot 5 opened as `last`.
fn plan(last: SlotPlan) -> Vec<SlotPlan> {
    let mut plan = vec![SlotPlan::Player; 5];
    plan.push(last);
    plan
}

/// The unit type of `unit` in `fixed`.
fn unit_type(fixed: &FixedMatch, unit: StableId) -> UnitType {
    let world = fixed.runner().world();
    let entity = world.resource::<EntityIndex>().get(unit).unwrap();
    *world.get::<UnitType>(entity).unwrap()
}

#[test]
fn a_late_joiner_gets_the_first_free_hero_and_a_bots_taker_keeps_its_hero() {
    // In the MOBA, slot 5 picks Veil, the last of the mode's six heroes in the order of its
    // dependencies; the others pick the five before it. The pick ends in tick 1199.
    let moba = Moba3v3::load();
    let mut fixed = moba.start();
    run_to(&mut fixed, 1200);
    let veil = unit_type(&fixed, MatchUnits::of(&fixed).hero(5));

    // Slot 5 open: the pick gives it no hero. A player joins it before tick 1250 and gets Veil,
    // which no slot holds, in that tick.
    let moba = Moba3v3::planned(plan(SlotPlan::Open));
    let mut fixed = moba.start();
    run_to(&mut fixed, 1250);
    assert_eq!(MatchUnits::of(&fixed).heroes(5).count(), 0);
    fixed.join(5).unwrap();
    run_to(&mut fixed, 1251);
    let heroes: Vec<_> = MatchUnits::of(&fixed).heroes(5).collect();
    let [hero] = heroes[..] else {
        panic!("{heroes:?}");
    };
    assert_eq!(unit_type(&fixed, hero), veil);

    // Slot 5 a bot's, which picks Veil as the player would: a player who takes the slot over
    // keeps the bot's hero, and gets no other.
    let moba = Moba3v3::planned(plan(SlotPlan::Bot));
    let mut fixed = moba.start();
    run_to(&mut fixed, 1250);
    let bots = MatchUnits::of(&fixed).hero(5);
    fixed.join(5).unwrap();
    run_to(&mut fixed, 1251);
    let heroes: Vec<_> = MatchUnits::of(&fixed).heroes(5).collect();
    assert_eq!(heroes, [bots]);
}
