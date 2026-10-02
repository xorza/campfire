use std::fmt;

use campfire_log::Level;
use campfire_math::{PlayerSlot, Tick};
use campfire_sim::StateHash;
use serde_json::Value;

use crate::outcome::Outcome;
use crate::process::Process;

/// One way a LAN match failed the check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    /// A process did not end with success; never with `Outcome::Succeeded`.
    Ended { process: Process, outcome: Outcome },
    /// A process logged a warning or an error: a refused join or input among them.
    Warned {
        process: Process,
        level: Level,
        target: String,
        fields: Value,
    },
    /// The server never said it listens.
    NeverListened,
    /// A bot never learned its slot: it did not join, or its match did not start.
    NoSlot { bot: usize },
    /// A bot sent another number of orders than its script holds.
    OrderCount {
        bot: usize,
        sent: usize,
        scripted: usize,
    },
    /// The server logged another number of inputs of a slot and stamp than its bot sent.
    Unlogged {
        slot: PlayerSlot,
        stamp: Tick,
        sent: usize,
        logged: usize,
    },
    /// The server applied an input in another tick than its stamp.
    Moved {
        slot: PlayerSlot,
        stamp: Tick,
        tick: Tick,
    },
    /// The impostor bot did not exit with failure: it linked, or it still ran at the deadline.
    ImpostorNotRefused { outcome: Outcome },
    /// The impostor bot did not log why its link failed.
    ImpostorSilent,
    /// The server did not say it wrote the session log.
    NoLog,
    /// The verifier did not give a final hash.
    NotVerified,
    /// The verifier's final hash is not the server's.
    OtherHash {
        server: StateHash,
        verifier: StateHash,
    },
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Ended { process, outcome } => write!(f, "{process} {outcome}"),
            Failure::Warned {
                process,
                level,
                target,
                fields,
            } => write!(f, "{process} logged {level:?} from {target}: {fields}"),
            Failure::NeverListened => f.write_str("the server never listened"),
            Failure::NoSlot { bot } => write!(f, "bot {bot} never learned its slot"),
            Failure::OrderCount {
                bot,
                sent,
                scripted,
            } => write!(f, "bot {bot} sent {sent} orders of its script's {scripted}"),
            Failure::Unlogged {
                slot,
                stamp,
                sent,
                logged,
            } => write!(
                f,
                "slot {} sent {sent} inputs stamped {}, and the server logged {logged}",
                slot.get(),
                stamp.get()
            ),
            Failure::Moved { slot, stamp, tick } => write!(
                f,
                "the input of slot {} stamped {} took effect in tick {}",
                slot.get(),
                stamp.get(),
                tick.get()
            ),
            Failure::ImpostorNotRefused { outcome } => {
                write!(
                    f,
                    "the impostor bot {outcome}, where it must exit with failure"
                )
            }
            Failure::ImpostorSilent => f.write_str("the impostor bot did not log its lost link"),
            Failure::NoLog => f.write_str("the server did not write the session log"),
            Failure::NotVerified => f.write_str("the verifier gave no final hash"),
            Failure::OtherHash { server, verifier } => write!(
                f,
                "the verifier's final hash {verifier} is not the server's {server}"
            ),
        }
    }
}
