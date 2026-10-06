use std::num::NonZeroU32;
use std::path::PathBuf;

use campfire_common::StateHash;
use campfire_protocol::{CertificateHash, SessionId};
use serde_json::json;

use super::*;

fn logged(slot: u32, stamp: u64, tick: u64) -> InputLogged {
    InputLogged {
        slot: PlayerSlot::new(slot),
        stamp: Tick::new(stamp),
        tick: Tick::new(tick),
    }
}

/// The process of bot `index`, of a script of 2 orders, which started in `slot` and sent and kept
/// `sent`, by stamp, and ran to its end.
fn bot(index: usize, slot: Option<u32>, sent: &[(u64, usize)]) -> BotEvents {
    BotEvents {
        bot: index,
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
        kept: sent
            .iter()
            .map(|&(stamp, orders)| OrdersSent {
                stamp: Tick::new(stamp),
                orders,
            })
            .collect(),
        scripted: 2,
        whole: true,
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
fn a_match_passes_when_every_order_lands_in_its_stamp_tick_or_waited_for_a_catch_up() {
    // Bot 0 sends its 2 orders in one message at tick 20; bot 1 one each at 20 and 50; the
    // server logs all 4, in any order, each in its stamp tick but bot 1's at 50: a frame that
    // caught up ran ticks 50 to 55, and the server read that input only after them, so it took
    // effect in 56.
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
    let caught_up = TicksCaughtUp {
        first: Tick::new(50),
        last: Tick::new(55),
    };
    verdict.orders(
        &[
            logged(1, 20, 20),
            logged(0, 20, 20),
            logged(0, 20, 20),
            logged(1, 50, 56),
        ],
        &[caught_up],
        &[
            bot(0, Some(0), &[(20, 2)]),
            bot(1, Some(1), &[(20, 1), (50, 1)]),
        ],
    );
    verdict.hash(
        SessionKind::Lan,
        Some(&written(hash("aa"))),
        Some(&verified(hash("aa"))),
    );
    verdict.process(
        Process::Server,
        ProcessOutcome::Succeeded,
        &ProcessLog::empty(Process::Server),
    );
    assert_eq!(found(&verdict), []);
}

#[test]
fn a_match_fails_by_each_flaw_it_has() {
    // No listening; slot 0's input stamped 20 applied in 21, before a frame that caught up
    // from 21, so it waited for none of it; of the 2 inputs bot 0 sent at 50,
    // the server logged 1; bot 0 sent 3 of its 2 orders; bot 1 never started; the verifier's
    // hash differs.
    let mut verdict = Verdict::default();
    verdict.listened(&[]);
    verdict.orders(
        &[logged(0, 20, 21), logged(0, 50, 50)],
        &[TicksCaughtUp {
            first: Tick::new(21),
            last: Tick::new(25),
        }],
        &[bot(0, Some(0), &[(20, 1), (50, 2)]), bot(1, None, &[])],
    );
    verdict.hash(
        SessionKind::Lan,
        Some(&written(hash("aa"))),
        Some(&verified(hash("bb"))),
    );
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
                session: SessionKind::Lan,
                host: hash("aa"),
                verifier: hash("bb")
            },
        ]
    );
    // Without the session log there is nothing to verify; with it and no verifier's hash,
    // the log did not verify.
    let mut verdict = Verdict::default();
    let lan = SessionKind::Lan;
    verdict.hash(lan, None, Some(&verified(hash("aa"))));
    verdict.hash(lan, Some(&written(hash("aa"))), None);
    let no_log = Failure::NoLog { session: lan };
    assert_eq!(
        found(&verdict),
        [no_log.clone(), Failure::NotVerified { session: lan }]
    );
    // Neither follows from a verifier that failed but the first; both follow from a server
    // that overran, which alone the verdict names.
    let empty = |process| ProcessLog::empty(process);
    let failed = ProcessOutcome::Failed { code: Some(1) };
    verdict.process(Process::Verifier, failed, &empty(Process::Verifier));
    let verifier_failed = Failure::Ended {
        process: Process::Verifier,
        outcome: failed,
    };
    assert_eq!(found(&verdict), [no_log, verifier_failed.clone()]);
    // The server that writes the log is the one the check started again.
    let again = Process::ServerAgain;
    verdict.process(again, ProcessOutcome::Overran, &empty(again));
    let server_overran = Failure::Ended {
        process: again,
        outcome: ProcessOutcome::Overran,
    };
    assert_eq!(found(&verdict), [verifier_failed, server_overran]);
}

