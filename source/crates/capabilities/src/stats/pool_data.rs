use serde::Deserialize;

use crate::values::stat::Stat;

/// A pool as the mode declares it, in `[pools.<name>]`: the stat of its maximum, and the stat
/// of what it regains a second.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolData {
    pub max: Stat,
    pub regen: Option<Stat>,
}
