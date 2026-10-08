use crate::actions::amount::Amount;
use crate::actions::capability_does::CapabilityDoes;
use crate::units::modifier_id::ModifierId;
use crate::units::tag::Tag;
use crate::units::unit_type::UnitType;

/// What a listed effect does, its names resolved: an effect the action pipeline queues itself,
/// as `stats` and the core are below it, or one a capability above it queues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Does {
    Modifier {
        id: ModifierId,
        duration_ms: Option<Amount>,
    },
    Purge {
        tag: Tag,
    },
    /// A unit of the mode's type `unit_type`, despawning `duration_ms` after it spawns when given.
    Spawn {
        unit_type: UnitType,
        duration_ms: Option<Amount>,
    },
    Capability(CapabilityDoes),
}
