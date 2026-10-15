use serde::Deserialize;

/// When a build starts, as its `start` names it: as its builder comes in range, as StarCraft II's
/// SCV builds, or in the tick its order applies, wherever its builder stands, as Zero Hour's
/// dozer places its building and then drives to it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildStart {
    #[default]
    Arrival,
    Order,
}
