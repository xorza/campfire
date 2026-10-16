use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;

campfire_common::enum_all! {
    /// Why the projectiles capability refused a script call: a script gave it a value it does not
    /// take.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ProjectilesRefusal {
        /// A projectile launched in the other form than its type flies: a homing type at a unit, a
        /// line type along a direction.
        OtherFlight,
    }
    const ALL;
}

impl CapabilityRefusal for ProjectilesRefusal {
    const OWNER: Capability = Capability::Projectiles;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::OtherFlight => {
                "a homing projectile flies at a unit, and a line projectile along a direction"
            }
        }
    }
}
