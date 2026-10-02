use std::collections::{BTreeMap, BTreeSet};

use campfire_common::{PlayerSlot, Tick};
use campfire_log::Level;
use campfire_net::{
    InputLogged, LinkLost, Listening, MatchStarted, OrdersSent, SessionWritten, TicksCaughtUp,
};
use campfire_verifier::Verified;

use crate::failure::Failure;
use crate::outcome::Outcome;
use crate::process::Process;
use crate::process_log::ProcessLog;

/// What the check found wrong with a LAN match, from the processes' outcomes and logs; nothing
/// when the match passed.
#[derive(Debug, Default)]
pub(crate) struct Verdict {
    failures: Vec<Failure>,
}

/// What a bot logged of its match, and the number of orders its script holds.
#[derive(Debug)]
pub(crate) struct BotEvents {
    pub(crate) started: Option<MatchStarted>,
    pub(crate) sent: Vec<OrdersSent>,
    pub(crate) scripted: usize,
}

impl Verdict {
    /// Checks that `process` succeeded, and logged no warning or error.
    pub(crate) fn process(&mut self, process: Process, outcome: Outcome, log: &ProcessLog) {
        if outcome != Outcome::Succeeded {
            self.failures.push(Failure::Ended { process, outcome });
        }
        for line in log.lines() {
            if line.level >= Level::Warn {
                self.failures.push(Failure::Warned {
                    process,
                    level: line.level,
                    target: line.target.clone(),
                    fields: line.fields.clone(),
                });
            }
        }
    }

    /// Checks that the impostor bot exited with failure, and logged why its link failed: a client
    /// whose link fails ends, and says why.
    pub(crate) fn impostor(&mut self, outcome: Outcome, lost: &[LinkLost]) {
        if !matches!(outcome, Outcome::Failed { .. }) {
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

    /// Checks that each bot learned its slot and sent every order of its script, and that the
    /// server logged exactly the inputs the bots sent, each taking effect in its stamp tick, or
    /// after the ticks of a frame that `caught_up` with a stall and that it waited for.
    pub(crate) fn orders(
        &mut self,
        logged: &[InputLogged],
        caught_up: &[TicksCaughtUp],
        bots: &[BotEvents],
    ) {
        let mut sent = BTreeMap::<(PlayerSlot, Tick), usize>::new();
        for (bot, events) in bots.iter().enumerate() {
            let Some(MatchStarted { slot, .. }) = events.started else {
                self.failures.push(Failure::NoSlot { bot });
                continue;
            };
            let mut count = 0;
            for &OrdersSent { stamp, orders } in &events.sent {
                *sent.entry((slot, stamp)).or_default() += orders;
                count += orders;
            }
            if count != events.scripted {
                self.failures.push(Failure::OrderCount {
                    bot,
                    sent: count,
                    scripted: events.scripted,
                });
            }
        }
        let mut by_stamp = BTreeMap::<(PlayerSlot, Tick), usize>::new();
        for &InputLogged { slot, stamp, tick } in logged {
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

    /// Checks that the server wrote the session log, and that the verifier replayed it to the
    /// server's final hash.
    pub(crate) fn hash(&mut self, written: Option<&SessionWritten>, verified: Option<&Verified>) {
        let Some(written) = written else {
            self.failures.push(Failure::NoLog);
            return;
        };
        match verified {
            None => self.failures.push(Failure::NotVerified),
            Some(verified) if verified.hash != written.hash => {
                self.failures.push(Failure::OtherHash {
                    server: written.hash,
                    verifier: verified.hash,
                });
            }
            Some(_) => {}
        }
    }

    /// The failures found, less each that follows from another: with a server that did not
    /// succeed, the missing session log and final hash; with a verifier that did not, the final
    /// hash.
    pub(crate) fn failures(&self) -> impl Iterator<Item = &Failure> {
        let ended = |of: Process| {
            self.failures
                .iter()
                .any(|failure| matches!(failure, Failure::Ended { process, .. } if *process == of))
        };
        let server = ended(Process::Server);
        let verifier = ended(Process::Verifier);
        self.failures.iter().filter(move |failure| match failure {
            Failure::NoLog => !server,
            Failure::NotVerified => !server && !verifier,
            _ => true,
        })
    }
}

#[cfg(test)]
mod tests;
