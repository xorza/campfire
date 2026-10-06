use campfire_common::{PlayerSlot, StateHash, Tick};
use campfire_log::LogLevel;
use derive_more::Display;
use serde_json::Value;

use crate::process::Process;
use crate::process_outcome::ProcessOutcome;
use crate::session_kind::SessionKind;

/// One way a LAN match failed the check.
#[derive(Debug, Display, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    /// A process did not end with success; never with `ProcessOutcome::Succeeded`.
    #[display("{process} {outcome}")]
    Ended {
        process: Process,
        outcome: ProcessOutcome,
    },
    /// The process the check stops mid-match ended before it, as `outcome` says.
    #[display("{process} {outcome}, before the check stopped it")]
    NotStopped {
        process: Process,
        outcome: ProcessOutcome,
    },
    /// A process logged a warning or an error: a refused join or input among them.
    #[display("{process} logged {level:?} from {target}: {fields}")]
    Warned {
        process: Process,
        level: LogLevel,
        target: String,
        fields: Value,
    },
    /// The server never said it listens.
    #[display("the server never listened")]
    NeverListened,
    /// A bot never learned its slot: it did not join, or its match did not start.
    #[display("bot {bot} never learned its slot")]
    NoSlot { bot: usize },
    /// A bot sent another number of orders than its script holds.
    #[display("bot {bot} sent {sent} orders of its script's {scripted}")]
    OrderCount {
        bot: usize,
        sent: usize,
        scripted: usize,
    },
    /// The server logged another number of inputs of a slot and stamp than its bot sent.
    #[display("slot {} sent {sent} inputs stamped {}, and the server logged {logged}", slot.get(), stamp.get())]
    Unlogged {
        slot: PlayerSlot,
        stamp: Tick,
        sent: usize,
        logged: usize,
    },
    /// The server applied an input in another tick than its stamp.
    #[display("the input of slot {} stamped {} took effect in tick {}", slot.get(), stamp.get(), tick.get())]
    Moved {
        slot: PlayerSlot,
        stamp: Tick,
        tick: Tick,
    },
    /// The impostor bot did not exit with failure: it linked, or it still ran at the deadline.
    #[display("the impostor bot {outcome}, where it must exit with failure")]
    ImpostorNotRefused { outcome: ProcessOutcome },
    /// The impostor bot did not log why its link failed.
    #[display("the impostor bot did not log its lost link")]
    ImpostorSilent,
    /// The host of the session did not say it wrote the session log.
    #[display("{session} did not write the session log")]
    NoLog { session: SessionKind },
    /// The verifier did not give a final hash.
    #[display("the verifier of {session}'s log gave no final hash")]
    NotVerified { session: SessionKind },
    /// The verifier's final hash is not the host's.
    #[display("the verifier's final hash {verifier} is not {session}'s {host}")]
    OtherHash {
        session: SessionKind,
        host: StateHash,
        verifier: StateHash,
    },
}
