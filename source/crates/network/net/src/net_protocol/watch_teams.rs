use campfire_values::TeamSet;
use serde::{Deserialize, Serialize};

/// A spectator's choice of the teams whose views it watches together, in a session whose spectators
/// watch teams' views (10-replication-audiences.md, The host admits spectators, and their fog is a
/// setting); the server ignores it in one whose spectators see every unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WatchTeams(pub TeamSet);
