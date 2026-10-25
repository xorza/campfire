use campfire_common::Tick;
use serde::{Deserialize, Serialize};

/// A player's command of the journal, which a local server takes: a save at the boundary after the
/// next tick, a load of the latest save, or the log so far written into a report the player wrote
/// of the tick its client drew (08-sessions.md, The log is the local server's, published at the
/// report's tick).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaveCommand {
    Save,
    LoadLatest,
    Report(ReportKey),
}

/// Which of a session's reports a folder holds: the last tick the client ran, whose end it drew,
/// and how many reports of that tick the client wrote before, as a paused match draws one tick
/// for each of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportKey {
    pub tick: Tick,
    pub take: u32,
}
