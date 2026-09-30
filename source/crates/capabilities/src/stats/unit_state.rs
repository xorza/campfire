use serde::Deserialize;

/// A state a modifier puts its carrier in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitState {
    Stunned,
    Rooted,
    Silenced,
    Disarmed,
    Airborne,
    Stealthed,
    Untargetable,
    SlowImmune,
}
