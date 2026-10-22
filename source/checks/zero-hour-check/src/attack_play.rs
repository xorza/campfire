use std::num::NonZeroU32;

use campfire_harness::FixedSession;
use campfire_mode::{OptionPicks, OptionValue, SeatPick};
use campfire_package::ModePackages;
use campfire_protocol::{InputRules, SeatPlan};

use crate::failure::CheckFailure;
use crate::match_play::{MatchPlay, START_MONEY};
use crate::pins::{BotMatchEnd, HZ};

/// The most ticks the play's match runs.
const ATTACK_LIMIT: u64 = HZ * 60 * 20;

/// The attack play on Tournament Desert of `START_MONEY`: two bots from standard starts, the
/// mode's attacker of China at `Player_1_Start` and its builder of GLA at `Player_2_Start`, whose
/// gatherers walk (09-zero-hour.md, A builder and an attacker); the play
/// pins how their match stands as it ends or `ATTACK_LIMIT` passes, so both bots keep working
/// (09-zero-hour.md, A bots' match that ends).
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
        let factions: Vec<SeatPick> = ["0:faction=china", "1:faction=gla"]
            .map(|pick| pick.parse().expect("a seat's pick reads"))
            .into();
        let session = FixedSession::planned(packages, InputRules::ROOMY, [SeatPlan::Bot; 2])
            .rate(rate)
            .picked(picks.places())
            .seat_picked(&factions);
        let mut play = MatchPlay::new(session.start());
        let fought = Self::fight(&mut play, session.packages());
        let ended = play.end();
        match fought.and_then(|fought| ended.map(|()| fought)) {
            Ok(fought) => Some(fought),
            Err(failure) => {
                failures.push(failure);
                None
            }
        }
    }

    /// The attacker in seat 0 and the builder in seat 1 of `play`, of `packages`, each from its
    /// start, until their match ends or its limit passes.
    fn fight(play: &mut MatchPlay, packages: &ModePackages) -> Result<BotMatchEnd, CheckFailure> {
        for (seat, bot) in [(0, "attacker"), (1, "builder")] {
            let home = play.start(seat + 1)?;
            play.drive_mode_bot(packages, seat, bot, home)?;
        }
        Ok(play.fight(ATTACK_LIMIT))
    }
}
