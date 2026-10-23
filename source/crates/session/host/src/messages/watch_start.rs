use crate::match_clock::MatchClock;
use serde::{Deserialize, Serialize};

/// Tells a spectator that it watches the match, as the match starts or as it joins one under
/// way: the clock of the match from the tick it watches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchStart {
    pub clock: MatchClock,
}
