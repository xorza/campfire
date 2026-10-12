use campfire_common::Tick;
use serde::{Deserialize, Serialize};

/// Tells a spectator that it watches the match, as the match starts or as it joins one under
/// way: the Lightyear tick of sim tick `first`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WatchStart {
    /// The Lightyear tick that is sim tick `first`.
    pub start_tick: u32,
    pub first: Tick,
}
