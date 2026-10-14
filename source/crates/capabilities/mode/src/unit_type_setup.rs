use campfire_values::{ModifierId, UnitType};

/// A unit type the mode spawns, loaded: a type of the mode's `units.toml`, or an avatar's. The
/// books hold its parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitTypeSetup {
    pub unit_type: UnitType,
    /// The modifier it holds from its spawn on, from itself.
    pub passive: Option<ModifierId>,
}
