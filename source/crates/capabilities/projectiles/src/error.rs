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
    /// An attack whose projectile neither homes nor flies a curve, as a weapon's does.
    #[error("a weapon's delivery is a homing projectile or a curve")]
    Weapon,
    /// A scatter of a projectile that flies no curve, which the scatter aims.
    #[error("a scatter aims its delivery's curve")]
    Scatter,
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
    const OWNER: Capability = Capability::Projectiles;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::OtherFlight => {
                "a homing or a curve projectile flies at a unit, and a line projectile along a direction"
            }
        }
    }
}
