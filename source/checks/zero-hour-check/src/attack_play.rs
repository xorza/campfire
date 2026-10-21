use std::num::NonZeroU32;

use campfire_harness::FixedSession;
use campfire_mode::internals::MarkerOf;
use campfire_mode::{Mode, OptionPicks, OptionValue};
use campfire_package::ModePackages;
use campfire_protocol::{InputRules, SeatPlan};

use crate::failure::CheckFailure;
use crate::match_play::{self, MatchPlay, START_MONEY, america};
use crate::pins::{BotMatchEnd, HZ};
use crate::skirmish_play::BUILDER;

/// The attacker bot's orders, which the play's first bot plays and a host gives a local server's
/// bot (09-zero-hour.md, A bot that builds and attacks).
const ATTACKER: &str =
    include_str!("../../../bots/zero-hour/tournament_desert/start1-america-attacker.toml");

/// The most ticks the play's match runs.
const ATTACK_LIMIT: u64 = HZ * 60 * 15;

/// The attack play on Tournament Desert of `START_MONEY`: two bots of America, the attacker at
/// `Player_1_Start`, which builds its base and sends its army at the other start every minute,
/// and the builder at `Player_2_Start`, which builds its own and attacks nothing; the play pins how
/// their match stands as it ends or `ATTACK_LIMIT` passes, so both scripts keep working.
#[derive(Debug)]
pub(crate) struct AttackPlay;

impl AttackPlay {
    /// Plays the two bots in a match of `packages` at `rate` ticks a second: how the match stood,
    /// or none, with each failure in `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<BotMatchEnd> {
        let start_money: OptionValue = START_MONEY.parse().expect("the play's option");
        let picks = match OptionPicks::named(&packages.data().options, &[start_money]) {
            Ok(picks) => picks,
            Err(error) => {
                failures.push(CheckFailure::Options(error));
                return None;
            }
        };
        let mut bots = Vec::with_capacity(2);
        for text in [ATTACKER, BUILDER] {
            match match_play::bot_script(&packages, text) {
                Ok(bot) => bots.push(bot),
                Err(failure) => {
                    failures.push(failure);
                    return None;
                }
            }
        }
        let session = FixedSession::planned(packages, InputRules::ROOMY, [SeatPlan::Bot; 2])
            .rate(rate)
            .picked(picks.places())
            .seat_picked(&america(2));
        let mut play = MatchPlay::new(session.start());
        for (seat, (bot, start)) in
            (0..).zip(bots.into_iter().zip(["Player_1_Start", "Player_2_Start"]))
        {
            let of = MarkerOf {
                tag: "start",
                param: "name",
                value: start,
            };
            let Some(home) = Mode::marker_point(play.game(), of) else {
                failures.push(CheckFailure::NoStarts);
                play.end();
                return None;
            };
            play.drive_bot(seat, bot, home);
        }
        let ended = play.fight(ATTACK_LIMIT);
        play.end();
        Some(ended)
    }
}
