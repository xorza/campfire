//! A bot's input the session log refuses, which the server drops by its cause.

use campfire_common::PlayerSlot;
use campfire_harness::FixedSession;
use campfire_protocol::{InputRules, PlayerSlotPlan};
use campfire_runner::BotInputDrop;
use campfire_test_packages::TestMode;

#[test]
fn a_bots_input_the_log_refuses_is_dropped_by_its_cause() {
    // Under the LAN terms a payload of the max length, 1,024 bytes, is logged, and one of 1,025
    // is too large. The bot's slot then takes 4 inputs a tick for the next tick and the 30 of the
    // max lead past it, 4 × 31 = 124 with the first, and the 125th is ahead of time.
    let rules = InputRules::LAN;
    let packages = TestMode::Skirmish.packages();
    let session = FixedSession::planned(packages, rules, [PlayerSlotPlan::Bot; 3]);
    let mut fixed = session.each_picked("faction", "base").start();
    let bot = PlayerSlot::new(0);
    let most = usize::try_from(rules.max_payload_len).unwrap();
    assert_eq!(most, 1024);
    assert_eq!(fixed.serve_bot(bot, &vec![0; most]), Ok(()));
    assert_eq!(
        fixed.serve_bot(bot, &vec![0; most + 1]),
        Err(BotInputDrop::TooLarge)
    );
    let room = rules.max_per_tick * (u32::try_from(rules.max_lead.get()).unwrap() + 1);
    assert_eq!(room, 124);
    for at in 1..room {
        assert_eq!(fixed.serve_bot(bot, &[0]), Ok(()), "input {at}");
    }
    assert_eq!(fixed.serve_bot(bot, &[0]), Err(BotInputDrop::AheadOfTime));
}
