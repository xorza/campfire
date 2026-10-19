use campfire_common::EnumAll;
use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;
use campfire_values::TimeTooLarge;
use thiserror::Error;

/// Why the mode's `[combat]` or an attack breaks a rule of combat. Packages are untrusted, so
/// each is an expected failure.
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
    /// A weapon's own period that is not whole milliseconds above 0 at a rank.
    #[error("a weapon's own period is whole milliseconds above 0")]
    Period,
    /// A weapon's own damage that is not a number at least 0 at a rank.
    #[error("a weapon's own damage is a number at least 0")]
    Amount,
    /// A weapon's clip that is not shots of 1 to 65,535 and a reload of whole milliseconds at a
    /// rank.
    #[error("a weapon's clip is shots of 1 to 65,535 and a reload of whole milliseconds")]
    Clip,
    /// A time of a weapon too large to count in ticks at the match's rate.
    #[error(transparent)]
    Time(#[from] TimeTooLarge),
    /// A weapon whose delivery is an area or a projectile that flies in a line, where a weapon's
    /// is a homing projectile or a curve.
    #[error("a weapon's delivery is a homing projectile or a curve")]
    Delivery,
    /// A splash, and no projectile delivery for it to go off where it ends.
    #[error("a splash goes off where a projectile of its delivery ends")]
    Splash,
    /// A scatter, and no projectile delivery that flies a curve for it to aim.
    #[error("a scatter aims a curve of its delivery")]
    Scatter,
    /// A speed, which delays the hit of a weapon of no delivery, and a delivery.
    #[error("a speed delays the hit of a weapon of no delivery")]
    Speed,
}

/// Why the combat capability refused a script call: a script gave it a value it does not take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum CombatRefusal {
    NegativeDamage,
    /// A heal of a negative amount.
    NegativeHeal,
    /// An attack field or an extra attack of a unit that has no attack.
    NoAttack,
}

impl CapabilityRefusal for CombatRefusal {
    const CAPABILITY: Capability = Capability::Combat;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::NegativeDamage => "damage is negative",
            Self::NegativeHeal => "amount to heal is negative",
            Self::NoAttack => "unit has no attack",
        }
    }
}
