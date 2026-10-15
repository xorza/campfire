use campfire_script::{ApiBuilder, ApiOwner, MemberSpec, plain_script_arg};
use campfire_sim::StableId;
use campfire_values::UnitType;

use crate::units::unit_state_access::UnitStateAccess;

/// A unit a call creates, `NewUnit` in scripts: it spawns once the call applies, with the id the
/// call took for it, so the call can give it to `grant` and write its state, but reads none of
/// its other fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewUnit {
    pub id: StableId,
    pub unit_type: UnitType,
}

impl NewUnit {
    /// The `NewUnit` handle's fields.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.ty::<NewUnit>("NewUnit").bind(
            MemberSpec::field(
                ApiOwner::NewUnit,
                "state",
                "its script state, by name, at its type's defaults but what the call wrote, which applies as it spawns",
            ),
            |unit: &mut NewUnit| UnitStateAccess::of_new(unit.id, unit.unit_type),
        );
    }
}

plain_script_arg!(NewUnit);