#[test]
fn the_impostor_and_each_process_fail_by_their_own_flaws() {
    // The impostor must exit with failure and say why; one that succeeded or overran, or
    // stayed silent, fails the check.
    let failed = ProcessOutcome::Failed { code: Some(1) };
    let lost = LinkLost {
        reason: "Transport error: certificate hash mismatch".to_owned(),
    };
    let mut verdict = Verdict::default();
    verdict.impostor(failed, &[lost]);
    assert_eq!(found(&verdict), []);
    for outcome in [
        ProcessOutcome::Succeeded,
        ProcessOutcome::Overran,
        ProcessOutcome::NotStarted,
    ] {
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
    verdict.process(Process::Server, ProcessOutcome::Overran, &log);
    assert_eq!(
        found(&verdict),
        [
            Failure::Ended {
                process: Process::Server,
                outcome: ProcessOutcome::Overran
            },
            Failure::Warned {
                process: Process::Server,
                level: LogLevel::Warn,
                target: "campfire_net::lobby".to_owned(),
                fields: json!({"message": "refused a join"}),
            },
        ]
    );
}

#[test]
fn a_bot_the_check_stops_and_starts_again_counts_its_orders_once_across_both() {
    // Bot 1's first process sent 1 order, stamped 20, before the check stopped it; started
    // again, it sent both of its script's late, stamped 40. The server logged all 3, and the
    // first process sent fewer than its script with no failure.
    let stopped = BotEvents {
        whole: false,
        ..bot(1, Some(1), &[(20, 1)])
    };
    let again = bot(1, Some(1), &[(40, 2)]);
    let mut verdict = Verdict::default();
    verdict.orders(
        &[logged(1, 20, 20), logged(1, 40, 40), logged(1, 40, 40)],
        &[],
        &[stopped, again],
    );
    let empty = ProcessLog::empty(Process::Bot(1));
    verdict.stopped(Process::Bot(1), ProcessOutcome::Stopped, &empty);
    assert_eq!(found(&verdict), []);
    // One that ended before the check stopped it fails.
    verdict.stopped(Process::Bot(1), ProcessOutcome::Succeeded, &empty);
    assert_eq!(
        found(&verdict),
        [Failure::NotStopped {
            process: Process::Bot(1),
            outcome: ProcessOutcome::Succeeded
        }]
    );

    // The local session's missing log follows from its client's failure alone, not the LAN
    // server's.
    let local = SessionKind::Local;
    let mut verdict = Verdict::default();
    verdict.hash(local, None, None);
    let empty = ProcessLog::empty(Process::Server);
    verdict.process(Process::Server, ProcessOutcome::Overran, &empty);
    let server_overran = Failure::Ended {
        process: Process::Server,
        outcome: ProcessOutcome::Overran,
    };
    assert_eq!(
        found(&verdict),
        [Failure::NoLog { session: local }, server_overran.clone()]
    );
    let empty = ProcessLog::empty(Process::Local);
    verdict.process(Process::Local, ProcessOutcome::Overran, &empty);
    let local_overran = Failure::Ended {
        process: Process::Local,
        outcome: ProcessOutcome::Overran,
    };
    assert_eq!(found(&verdict), [server_overran, local_overran]);
}
