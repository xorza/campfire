use std::fmt;

use campfire_log::Level;
use campfire_math::PlayerSlot;
use campfire_sim::{StateHash, Tick};
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
            Failure::Ended { process, outcome } => match outcome {
                Outcome::Succeeded => unreachable!("{process} succeeded, which is no failure"),
                Outcome::Failed { code: Some(code) } => write!(f, "{process} exited with {code}"),
                Outcome::Failed { code: None } => write!(f, "{process} was killed by a signal"),
                Outcome::Overran => write!(f, "{process} still ran at the deadline"),
                Outcome::NotStarted => write!(f, "{process} did not start"),
            },
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
            Failure::NoLog => f.write_str("the server did not write the session log"),
            Failure::NotVerified => f.write_str("the verifier gave no final hash"),
            Failure::OtherHash { server, verifier } => write!(
                f,
                "the verifier's final hash {verifier} is not the server's {server}"
            ),
        }
    }
}
