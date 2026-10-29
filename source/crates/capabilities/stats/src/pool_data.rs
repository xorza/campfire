use campfire_values::Stat;
use serde::Deserialize;

/// A pool as the mode declares it, in `[pools.<name>]`: the stat of its maximum, the stat of what
/// it regains a second, and the stat of the share of its maximum it regains a second besides, as
/// Zero Hour's propaganda tower heals a share of each unit's most health
/// (riders-and-upgrades.md, A pool regains a share of its maximum).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolData {
    pub max: Stat,
    pub regen: Option<Stat>,
    pub regen_share: Option<Stat>,
}
