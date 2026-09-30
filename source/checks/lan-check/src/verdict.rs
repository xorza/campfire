use std::collections::{BTreeMap, BTreeSet};

use campfire_math::PlayerSlot;
use campfire_sim::Tick;

use crate::event::Level;
use crate::failure::Failure;
use crate::known_event::KnownEvent;
use crate::outcome::Outcome;
use crate::process::Process;
use crate::process_log::ProcessLog;

/// What the check found wrong with a LAN match, from the processes' outcomes and logs; nothing
/// when the match passed.
#[derive(Debug, Default)]
pub(crate) struct Verdict {
    failures: Vec<Failure>,
}

/// A bot's log, and the number of orders its script holds.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BotLog<'a> {
    pub(crate) log: &'a ProcessLog,
    pub(crate) scripted: usize,
}

impl Verdict {
    /// Checks that `process` succeeded, and logged no warning or error.
    pub(crate) fn process(&mut self, process: Process, outcome: Outcome, log: &ProcessLog) {
        if outcome != Outcome::Succeeded {
            self.failures.push(Failure::Ended { process, outcome });
        }
        for event in log.events() {
            if event.level >= Level::Warn {
                self.failures.push(Failure::Warned {
                    process,
                    level: event.level,
                    target: event.target.clone(),
                    fields: event.fields.clone(),
                });
            }
        }
    }

    /// Checks that the server listened.
    pub(crate) fn listened(&mut self, server: &ProcessLog) {
        let listened = server
            .known()
            .any(|event| matches!(event, KnownEvent::Listening { .. }));
        if !listened {
            self.failures.push(Failure::NeverListened);
        }
    }

