use std::num::NonZeroU32;

use campfire_harness::FixedSession;
use campfire_mode::{OptionPicks, OptionValue, PlayerSlotPick};
use campfire_package::ModePackages;
use campfire_protocol::{InputRules, PlayerSlotPlan};

use crate::failure::CheckFailure;
use crate::match_play::{MatchPlay, PlayTools, START_MONEY};
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
        tools: PlayTools,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<BotMatchEnd> {
        let played = Self::session(packages, rate)
            .and_then(|session| Self::with_play(&session, tools, |play| play.fight(ATTACK_LIMIT)));
        match played {
            Ok(fought) => Some(fought),
            Err(failure) => {
                failures.push(failure);
                None
            }
        }
    }

    /// The session of the attack play of `packages` at `rate` ticks a second, which starts each of
    /// its matches.
    pub(crate) fn session(
        packages: ModePackages,
        rate: NonZeroU32,
    ) -> Result<FixedSession, CheckFailure> {
        let start_money: OptionValue = START_MONEY.parse().expect("the play's option");
        let picks = OptionPicks::named(&packages.data().options, &[start_money])
            .map_err(CheckFailure::Options)?;
        let factions: Vec<PlayerSlotPick> = ["0:faction=china", "1:faction=gla"]
            .map(|pick| pick.parse().expect("a player slot's pick reads"))
            .into();
        let session = FixedSession::planned(packages, InputRules::LAN, [PlayerSlotPlan::Bot; 2])
            .rate(rate)
            .picked(picks.places())
            .player_slot_picked(&factions);
        Ok(session)
    }

    /// Starts the two bots in a match of `session`, the attacker in player slot 0 and the builder
    /// in player slot 1, each from its start, runs `steps` on it, and ends it: what `steps` gave,
    /// or the play's failure.
    pub(crate) fn with_play<T>(
        session: &FixedSession,
        tools: PlayTools,
        steps: impl FnOnce(&mut MatchPlay<'_>) -> T,
    ) -> Result<T, CheckFailure> {
        let mut play = MatchPlay::new(session.start(), session.packages(), tools);
        let stepped = Self::start(&mut play, session.packages()).map(|()| steps(&mut play));
        let closed = play.end();
        let stepped = stepped?;
        closed?;
        Ok(stepped)
    }

    /// The attacker in player slot 0 and the builder in player slot 1 of `play`, of `packages`,
    /// each driven from its start.
    fn start(play: &mut MatchPlay<'_>, packages: &ModePackages) -> Result<(), CheckFailure> {
        for (player_slot, bot) in [(0, "attacker"), (1, "builder")] {
            let home = play.start(player_slot + 1)?;
            play.drive_mode_bot(packages, player_slot, bot, home)?;
        }
        Ok(())
    }
}
