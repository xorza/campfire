//! Zero Hour's skirmish, its rules package's script played by a test mode (09-zero-hour.md, Eight
//! teams of one; 09-zero-hour.md, Defeat by the last building that counts).

use bevy_ecs::world::World;
use campfire_combat::{Combat, DamageCause};
use campfire_common::PlayerSlot;
use campfire_harness::{FixedMatch, FixedSession, MatchUnits};
use campfire_math::Num;
use campfire_mode::{Choices, MatchEnd, ModeWorld, Offer, PlayerSlotPick};
use campfire_players::{HeldOut, PlayersWorld};
use campfire_production::Production;
use campfire_protocol::{Outcome, PlayerSlotPlan, SessionLog};
use campfire_runner::{Replay, SessionWorld};
use campfire_sim::StableId;
use campfire_sim::internals::SimWorldInternals;
use campfire_test_packages::{Edit, PackageTree, TestMode};
use campfire_units::{Body, Dead, Life, Owner, StatusTags, Units, UnitsWorld, Variation};
use campfire_values::UnitType;

/// The units of the type `name` that player `player_slot` owns, by stable id.
fn owned(world: &World, name: &str, player_slot: u32) -> Vec<StableId> {
    let typed: UnitType = Units::type_named(world, name).unwrap();
    let owned = MatchUnits::of(world).owned(typed, player_slot);
    owned.map(|(id, _)| id).collect()
}

/// The session of a skirmish whose slots `plan` opens, each of the faction `base`, its center
/// and its dozer.
fn session(plan: [PlayerSlotPlan; 3]) -> FixedSession {
    unpicked(plan).each_picked("faction", "base")
}

/// The session of a skirmish whose slots `plan` opens, of no faction picked.
fn unpicked(plan: [PlayerSlotPlan; 3]) -> FixedSession {
    let packages = TestMode::Skirmish.packages();
    FixedSession::planned(packages, plan)
}

/// A skirmish whose slots `plan` opens, after its start's tick.
fn skirmish(plan: [PlayerSlotPlan; 3]) -> FixedMatch {
    session(plan).start()
}

/// The money of player `player_slot`, the skirmish's one resource.
fn money(world: &World, player_slot: u32) -> i64 {
    let units = MatchUnits::of(world);
    units.amount(player_slot, units.resource("money").unwrap())
}

/// The one unit of the type `name` that player `player_slot` owns.
fn one(fixed: &FixedMatch, name: &str, player_slot: u32) -> StableId {
    let [unit] = owned(fixed.game(), name, player_slot)[..] else {
        panic!("player {player_slot}'s {name}");
    };
    unit
}

/// Deals each of `centers` its whole life, 10, and runs a tick.
fn destroy(fixed: &mut FixedMatch, centers: &[StableId]) {
    for &center in centers {
        Combat::damage(fixed.game_mut(), None, center, 10, DamageCause::Effect);
    }
    fixed.step();
}

/// Whether the match ended.
fn is_ended(world: &World) -> bool {
    world
        .match_entity()
        .is_some_and(|entity| world.entity(entity).contains::<MatchEnd>())
}

/// Whether the script holds player `player_slot` out of play.
fn held_out(world: &World, player_slot: u32) -> bool {
    let player = world.player_entity(PlayerSlot::new(player_slot)).unwrap();
    world.entity(player).contains::<HeldOut>()
}

