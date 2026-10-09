use std::collections::{BTreeMap, BTreeSet};

use campfire_common::{PlayerSlot, Tick};
use campfire_log::LogLevel;
use campfire_net::{
    InputsDiscarded, LinkLost, Listening, MatchStarted, OrdersSent, SessionWritten, TicksCaughtUp,
};
use campfire_verifier::Verified;

use crate::error::CheckError;
use crate::failure::Failure;
use crate::process::Process;
use crate::process_log::ProcessLog;
use crate::process_outcome::ProcessOutcome;
use crate::session_kind::SessionKind;

/// What the check found wrong with a LAN match, from the processes' outcomes and logs; nothing
/// when the match passed.
#[derive(Debug, Default)]
pub(crate) struct Verdict {
    failures: Vec<Failure>,
}

/// An input a session's log took: its slot, its stamp, and the tick it took effect in, as the
/// published log holds it, a server bot's among them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LoggedInput {
    pub(crate) slot: PlayerSlot,
    pub(crate) stamp: Tick,
    pub(crate) tick: Tick,
}

/// What a bot process logged of its match, the bot's index, the number of orders its script
/// holds, and whether it ran to its end: one the check stopped sent only some. Of the orders it
/// sent, those it kept are the ones no resume discarded.
#[derive(Debug)]
pub(crate) struct BotEvents {
    pub(crate) bot: usize,
    pub(crate) started: Option<MatchStarted>,
    pub(crate) sent: Vec<OrdersSent>,
    pub(crate) kept: Vec<OrdersSent>,
    pub(crate) scripted: usize,
    pub(crate) whole: bool,
}

impl BotEvents {
    /// What bot `bot`'s process logged in `log`, its script of `scripted` orders, and whether it
    /// ran to its end.
    pub(crate) fn of(
        bot: usize,
        log: &ProcessLog,
        scripted: usize,
        whole: bool,
    ) -> Result<BotEvents, CheckError> {
        Ok(BotEvents {
            bot,
            started: log.first::<MatchStarted>()?,
            sent: log.read_all::<OrdersSent>()?,
            kept: log.kept_orders()?,
            scripted,
            whole,
        })
    }
}

impl Verdict {
    /// Checks that `process` succeeded, and logged no warning or error.
    pub(crate) fn process(&mut self, process: Process, outcome: ProcessOutcome, log: &ProcessLog) {
        if outcome != ProcessOutcome::Succeeded {
            self.failures.push(Failure::Ended { process, outcome });
        }
        self.warned(process, log);
    }

    /// Checks that `process` logged no warning or error but the inputs a resume discarded, which
    /// the orders' check counts.
    fn warned(&mut self, process: Process, log: &ProcessLog) {
        for line in log.lines() {
            if line.level >= LogLevel::Warn && line.read::<InputsDiscarded>().is_none() {
                self.failures.push(Failure::Warned {
                    process,
                    level: line.level,
                    target: line.target.clone(),
                    fields: line.fields.clone(),
                });
            }
        }
    }

    /// Checks that `process`, which the check stops mid-match, ran until it did, and logged no
    /// warning or error.
    pub(crate) fn stopped(&mut self, process: Process, outcome: ProcessOutcome, log: &ProcessLog) {
        if outcome != ProcessOutcome::Stopped {
            self.failures.push(Failure::NotStopped { process, outcome });
        }
        self.warned(process, log);
    }

    /// Checks that the impostor bot exited with failure, and logged why its link failed: a client
    /// whose link fails ends, and says why.
    pub(crate) fn impostor(&mut self, outcome: ProcessOutcome, lost: &[LinkLost]) {
        if !matches!(outcome, ProcessOutcome::Failed { .. }) {
            self.failures.push(Failure::ImpostorNotRefused { outcome });
        }
        if lost.is_empty() {
            self.failures.push(Failure::ImpostorSilent);
        }
    }

    /// Checks that the server listened.
    pub(crate) fn listened(&mut self, listening: &[Listening]) {
        if listening.is_empty() {
            self.failures.push(Failure::NeverListened);
        }
    }

    /// Checks that each bot process learned its slot and, when it ran to its end, sent every
    /// order of its script, and that the session's log, `logged`, holds exactly the inputs the
    /// bots sent and kept, each taking effect in its stamp tick, or after the ticks of a frame
    /// that `caught_up` with a stall and that it waited for.
    pub(crate) fn orders(
        &mut self,
        logged: &[LoggedInput],
        caught_up: &[TicksCaughtUp],
        bots: &[BotEvents],
    ) {
        let mut sent = BTreeMap::<(PlayerSlot, Tick), usize>::new();
        for events in bots {
            let bot = events.bot;
            let Some(MatchStarted { slot, .. }) = events.started else {
                self.failures.push(Failure::NoSlot { bot });
                continue;
            };
            for &OrdersSent { stamp, orders, .. } in &events.kept {
                *sent.entry((slot, stamp)).or_default() += orders;
            }
            let count: usize = events.sent.iter().map(|sent| sent.orders).sum();
            if events.whole && count != events.scripted {
                self.failures.push(Failure::OrderCount {
                    bot,
                    sent: count,
                    scripted: events.scripted,
                });
            }
        }
        let mut by_stamp = BTreeMap::<(PlayerSlot, Tick), usize>::new();
        for &LoggedInput { slot, stamp, tick } in logged {
            *by_stamp.entry((slot, stamp)).or_default() += 1;
            let waited = caught_up.iter().any(|ticks| ticks.delayed(stamp, tick));
            if tick != stamp && !waited {
                self.failures.push(Failure::Moved { slot, stamp, tick });
            }
        }
        let keys: BTreeSet<_> = sent.keys().chain(by_stamp.keys()).collect();
        for &(slot, stamp) in keys {
            let sent = sent.get(&(slot, stamp)).copied().unwrap_or(0);
            let logged = by_stamp.get(&(slot, stamp)).copied().unwrap_or(0);
            if sent != logged {
                self.failures.push(Failure::Unlogged {
                    slot,
                    stamp,
                    sent,
                    logged,
                });
            }
        }
    }

    /// Checks that the host of `session` wrote the session log, and that the verifier replayed
    /// it to the host's final hash.
    pub(crate) fn hash(
        &mut self,
        session: SessionKind,
        written: Option<&SessionWritten>,
        verified: Option<&Verified>,
    ) {
        let Some(written) = written else {
            self.failures.push(Failure::NoLog { session });
            return;
        };
        match verified {
            None => self.failures.push(Failure::NotVerified { session }),
            Some(verified) if verified.hash != written.hash => {
                self.failures.push(Failure::OtherHash {
                    session,
                    host: written.hash,
                    verifier: verified.hash,
                });
            }
            Some(_) => {}
        }
    }

    /// The failures found, less each that follows from another: with a session's host that did
    /// not succeed, its missing session log and final hash; with its verifier that did not, the
    /// final hash.
    pub(crate) fn failures(&self) -> impl Iterator<Item = &Failure> {
        let ended = |of: Process| {
            self.failures
                .iter()
                .any(|failure| matches!(failure, Failure::Ended { process, .. } if *process == of))
        };
        self.failures.iter().filter(move |failure| match failure {
            Failure::NoLog { session } => !ended(session.host()),
            Failure::NotVerified { session } => {
                !ended(session.host()) && !ended(session.verifier())
            }
            _ => true,
        })
    }
}

#[cfg(test)]
mod tests;
