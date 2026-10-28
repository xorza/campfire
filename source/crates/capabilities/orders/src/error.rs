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

/// Why a unit type's idle scan does not load. Packages are untrusted, so each is an expected
/// failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AcquireError {
    /// The scan's period is too long to count in ticks.
    #[error("scan period: {0}")]
    Time(#[from] TimeTooLarge),
    /// The scan names no filter, so it would take no target.
    #[error("an `acquire` names no filter of its targets")]
    NoTargets,
    /// The type has no weapon to attack what it finds.
    #[error("a type that scans has a weapon")]
    NoWeapon,
    /// The type builds or gathers, whose loops stand it idle between their steps.
    #[error("a type that scans neither builds nor gathers")]
    Works,
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

/// Why the mode's `[retaliation]` or a unit type's `retaliate` does not load. Packages are
/// untrusted, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RetaliationError {
    /// The chase is too long to count in ticks.
    #[error("retaliation's chase: {0}")]
    Time(#[from] TimeTooLarge),
    /// A distance, a radius, a leash or a reach is below 0.
    #[error("a retaliation's distance, radius, leash and reach are at least 0")]
    Negative,
    /// The type has no weapon to answer with.
    #[error("a type that retaliates has a weapon")]
    NoWeapon,
    /// The type does not walk, so it cannot chase.
    #[error("a type that retaliates walks")]
    NoWalk,
}
