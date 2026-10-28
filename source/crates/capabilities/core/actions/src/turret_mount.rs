use campfire_declarations::PerId;
use campfire_values::UnitType;

/// That a unit type mounts a turret, which the capability that turns turrets books for each type of
/// one, so the scan of a turret target, which knows no turret, reads it (04-capabilities/combat.md,
/// A turret has its own target).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurretMount;

impl PerId<UnitType> for TurretMount {
    const SCRIPTS_ONLY: bool = false;
}