#[test]
fn a_skirmish_starts_each_held_slot_and_ends_as_the_last_team_with_a_building_stands() {
    // With slots 2 and 3 open, player 1 alone starts: its center at its start, its dozer 2 m
    // along +z.
    let fixed = skirmish([
        PlayerSlotPlan::Player,
        PlayerSlotPlan::Open,
        PlayerSlotPlan::Open,
    ]);
    let world = fixed.game();
    assert_eq!(owned(world, "center", 0).len(), 1);
    assert_eq!(owned(world, "dozer", 0).len(), 1);
    for slot in [1, 2] {
        assert!(owned(world, "center", slot).is_empty() && owned(world, "dozer", slot).is_empty());
    }

    // With bots in slots 2 and 3, all start. The second's center takes 10 damage, its whole
    // life, in tick 0: it dies as the tick's deaths find it, and the mode, which hears it then,
    // holds that bot out of play and takes its dozer's life whole; two teams hold a building
    // that counts, so the match goes on, and the center stays dead until its decay. The dozer
    // dies in the next tick's deaths.
    let mut fixed = skirmish([
        PlayerSlotPlan::Player,
        PlayerSlotPlan::Bot,
        PlayerSlotPlan::Bot,
    ]);
    let center = one(&fixed, "center", 1);
    let dozer = one(&fixed, "dozer", 1);
    destroy(&mut fixed, &[center]);
    let world = fixed.game();
    assert!(MatchUnits::of(world).unit(center).contains::<Dead>());
    let dozer_life = MatchUnits::of(world).unit(dozer).get::<Life>();
    assert_eq!(dozer_life.map(|life| life.is_empty()), Some(true));
    assert!(held_out(world, 1) && !held_out(world, 0) && !held_out(world, 2));
    assert!(!is_ended(world));
    fixed.step();
    assert!(MatchUnits::of(fixed.game()).unit(dozer).contains::<Dead>());
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
    assert!(is_ended(world));
    assert_eq!(fixed.end().outcome, Outcome::Won { team: 0 });

    // Of the last two centers dying in one tick, no team holds a building that counts: a draw.
    let mut fixed = skirmish([
        PlayerSlotPlan::Player,
        PlayerSlotPlan::Bot,
        PlayerSlotPlan::Open,
    ]);
    let centers = [one(&fixed, "center", 0), one(&fixed, "center", 1)];
    destroy(&mut fixed, &centers);
    assert_eq!(fixed.end().outcome, Outcome::Draw);
}

/// A player whose last building that counts goes with no death, as a site cancelled goes, is out
/// of play as the tick's count finds it holds none, and two teams still hold one; one whose last
/// one passes to another player is out alike, and the match ends, the taker's team the winner.
#[test]
fn a_player_whose_last_building_goes_or_changes_hands_is_out() {
    let mut fixed = skirmish([
        PlayerSlotPlan::Player,
        PlayerSlotPlan::Bot,
        PlayerSlotPlan::Bot,
    ]);
    let center = one(&fixed, "center", 1);
    let world = fixed.game_mut();
    let entity = world.unit_entity(center);
    world.despawn(entity);
    fixed.step();
    let world = fixed.game();
    assert!(held_out(world, 1) && !held_out(world, 0) && !held_out(world, 2));
    assert!(!is_ended(world));

    let center = one(&fixed, "center", 2);
    let world = fixed.game_mut();
    let team = world.team_of(PlayerSlot::new(0)).unwrap();
    let entity = world.unit_entity(center);
    world
        .entity_mut(entity)
        .insert((Owner::new(PlayerSlot::new(0)), team));
    fixed.step();
    assert!(held_out(fixed.game(), 2));
    assert_eq!(fixed.end().outcome, Outcome::Won { team: 0 });
}

#[test]
fn a_slot_starts_as_its_hosts_pick_and_one_of_no_pick_draws_its_faction_and_keeps_it() {
    // Player 1 picked the outpost, its center alone; the bots of slots 2 and 3 picked nothing,
    // each draws a faction, which counts as its pick, and starts as it.
    let plan = [
        PlayerSlotPlan::Player,
        PlayerSlotPlan::Bot,
        PlayerSlotPlan::Bot,
    ];
    let outpost: PlayerSlotPick = "0:faction=outpost".parse().unwrap();
    let fixed = unpicked(plan).player_slot_picked(&[outpost]).start();
    let world = fixed.game();
    // Each player slot's row holds its faction, then its start, which none picked.
    let chosen = world.resource::<Choices>().offers();
    let faction = |player_slot: usize| chosen[2 * player_slot];
    assert_eq!(chosen[..2], [Some(Offer::new(1)), None]);
    assert_eq!(owned(world, "center", 0).len(), 1);
    assert!(owned(world, "dozer", 0).is_empty());
    for player_slot in [1, 2] {
        let base = match faction(player_slot) {
            Some(offer) if offer == Offer::new(0) => true,
            Some(offer) if offer == Offer::new(1) => false,
            other => panic!("slot {player_slot} drew {other:?}"),
        };
        let player_slot = u32::try_from(player_slot).unwrap();
        assert_eq!(owned(world, "center", player_slot).len(), 1);
        assert_eq!(
            owned(world, "dozer", player_slot).len(),
            usize::from(base),
            "slot {player_slot}"
        );
    }
}

