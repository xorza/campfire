//! The MOBA 3v3's rules for a player who joins after the pick: one who takes an open player_slot
//! gets the first hero no slot holds, and one who takes a bot's slot keeps the bot's hero; and
//! the player slot a joiner takes.

use campfire_common::{AfterLeave, PlayerSlot};
use campfire_harness::{FixedMatch, FixedSession, MatchUnits, Moba3v3};
use campfire_protocol::internals::TestKey;
use campfire_protocol::{
    Delegation, DelegationTerms, InputRules, LeaveReason, PlayerSlotPlan, ServerInput,
};
use campfire_runner::{PlayerSlotFor, SessionWorld};
use campfire_sim::{SimWorld, StableId};
use campfire_test_packages::TestMode;
use campfire_values::UnitType;

use crate::runner::moba::run_to;

/// The MOBA's slots with slot 5 opened as `last`.
const fn plan(last: PlayerSlotPlan) -> [PlayerSlotPlan; 6] {
    let mut plan = [PlayerSlotPlan::Player; 6];
    plan[5] = last;
    plan
}

/// The unit type of `id` in `fixed`.
fn unit_type(fixed: &FixedMatch, id: StableId) -> UnitType {
    let world = fixed.game();
    let entity = world.entity_of(id).unwrap();
    *world.get::<UnitType>(entity).unwrap()
}

#[test]
fn a_late_joiner_gets_the_first_free_hero_and_a_bots_taker_keeps_its_hero() {
    // In the MOBA, slot 5 picks Veil, the last of the mode's six heroes in the order of its
    // dependencies; the others pick the five before it. The pick ends in tick 1199.
    let moba = Moba3v3::load();
    let mut fixed = moba.start();
    run_to(&mut fixed, 1200);
    let veil = unit_type(&fixed, MatchUnits::of(fixed.game()).hero(5));

    // Slot 5 open: the pick gives it no hero. A player joins it before tick 1250 and gets Veil,
    // which no slot holds, in that tick.
    let moba = Moba3v3::planned(plan(PlayerSlotPlan::Open));
    let mut fixed = moba.start();
    run_to(&mut fixed, 1250);
    assert_eq!(MatchUnits::of(fixed.game()).heroes(5).count(), 0);
    fixed.join(5).unwrap();
    run_to(&mut fixed, 1251);
    let heroes: Vec<_> = MatchUnits::of(fixed.game()).heroes(5).collect();
    let [hero] = heroes[..] else {
        panic!("{heroes:?}");
    };
    assert_eq!(unit_type(&fixed, hero), veil);

    // Slot 5 a bot's, which picks Veil as the player would: a player who takes the slot over
    // keeps the bot's hero, and gets no other.
    let moba = Moba3v3::planned(plan(PlayerSlotPlan::Bot));
    let mut fixed = moba.start();
    run_to(&mut fixed, 1250);
    let bots = MatchUnits::of(fixed.game()).hero(5);
    fixed.join(5).unwrap();
    run_to(&mut fixed, 1251);
    let heroes: Vec<_> = MatchUnits::of(fixed.game()).heroes(5).collect();
    assert_eq!(heroes, [bots]);
}

#[test]
fn a_joiner_takes_its_player_slot_the_one_it_left_or_one_the_rules_open() {
    let player_slot_for = |fixed: &FixedMatch, delegation: &Delegation| {
        fixed.game().session().player_slot_for(delegation)
    };
    let joins = |player_slot| Some(PlayerSlotFor::Join(PlayerSlot::new(player_slot)));

    // The MOBA admits late joins and bot takeovers. Player 0 holds slot 0; with a delegation of
    // another session key, it takes the slot back by a renewal.
    let mut fixed = Moba3v3::planned(plan(PlayerSlotPlan::Open)).start();
    let first = fixed.delegation(0);
    let Some(PlayerSlotFor::Held {
        player_slot,
        chain,
        renews: false,
    }) = player_slot_for(&fixed, &first)
    else {
        panic!("player 0 holds slot 0");
    };
    assert_eq!(player_slot, PlayerSlot::new(0));
    assert_eq!(chain.player_slot(), player_slot);
    let keys = TestKey::player(0);
    let renewed = Delegation::sign(
        &TestKey::signer(keys.main),
        &DelegationTerms {
            session_key: TestKey::of(99).x_only_public_key().0,
            ..*first.terms()
        },
        TestKey::NOW,
    );
    assert!(matches!(
        player_slot_for(&fixed, &renewed),
        Some(PlayerSlotFor::Held { renews: true, .. })
    ));

    // A new player takes open slot 5. Once player 1 leaves its slot to a bot, it takes slot 1
    // back, and a new player still takes the open slot before the bot's.
    assert_eq!(player_slot_for(&fixed, &fixed.delegation(6)), joins(5));
    fixed
        .serve(ServerInput::Leave {
            player_slot: PlayerSlot::new(1),
            reason: LeaveReason::Asked,
            becomes: AfterLeave::Bot,
        })
        .unwrap();
    assert_eq!(player_slot_for(&fixed, &fixed.delegation(1)), joins(1));
    assert_eq!(player_slot_for(&fixed, &fixed.delegation(6)), joins(5));
    // With no open slot, a new player takes the first bot's.
    let bot = Moba3v3::planned(plan(PlayerSlotPlan::Bot)).start();
    assert_eq!(player_slot_for(&bot, &bot.delegation(6)), joins(5));

    // The lane admits no late join: an open player slot takes no new player.
    let lane = FixedSession::planned(
        TestMode::Lane.packages(),
        InputRules::ROOMY,
        [PlayerSlotPlan::Player, PlayerSlotPlan::Open],
    )
    .start();
    assert_eq!(player_slot_for(&lane, &lane.delegation(2)), None);
    assert!(matches!(
        player_slot_for(&lane, &lane.delegation(0)),
        Some(PlayerSlotFor::Held { .. })
    ));
}
