//! Zero Hour's skirmish, its rules package's script played by a test mode (12-zero-hour.md, Eight
//! teams of one; 12-zero-hour.md, Defeat by the last building that counts).

use std::num::NonZeroU32;

use bevy_ecs::world::World;
use campfire_combat::{Combat, DamageCause};
use campfire_common::PlayerSlot;
use campfire_harness::{FixedMatch, FixedSession, MatchUnits};
use campfire_mode::MatchEnd;
use campfire_players::{HeldOut, PlayersWorld};
use campfire_protocol::{InputRules, Outcome, SessionLog, SlotPlan};
use campfire_runner::{Replay, SessionWorld};
use campfire_sim::StableId;
use campfire_stats::Life;
use campfire_test_packages::TestMode;
use campfire_units::{Dead, Units, UnitsWorld};
use campfire_values::{Names, ResourceId, UnitType};

/// The units of the type `name` that player `slot` owns, by stable id.
fn owned(world: &World, name: &str, slot: u32) -> Vec<StableId> {
    let typed: UnitType = Units::type_named(world, name).unwrap();
    MatchUnits::of(world)
        .all()
        .filter(|(_, unit)| {
            unit.get::<UnitType>() == Some(&typed) && MatchUnits::owned_by(unit, slot)
        })
        .map(|(id, _)| id)
        .collect()
}

/// The session of a skirmish whose slots `plan` opens.
fn session(plan: [SlotPlan; 3]) -> FixedSession {
    let rate = NonZeroU32::new(30).unwrap();
    let packages = TestMode::Skirmish.packages();
    FixedSession::planned(packages, rate, InputRules::ROOMY, plan)
}

/// A skirmish whose slots `plan` opens, after its start's tick.
fn skirmish(plan: [SlotPlan; 3]) -> FixedMatch {
    session(plan).start()
}

/// The money of player `slot`, the skirmish's one resource.
fn money(world: &World, slot: u32) -> i64 {
    let resources: Names<ResourceId> = ["money"].into_iter().collect();
    let money = resources.named("money").unwrap();
    world
        .player_resources(PlayerSlot::new(slot))
        .unwrap()
        .amount(money)
}

/// The one unit of the type `name` that player `slot` owns.
fn one(fixed: &FixedMatch, name: &str, slot: u32) -> StableId {
    let [unit] = owned(fixed.game(), name, slot)[..] else {
        panic!("player {slot}'s {name}");
    };
    unit
}

/// Deals each of `centers` its whole life, 10, and runs a tick.
fn destroy(fixed: &mut FixedMatch, centers: &[StableId]) {
    for &center in centers {
        Combat::damage(fixed.game_mut(), None, center, 10, DamageCause::Effect);
    }
    fixed.game_mut().run_tick_alone();
}

/// Whether the match ended.
fn ended(world: &World) -> bool {
    world
        .match_entity()
        .is_some_and(|entity| world.entity(entity).contains::<MatchEnd>())
}

/// Whether the script holds player `slot` out of play.
fn held_out(world: &World, slot: u32) -> bool {
    let player = world.player_entity(PlayerSlot::new(slot)).unwrap();
    world.entity(player).contains::<HeldOut>()
}

#[test]
fn a_skirmish_starts_each_held_slot_and_ends_as_the_last_team_with_a_building_stands() {
    // With slots 2 and 3 open, player 1 alone starts: its center at its start, its dozer 2 m
    // along +z.
    let fixed = skirmish([SlotPlan::Player, SlotPlan::Open, SlotPlan::Open]);
    let world = fixed.game();
    assert_eq!(owned(world, "center", 0).len(), 1);
    assert_eq!(owned(world, "dozer", 0).len(), 1);
    for slot in [1, 2] {
        assert!(owned(world, "center", slot).is_empty() && owned(world, "dozer", slot).is_empty());
    }

    // With bots in slots 2 and 3, all start. The second's center takes 10 damage, its whole
    // life, in tick 0: it dies as the tick's deaths find it, and the mode, which hears it then,
    // holds that bot out of play and takes its dozer's life whole; two teams hold a building
    // that counts, so the match goes on, and the center despawns. The dozer dies, and despawns,
    // in the next tick's deaths.
    let mut fixed = skirmish([SlotPlan::Player, SlotPlan::Bot, SlotPlan::Bot]);
    let center = one(&fixed, "center", 1);
    let dozer = one(&fixed, "dozer", 1);
    destroy(&mut fixed, &[center]);
    let world = fixed.game();
    assert!(owned(world, "center", 1).is_empty());
    let dozer_life = MatchUnits::of(world).unit(dozer).get::<Life>();
    assert_eq!(dozer_life.map(|life| life.is_empty()), Some(true));
    assert!(held_out(world, 1) && !held_out(world, 0) && !held_out(world, 2));
    assert!(!ended(world));
    fixed.game_mut().run_tick_alone();
    assert!(owned(fixed.game(), "dozer", 1).is_empty());
    // The third's center dies alike, and the match ends, player 1's team, the first, the
    // winner. The match runs no stage after it ends, so the center stays dead and that bot's
    // dozer does not die, its life gone.
    let center = one(&fixed, "center", 2);
    let dozer = one(&fixed, "dozer", 2);
    destroy(&mut fixed, &[center]);
    let world = fixed.game();
    assert!(MatchUnits::of(world).unit(center).contains::<Dead>());
    assert!(!MatchUnits::of(world).unit(dozer).contains::<Dead>());
    let dozer_life = MatchUnits::of(world).unit(dozer).get::<Life>();
    assert_eq!(dozer_life.map(|life| life.is_empty()), Some(true));
    assert!(ended(world));
    assert_eq!(fixed.end().outcome, Outcome::Won { team: 0 });

    // Of the last two centers dying in one tick, no team holds a building that counts: a draw.
    let mut fixed = skirmish([SlotPlan::Player, SlotPlan::Bot, SlotPlan::Open]);
    let centers = [one(&fixed, "center", 0), one(&fixed, "center", 1)];
    destroy(&mut fixed, &centers);
    assert_eq!(fixed.end().outcome, Outcome::Draw);
}

#[test]
fn every_player_starts_with_the_money_the_host_picked_and_a_replay_plays_with_it() {
    // Of no pick, each held slot starts with the option's default, 100; of the pick of its
    // second value, with 5,000, as the terms record it; an open slot starts with it too, as
    // the script gives each slot its money.
    let plan = [SlotPlan::Player, SlotPlan::Bot, SlotPlan::Open];
    let fixed = skirmish(plan);
    assert_eq!([0, 1, 2].map(|slot| money(fixed.game(), slot)), [100; 3]);
    let picked = session(plan).picked(&[1]);
    assert_eq!(picked.terms().options, [1]);
    let mut fixed = picked.start();
    assert_eq!([0, 1, 2].map(|slot| money(fixed.game(), slot)), [5000; 3]);
    // The published log replays to the same money, its session of the terms' pick.
    fixed.end();
    let mut bytes = Vec::new();
    fixed.game().session().log().encode(&mut bytes);
    let packages = TestMode::Skirmish.packages();
    let mut replay = Replay::of(SessionLog::decode(&bytes).unwrap(), &packages).unwrap();
    replay.run().unwrap();
    assert_eq!(money(replay.game(), 0), 5000);
}
