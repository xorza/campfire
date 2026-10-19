use campfire_common::EnumAll;
use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;
use campfire_values::TimeTooLarge;
use thiserror::Error;

/// Why a unit type's AI does not load. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AiError {
    /// The think period is too long to count in ticks.
    #[error("think period: {0}")]
    Time(#[from] TimeTooLarge),
    /// The script has no `on_think(ctx, unit)`.
    #[error("AI script has no on_think(ctx, unit)")]
    NoThink,
    /// An avatar's type gives `orders`, and bots play an avatar.
    #[error("an avatar takes no `orders`: bots play it")]
    AvatarOrders,
}

/// Why the orders capability refused a script call: a script gave it a value it does not take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum OrdersRefusal {
    /// An attack order on a unit that is not a living enemy.
    NotAnEnemy,
    /// An attack order on a target none of the unit's weapons selects.
    NoAttack,
}

impl CapabilityRefusal for OrdersRefusal {
    const CAPABILITY: Capability = Capability::Orders;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::NotAnEnemy => "target is not a living enemy",
            Self::NoAttack => "unit has no attack for the target",
        }
    }
}
