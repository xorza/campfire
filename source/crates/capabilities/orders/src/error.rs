use campfire_common::EnumAll;
use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;
use thiserror::Error;

/// Why a unit type's AI does not load. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AiError {
    /// The think period is too long to count in ticks.
    #[error("think period too long to count in ticks")]
    TimeTooLarge,
    /// The script has no `on_think(ctx, unit)`.
    #[error("AI script has no on_think(ctx, unit)")]
    NoThink,
}

/// Why the orders capability refused a script call: a script gave it a value it does not take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum OrdersRefusal {
    /// An attack order on a unit that is not a living enemy.
    NotAnEnemy,
    /// An AI order for another unit than the one that thinks.
    OtherUnit,
}

impl CapabilityRefusal for OrdersRefusal {
    const OWNER: Capability = Capability::Orders;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::NotAnEnemy => "target is not a living enemy",
            Self::OtherUnit => "an AI orders only the unit that thinks",
        }
    }
}