#[test]
fn a_player_slot_that_picks_a_base_start_gets_its_factions_base_beside_its_start() {
    // Player 1 picked the start `base`: beside its center and dozer, its faction's base, a second
    // center and `base_count` dozers, 2; the bot of slot 2 picked none, and starts as standard
    // (09-zero-hour.md, A player slot picks its start; 09-zero-hour.md, A base start).
    let plan = [
        PlayerSlotPlan::Player,
        PlayerSlotPlan::Bot,
        PlayerSlotPlan::Open,
    ];
    let picks = [
        "0:faction=base",
        "1:faction=base",
        "2:faction=base",
        "0:start=base",
    ]
    .map(|pick| pick.parse::<PlayerSlotPick>().unwrap());
    let fixed = unpicked(plan).player_slot_picked(&picks).start();
    let world = fixed.game();
    let counts = |player_slot| {
        [
            owned(world, "center", player_slot).len(),
            owned(world, "dozer", player_slot).len(),
        ]
    };
    assert_eq!(counts(0), [2, 3]);
    assert_eq!(counts(1), [1, 1]);
}

#[test]
fn every_player_starts_with_the_money_the_host_picked_and_a_replay_plays_with_it() {
    // Of no pick, each held slot starts with the option's default, 100; of the pick of its
    // second value, with 5,000, as the terms record it; an open player slot starts with it too, as
    // the script gives each slot its money.
    let plan = [
        PlayerSlotPlan::Player,
        PlayerSlotPlan::Bot,
        PlayerSlotPlan::Open,
    ];
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

#[test]
fn a_kill_pays_its_killers_player_its_share_of_the_victims_cost_rounded_up() {
    // The center costs 1,001 money, and the start raises player 1's share to a quarter: its
    // dozer's kill of the second's center pays it 250.25 rounded up, 251; a share of none, the
    // rules' start, pays nothing.
    let mut tree = PackageTree::of(TestMode::Skirmish);
    tree.edit(
        "test/engine/modes/skirmish/data/units.toml",
        Edit::Replace(
            "combat = { on_death = \"despawn\" }",
            "combat = { on_death = \"despawn\" }\nbounty = { resource = \"money\", cost = 1001 }",
        ),
    );
    tree.edit(
        "zero-hour/scripts/mode.rhai",
        Edit::Replace(
            "fn on_match_start(ctx) {\n",
            "fn on_match_start(ctx) {\n    ctx.set_bounty_share(ctx.players[0], num(1) / 4);\n",
        ),
    );
    let plan = [
        PlayerSlotPlan::Player,
        PlayerSlotPlan::Bot,
        PlayerSlotPlan::Bot,
    ];
    let mut fixed = FixedSession::planned(tree.load().unwrap(), plan)
        .each_picked("faction", "base")
        .start();
    let dozer = one(&fixed, "dozer", 0);
    let center = one(&fixed, "center", 1);
    Combat::damage(
        fixed.game_mut(),
        Some(dozer),
        center,
        10,
        DamageCause::Effect,
    );
    fixed.step();
    let moneys = |fixed: &FixedMatch| [0, 1, 2].map(|slot| money(fixed.game(), slot));
    assert_eq!(moneys(&fixed), [100 + 251, 100, 100]);
    // The third's dozer kills the first's center: its share of none pays nothing.
    let dozer = one(&fixed, "dozer", 2);
    let center = one(&fixed, "center", 0);
    Combat::damage(
        fixed.game_mut(),
        Some(dozer),
        center,
        10,
        DamageCause::Effect,
    );
    fixed.step();
    assert_eq!(moneys(&fixed), [351, 100, 100]);
}

/// A kill of a site, a building under construction, pays no bounty under Zero Hour's rules, whose
/// `[bounty]` selects victims of no `constructing` tag at their death, as the original's
/// `Player::doBountyForKill` pays none for one; the same kill pays its share with no victim filter.
#[test]
fn a_kill_of_a_site_under_construction_pays_no_bounty_by_the_victim_filter() {
    let money_after_a_site_kill = |filtered: bool| {
        let mut tree = PackageTree::of(TestMode::Skirmish);
        tree.edit(
            "test/engine/modes/skirmish/data/units.toml",
            Edit::Replace(
                "combat = { on_death = \"despawn\" }",
                "combat = { on_death = \"despawn\" }\nbounty = { resource = \"money\", cost = 1001 }",
            ),
        );
        tree.edit(
            "zero-hour/scripts/mode.rhai",
            Edit::Replace(
                "fn on_match_start(ctx) {\n",
                "fn on_match_start(ctx) {\n    ctx.set_bounty_share(ctx.players[0], num(1) / 4);\n",
            ),
        );
        if !filtered {
            tree.edit("zero-hour/data/rules.toml", Edit::Remove("bounty.victim"));
        }
        let plan = [
            PlayerSlotPlan::Player,
            PlayerSlotPlan::Bot,
            PlayerSlotPlan::Bot,
        ];
        let mut fixed = FixedSession::planned(tree.load().unwrap(), plan)
            .each_picked("faction", "base")
            .start();
        let dozer = one(&fixed, "dozer", 0);
        let center = one(&fixed, "center", 1);
        let site = StatusTags::from([Production::CONSTRUCTING]);
        fixed.game_mut().insert_parts(center, site);
        fixed.step();
        Combat::damage(
            fixed.game_mut(),
            Some(dozer),
            center,
            10,
            DamageCause::Effect,
        );
        fixed.step();
        money(fixed.game(), 0)
    };
    // Each player starts with 100; with no filter the site pays a quarter of 1,001, rounded up.
    assert_eq!(money_after_a_site_kill(true), 100);
    assert_eq!(money_after_a_site_kill(false), 100 + 251);
}

#[test]
fn a_dozer_of_variations_spawns_as_its_draw_by_every_start_and_counts_as_its_type() {
    // The skirmish's dozer with three variations, circles of 0.25, 0.5 and 0.75 m: each slot's
    // start spawns a dozer, and slot 1's base start two more, by the mode's script; each dozer
    // holds the variation its spawn drew and that variation's body, and every count of the type
    // finds it. A neutral dozer the map places of the third variation spawns as it
    // (04-capabilities/00-overview.md, A placement may give its variation).
    let mut tree = PackageTree::of(TestMode::Skirmish);
    let varied = "[units.dozer]\nvariations = [{ collision = { radius = \"0.25\" } }, \
                  { collision = { radius = \"0.5\" } }, { collision = { radius = \"0.75\" } }]\n";
    tree.edit(
        "test/engine/modes/skirmish/data/units.toml",
        Edit::Replace("[units.dozer]\n", varied),
    );
    let placed = "[[units]]\nunit_type = \"dozer\"\nteam = \"neutral\"\nposition = [18, -18]\n\
                  variation = 2\n\n[[markers]]\nname = \"w1\"";
    tree.edit(
        "test/engine/modes/skirmish/map/field/map.toml",
        Edit::Replace("[[markers]]\nname = \"w1\"", placed),
    );
    let plan = [
        PlayerSlotPlan::Player,
        PlayerSlotPlan::Bot,
        PlayerSlotPlan::Bot,
    ];
    let picks = [
        "0:faction=base",
        "1:faction=base",
        "2:faction=base",
        "0:start=base",
    ]
    .map(|pick| pick.parse::<PlayerSlotPick>().unwrap());
    let fixed = FixedSession::planned(tree.packages(), plan)
        .player_slot_picked(&picks)
        .start();
    let world = fixed.game();
    let counts = [0, 1, 2].map(|player_slot| owned(world, "dozer", player_slot).len());
    assert_eq!(counts, [3, 1, 1]);
    let units = MatchUnits::of(world);
    for player_slot in 0..3 {
        for dozer in owned(world, "dozer", player_slot) {
            let unit = units.unit(dozer);
            let variation = unit
                .get::<Variation>()
                .expect("a dozer of variations")
                .get();
            let radius = Num::int(i64::from(variation) + 1) / 4;
            let body = unit.get::<Body>().unwrap();
            assert_eq!(body.shape().bound(), radius, "{dozer:?}");
        }
    }
    let neutral = units.unit(units.spawned_at((18, 0, -18)));
    assert_eq!(neutral.get::<Variation>(), Some(&Variation::new(2)));
    let body = neutral.get::<Body>().unwrap();
    assert_eq!(body.shape().bound(), Num::int(3) / 4);
}
