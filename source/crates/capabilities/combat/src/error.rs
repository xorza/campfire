use campfire_common::EnumAll;
use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;
use thiserror::Error;

/// Why the mode's `[combat]` breaks a rule of combat. Packages are untrusted, so each is an
/// expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CombatDataError {
    /// No damage kinds for its damage.
    #[error("the mode declares combat, and no damage kinds")]
    NoDamageKinds,
    /// No life pool for its units to die at the end of.
    #[error("the mode declares combat, and no life pool")]
    NoLifePool,
    /// An attack that aims at something but a unit.
    #[error("an attack aims at a unit")]
    AttackAims,
    /// An attack whose range is global at a rank, where it is in meters.
    #[error("an attack's range is in meters")]
    GlobalAttack,
}

/// Why the combat capability refused a script call: a script gave it a value it does not take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum CombatRefusal {
    NegativeDamage,
    /// A heal of a negative amount.
    NegativeHeal,
}

impl CapabilityRefusal for CombatRefusal {
    const OWNER: Capability = Capability::Combat;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::NegativeDamage => "damage is negative",
            Self::NegativeHeal => "amount to heal is negative",
        }
    }
}
