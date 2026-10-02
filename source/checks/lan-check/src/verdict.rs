use std::collections::{BTreeMap, BTreeSet};

use campfire_log::Level;
use campfire_math::{PlayerSlot, Tick};
use campfire_net::{InputLogged, LinkLost, Listening, MatchStarted, OrdersSent, SessionWritten};
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
    /// server logged exactly the inputs the bots sent, each taking effect in its stamp tick.
    pub(crate) fn orders(&mut self, logged: &[InputLogged], bots: &[BotEvents]) {
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
            if tick != stamp {
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
mod tests {
    use std::num::NonZeroU32;
    use std::path::PathBuf;

    use campfire_protocol::{CertificateHash, SessionId};
    use campfire_sim::StateHash;
    use serde_json::json;

    use super::*;

    fn logged(slot: u32, stamp: u64, tick: u64) -> InputLogged {
        InputLogged {
            slot: PlayerSlot::new(slot),
            stamp: Tick::new(stamp),
            tick: Tick::new(tick),
        }
    }

    /// A bot of a script of 2 orders, which started in `slot` and sent `sent`, by stamp.
    fn bot(slot: Option<u32>, sent: &[(u64, usize)]) -> BotEvents {
        BotEvents {
            started: slot.map(|slot| MatchStarted {
                slot: PlayerSlot::new(slot),
                start_tick: 5,
            }),
            sent: sent
                .iter()
                .map(|&(stamp, orders)| OrdersSent {
                    stamp: Tick::new(stamp),
                    orders,
                })
                .collect(),
            scripted: 2,
        }
    }

    fn found(verdict: &Verdict) -> Vec<Failure> {
        verdict.failures().cloned().collect()
    }

    fn hash(byte: &str) -> StateHash {
        byte.repeat(32).parse().unwrap()
    }

    fn written(hash: StateHash) -> SessionWritten {
        SessionWritten {
            session: SessionId::new([1; 32]),
            file: PathBuf::from("s.campfire-log"),
            hash,
        }
    }

    fn verified(hash: StateHash) -> Verified {
        Verified {
            file: PathBuf::from("s.campfire-log"),
            hash,
        }
    }

    #[test]
    fn a_match_passes_when_every_order_lands_in_its_stamp_tick_and_the_hashes_agree() {
        // Bot 0 sends its 2 orders in one message at tick 20; bot 1 one each at 20 and 50; the
        // server logs all 4, each in its stamp tick, in any order.
        let listening = Listening {
            certificate: CertificateHash::new([7; 32]),
            server_key: "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
                .parse()
                .unwrap(),
            tick_hz: NonZeroU32::new(30).unwrap(),
            join: String::new(),
        };
        let mut verdict = Verdict::default();
        verdict.listened(&[listening]);
        verdict.orders(
            &[
                logged(1, 20, 20),
                logged(0, 20, 20),
                logged(0, 20, 20),
                logged(1, 50, 50),
            ],
            &[bot(Some(0), &[(20, 2)]), bot(Some(1), &[(20, 1), (50, 1)])],
        );
        verdict.hash(Some(&written(hash("aa"))), Some(&verified(hash("aa"))));
        verdict.process(
            Process::Server,
            Outcome::Succeeded,
            &ProcessLog::empty(Process::Server),
        );
        assert_eq!(found(&verdict), []);
    }

    #[test]
    fn a_match_fails_by_each_flaw_it_has() {
        // No listening; slot 0's input stamped 20 applied in 21; of the 2 inputs bot 0 sent at 50,
        // the server logged 1; bot 0 sent 3 of its 2 orders; bot 1 never started; the verifier's
        // hash differs.
        let mut verdict = Verdict::default();
        verdict.listened(&[]);
        verdict.orders(
            &[logged(0, 20, 21), logged(0, 50, 50)],
            &[bot(Some(0), &[(20, 1), (50, 2)]), bot(None, &[])],
        );
        verdict.hash(Some(&written(hash("aa"))), Some(&verified(hash("bb"))));
        let slot = PlayerSlot::new(0);
        assert_eq!(
            found(&verdict),
            [
                Failure::NeverListened,
                Failure::OrderCount {
                    bot: 0,
                    sent: 3,
                    scripted: 2
                },
                Failure::NoSlot { bot: 1 },
                Failure::Moved {
                    slot,
                    stamp: Tick::new(20),
                    tick: Tick::new(21)
                },
                Failure::Unlogged {
                    slot,
                    stamp: Tick::new(50),
                    sent: 2,
                    logged: 1
                },
                Failure::OtherHash {
                    server: hash("aa"),
                    verifier: hash("bb")
                },
            ]
        );
        // Without the session log there is nothing to verify; with it and no verifier's hash,
        // the log did not verify.
        let mut verdict = Verdict::default();
        verdict.hash(None, Some(&verified(hash("aa"))));
        verdict.hash(Some(&written(hash("aa"))), None);
        assert_eq!(found(&verdict), [Failure::NoLog, Failure::NotVerified]);
        // Neither follows from a verifier that failed but the first; both follow from a server
        // that overran, which alone the verdict names.
        let empty = |process| ProcessLog::empty(process);
        let failed = Outcome::Failed { code: Some(1) };
        verdict.process(Process::Verifier, failed, &empty(Process::Verifier));
        let verifier_failed = Failure::Ended {
            process: Process::Verifier,
            outcome: failed,
        };
        assert_eq!(found(&verdict), [Failure::NoLog, verifier_failed.clone()]);
        verdict.process(Process::Server, Outcome::Overran, &empty(Process::Server));
        let server_overran = Failure::Ended {
            process: Process::Server,
            outcome: Outcome::Overran,
        };
        assert_eq!(found(&verdict), [verifier_failed, server_overran]);

        // The impostor must exit with failure and say why; one that succeeded or overran, or
        // stayed silent, fails the check.
        let lost = LinkLost {
            reason: "Transport error: certificate hash mismatch".to_owned(),
        };
        let mut verdict = Verdict::default();
        verdict.impostor(failed, &[lost]);
        assert_eq!(found(&verdict), []);
        for outcome in [Outcome::Succeeded, Outcome::Overran, Outcome::NotStarted] {
            let mut verdict = Verdict::default();
            verdict.impostor(outcome, &[]);
            assert_eq!(
                found(&verdict),
                [
                    Failure::ImpostorNotRefused { outcome },
                    Failure::ImpostorSilent
                ],
                "{outcome:?}"
            );
        }
        // A process that did not end with success fails, and so does each warning or error it
        // logged; lines below a warning do not.
        let text = [
            r#"{"level":"INFO","target":"a","fields":{"message":"fine"}}"#,
            r#"{"level":"WARN","target":"campfire_net::lobby","fields":{"message":"refused a join"}}"#,
            "",
        ]
        .join("\n");
        let log = ProcessLog::parse(Process::Server, &text).unwrap();
        let mut verdict = Verdict::default();
        verdict.process(Process::Server, Outcome::Overran, &log);
        assert_eq!(
            found(&verdict),
            [
                Failure::Ended {
                    process: Process::Server,
                    outcome: Outcome::Overran
                },
                Failure::Warned {
                    process: Process::Server,
                    level: Level::Warn,
                    target: "campfire_net::lobby".to_owned(),
                    fields: json!({"message": "refused a join"}),
                },
            ]
        );
    }
}