    /// Checks that each bot learned its slot and sent every order of its script, and that the
    /// server logged exactly the inputs the bots sent, each taking effect in its stamp tick.
    pub(crate) fn orders(&mut self, server: &ProcessLog, bots: &[BotLog<'_>]) {
        let mut sent = BTreeMap::<(PlayerSlot, Tick), usize>::new();
        for (bot, &BotLog { log, scripted }) in bots.iter().enumerate() {
            let slot = log.known().find_map(|event| match *event {
                KnownEvent::MatchStarted { slot } => Some(slot),
                _ => None,
            });
            let Some(slot) = slot else {
                self.failures.push(Failure::NoSlot { bot });
                continue;
            };
            let mut count = 0;
            for event in log.known() {
                if let KnownEvent::SentOrders { stamp, orders } = *event {
                    *sent.entry((slot, stamp)).or_default() += orders;
                    count += orders;
                }
            }
            if count != scripted {
                self.failures.push(Failure::OrdersSent {
                    bot,
                    sent: count,
                    scripted,
                });
            }
        }
        let mut logged = BTreeMap::<(PlayerSlot, Tick), usize>::new();
        for event in server.known() {
            if let KnownEvent::LoggedInput { slot, stamp, tick } = *event {
                *logged.entry((slot, stamp)).or_default() += 1;
                if tick != stamp {
                    self.failures.push(Failure::Moved { slot, stamp, tick });
                }
            }
        }
        let keys: BTreeSet<_> = sent.keys().chain(logged.keys()).collect();
        for &(slot, stamp) in keys {
            let sent = sent.get(&(slot, stamp)).copied().unwrap_or(0);
            let logged = logged.get(&(slot, stamp)).copied().unwrap_or(0);
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
    pub(crate) fn hash(&mut self, server: &ProcessLog, verifier: &ProcessLog) {
        let written = server.known().find_map(|event| match *event {
            KnownEvent::WroteLog { hash, .. } => Some(hash),
            _ => None,
        });
        let Some(server_hash) = written else {
            self.failures.push(Failure::NoLog);
            return;
        };
        let replayed = verifier.known().find_map(|event| match *event {
            KnownEvent::Verified { hash } => Some(hash),
            _ => None,
        });
        match replayed {
            None => self.failures.push(Failure::NotVerified),
            Some(hash) if hash != server_hash => self.failures.push(Failure::OtherHash {
                server: server_hash,
                verifier: hash,
            }),
            Some(_) => {}
        }
    }

    pub(crate) fn failures(&self) -> &[Failure] {
        &self.failures
    }
}

#[cfg(test)]
mod tests {
    use campfire_sim::StateHash;
    use serde_json::{Value, json};

    use super::*;

    /// The x coordinate of secp256k1's generator: a valid key.
    const KEY: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    fn line(level: &str, target: &str, fields: &Value) -> String {
        format!(
            "{}\n",
            json!({"level": level, "target": target, "fields": fields})
        )
    }

    fn log(lines: &[String]) -> ProcessLog {
        ProcessLog::parse(Process::Server, &lines.concat()).unwrap()
    }

    fn listening() -> String {
        let fields = json!({
            "message": "listening; players join with the command in `join`",
            "certificate": "07".repeat(32),
            "server_key": KEY,
            "join": "campfire-client …",
        });
        line("INFO", "campfire_server", &fields)
    }

    fn logged(slot: u32, stamp: u64, tick: u64) -> String {
        let fields =
            json!({"message": "logged an input", "slot": slot, "stamp": stamp, "tick": tick});
        line("DEBUG", "campfire_net::sim_server", &fields)
    }

    fn wrote(hash: &str) -> String {
        let fields = json!({
            "message": "every player left; wrote the session log, which `campfire-verifier \
                        <packages directory> <file>` replays to the same hash",
            "session": "00",
            "file": "s.campfire-log",
            "hash": hash,
        });
        line("INFO", "campfire_server", &fields)
    }

    fn started(slot: u32) -> String {
        let fields = json!({"message": "the match started", "slot": slot, "start_tick": 5});
        line("INFO", "campfire_net::sim_client", &fields)
    }

    fn sent(stamp: u64, orders: usize) -> String {
        let fields = json!({"message": "sent orders", "stamp": stamp, "orders": orders});
        line("DEBUG", "campfire_net::sim_client", &fields)
    }

    fn verified(hash: &str) -> String {
        let fields = json!({"message": "the log verifies; its final state hash", "hash": hash});
        line("INFO", "campfire_verifier", &fields)
    }

    fn hash(byte: &str) -> StateHash {
        byte.repeat(32).parse().unwrap()
    }

    /// The verdict on a server, two bots of `scripted` orders each and a verifier, all succeeded.
    fn judge(server: &ProcessLog, bots: [&ProcessLog; 2], verifier: &ProcessLog) -> Vec<Failure> {
        let mut verdict = Verdict::default();
        verdict.process(Process::Server, Outcome::Succeeded, server);
        verdict.listened(server);
        let bots = bots.map(|log| BotLog { log, scripted: 2 });
        verdict.orders(server, &bots);
        verdict.hash(server, verifier);
        verdict.failures
    }

    #[test]
    fn a_match_passes_when_every_order_lands_in_its_stamp_tick_and_the_hashes_agree() {
        // Bot 0 sends its 2 orders in one message at tick 20; bot 1 one each at 20 and 50; the
        // server logs all 4, each in its stamp tick, in any order.
        let server = log(&[
            listening(),
            logged(1, 20, 20),
            logged(0, 20, 20),
            logged(0, 20, 20),
            logged(1, 50, 50),
            wrote(&"aa".repeat(32)),
        ]);
        let bot_0 = log(&[started(0), sent(20, 2)]);
        let bot_1 = log(&[started(1), sent(20, 1), sent(50, 1)]);
        let verifier = log(&[verified(&"aa".repeat(32))]);
        assert_eq!(judge(&server, [&bot_0, &bot_1], &verifier), []);
    }

    #[test]
    fn a_match_fails_by_each_flaw_it_has() {
        // No listening; a warning; slot 0's input stamped 20 applied in 21; of the 2 inputs bot 0
        // sent at 50, the server logged 1; bot 0 sent 3 of its 2 orders; bot 1 never started.
        let warning = line(
            "WARN",
            "campfire_net::lobby",
            &json!({"message": "refused a join", "link": "1v0"}),
        );
        let server = log(&[
            warning,
            logged(0, 20, 21),
            logged(0, 50, 50),
            wrote(&"aa".repeat(32)),
        ]);
        let bot_0 = log(&[started(0), sent(20, 1), sent(50, 2)]);
        let bot_1 = log(&[]);
        let verifier = log(&[verified(&"bb".repeat(32))]);
        let slot = PlayerSlot::new(0);
        assert_eq!(
            judge(&server, [&bot_0, &bot_1], &verifier),
            [
                Failure::Warned {
                    process: Process::Server,
                    level: Level::Warn,
                    target: "campfire_net::lobby".to_owned(),
                    fields: json!({"message": "refused a join", "link": "1v0"}),
                },
                Failure::NeverListened,
                Failure::OrdersSent {
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
        verdict.hash(&log(&[]), &verifier);
        verdict.hash(&server, &log(&[]));
        assert_eq!(verdict.failures, [Failure::NoLog, Failure::NotVerified]);
        // A process that did not end with success fails, whatever it logged.
        let mut verdict = Verdict::default();
        verdict.process(Process::Bot(1), Outcome::Overran, &log(&[]));
        assert_eq!(
            verdict.failures,
            [Failure::Ended {
                process: Process::Bot(1),
                outcome: Outcome::Overran
            }]
        );
    }
}
