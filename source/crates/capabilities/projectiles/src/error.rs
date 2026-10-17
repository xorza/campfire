use campfire_common::EnumAll;
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
