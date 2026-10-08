use campfire_sim::Capability;

use crate::actions::effect_data::EffectTo;
use crate::actions::effect_lists::{Amount, LaunchId};
use crate::stats::pool_id::PoolId;
use crate::units::track_id::TrackId;
use crate::units::unit_type::UnitType;
use crate::values::damage_kind::DamageKind;

/// What a listed effect of a capability above the action pipeline does, its names resolved,
/// which that capability queues as it registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapabilityDoes {
    Damage {
        amount: Amount,
        kind: DamageKind,
    },
    Heal {
        amount: Amount,
    },
    Restore {
        pool: PoolId,
        amount: Amount,
    },
    Xp {
        track: TrackId,
        amount: Amount,
    },
    /// An area of the type `area`, which runs the lists of `launch`.
    Launch {
        area: UnitType,
        launch: LaunchId,
    },
    /// A dash at `speed` to the unit `to` names.
    Dash {
        to: EffectTo,
        speed: Amount,
    },
    /// A knock back `distance` away from the unit `from` names, over `ms`.
    KnockBack {
        from: EffectTo,
        distance: Amount,
        ms: Amount,
    },
}

impl CapabilityDoes {
    /// The capability whose effect it is.
    pub(crate) const fn capability(self) -> Capability {
        match self {
            CapabilityDoes::Damage { .. }
            | CapabilityDoes::Heal { .. }
            | CapabilityDoes::Restore { .. } => Capability::Combat,
            CapabilityDoes::Xp { .. } => Capability::Progression,
            CapabilityDoes::Launch { .. } => Capability::Areas,
            CapabilityDoes::Dash { .. } | CapabilityDoes::KnockBack { .. } => {
                Capability::Navigation
            }
        }
    }
}
