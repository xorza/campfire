use campfire_common::{DeclaredName, EnumAll};
use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;
use thiserror::Error;

/// Why a projectile type breaks a rule of the projectiles. Packages are untrusted, so each is an
/// expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ProjectileError {
    /// A homing projectile no faster than the mode's move speed cap, which might never catch a
    /// unit.
    #[error("a homing projectile is faster than the mode's move speed cap")]
    NotFaster,
    /// A projectile that hits nothing, and homes, stops on a hit, hits once a cast, or is wide
    /// on a line, as only one that hits can.
    #[error("a projectile that hits nothing neither homes, stops, hits once a cast nor is wide")]
    HitsNothing,
    /// It gives `gravity`, which the script API plans and the release does not read yet.
    #[error("gravity is planned, and the release does not read it yet")]
    Gravity,
}

/// Why a projectile's data is none a unit type may give: data is untrusted, so each is an expected
/// failure, which fails the section's read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ProjectileDataError {
    #[error("a projectile's speed is positive")]
    Speed,
    #[error("a projectile's width is not negative")]
    NegativeWidth,
    /// A width whose half, the radius of the circle it flies as, is past `MAX_BOUND`.
    #[error("a projectile's width is at most 4,096 m")]
    WideWidth,
    #[error("a projectile's range is not negative")]
    Range,
    /// A curve beside what only a line or a homing flight takes: a curve ends at its target and
    /// at its first hit, and falls by no gravity.
    #[error("a curve takes no `homing`, `gravity`, `range`, `stop_on_hit` or `once_per_cast`")]
    CurveOfLine,
    #[error("a curve's point is a fraction from 0 to 1 and a height up to 2,048 m either way")]
    CurvePoint,
    #[error("a curve's `follow` is not negative")]
    CurveFollow,
    #[error("a curve's `life_ms` is positive")]
    CurveLife,
}

/// Why an action's projectile delivery breaks a rule of the projectiles. Packages are untrusted,
/// so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProjectileDeliveryError {
    /// It delivers a unit type with no `projectile` section.
    #[error("unit type {0} lacks the projectile section its delivery needs")]
    NotProjectile(DeclaredName),
    /// It aims at nothing, and a projectile needs an aim.
    #[error("it aims at nothing, and a projectile needs an aim")]
    NoAim,
    /// Its projectile homes, and it aims at no unit, or launches more than one.
    #[error("a homing projectile flies one at a time, at a unit target")]
    Homing,
    /// Its projectile flies a curve, and it aims along a direction, or launches more than one.
    #[error("a curve flies one at a time, at a unit or a point")]
    Curve,
    /// It has an `on_hit` list or hook, and its projectile hits nothing.
    #[error("an `on_hit`, and its projectile hits nothing")]
    NoHit,
}

/// Why the projectiles capability refused a script call: a script gave it a value it does not
/// take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum ProjectilesRefusal {
    /// A projectile launched in the other form than its type flies: a homing or a curve type at
    /// a unit, a line type along a direction.
    OtherFlight,
}

impl CapabilityRefusal for ProjectilesRefusal {
    const CAPABILITY: Capability = Capability::Projectiles;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::OtherFlight => {
                "a homing or a curve projectile flies at a unit, and a line projectile along a direction"
            }
        }
    }
}
