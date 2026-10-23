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
    /// is a projectile launched at a unit.
    #[error("a weapon's delivery is a projectile launched at a unit")]
    Delivery,
    /// A splash, and no projectile delivery for it to go off where it ends.
    #[error("a splash goes off where a projectile of its delivery ends")]
    Splash,
    /// A splash's outer ring whose amount is not a number at least 0 at a rank.
    #[error("a splash's outer ring deals a number at least 0")]
    OuterAmount,
    /// A scatter, and no projectile delivery that flies to a point for it to aim.
    #[error("a scatter aims a projectile of its delivery that flies to a point")]
    Scatter,
    /// A speed, which delays the hit of a weapon of no delivery, and a delivery.
    #[error("a speed delays the hit of a weapon of no delivery")]
    Speed,
    /// A rate bonus beside the stat `rate`, which already speeds the weapon.
    #[error("a rate bonus speeds a weapon's own period")]
    RateBonus,
    /// A pre-attack whose time is not a whole number of milliseconds at least 0 at a rank.
    #[error("a pre-attack lasts a whole number of milliseconds at least 0")]
    PreAttack,
    /// A damage bonus beside the stat `damage`, which already gives the weapon's damage.
    #[error("a damage bonus scales a weapon's own amount")]
    DamageBonus,
    /// A splash's outer ring beside the stat `damage`, whose own amount no bonus would scale as
    /// the weapon's.
    #[error("a splash's outer ring goes with a weapon's own amount")]
    OuterBesideStat,
    /// A streak of no stage, or of stages whose shots do not strictly grow.
    #[error("a streak has stages, their shots strictly growing")]
    Stages,
}

/// Why the combat capability refused a script call: a script gave it a value it does not take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum CombatRefusal {
    NegativeDamage,
    /// A heal of a negative amount.
    NegativeHeal,
    /// An attack field or an extra attack of a unit that has no attack.
    NoAttack,
    /// A unit to respawn that is alive.
    RespawnAlive,
    /// A unit to respawn whose type despawns when it dies.
    RespawnDespawns,
}

impl CapabilityRefusal for CombatRefusal {
    const CAPABILITY: Capability = Capability::Combat;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::NegativeDamage => "damage is negative",
            Self::NegativeHeal => "amount to heal is negative",
            Self::NoAttack => "unit has no attack",
            Self::RespawnAlive => "unit to respawn is alive",
            Self::RespawnDespawns => "unit to respawn despawns when it dies",
        }
    }
}
