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

impl UnitState {
    pub const ALL: [UnitState; 8] = [
        UnitState::Stunned,
        UnitState::Rooted,
        UnitState::Silenced,
        UnitState::Disarmed,
        UnitState::Airborne,
        UnitState::Stealthed,
        UnitState::Untargetable,
        UnitState::SlowImmune,
    ];

    /// The state as data names it.
    pub const fn name(self) -> &'static str {
        match self {
            UnitState::Stunned => "stunned",
            UnitState::Rooted => "rooted",
            UnitState::Silenced => "silenced",
            UnitState::Disarmed => "disarmed",
            UnitState::Airborne => "airborne",
            UnitState::Stealthed => "stealthed",
            UnitState::Untargetable => "untargetable",
            UnitState::SlowImmune => "slow_immune",
        }
    }
}
