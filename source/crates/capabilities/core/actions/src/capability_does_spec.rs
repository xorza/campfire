use campfire_sim::Capability;
use campfire_values::{DamageKind, TrackId, UnitType};

use crate::amount::Amount;
use crate::effect_data::EffectTo;
use crate::launch_id::LaunchId;

/// What a listed effect of a capability above the action pipeline does, its names resolved,
/// which that capability queues as it registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityDoesSpec {
    Damage {
        amount: Amount,
        kind: DamageKind,
    },
    Heal {
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

impl CapabilityDoesSpec {
    /// The capability whose effect it is.
    pub const fn capability(self) -> Capability {
        match self {
            Self::Damage { .. } | Self::Heal { .. } => Capability::Combat,
            Self::Xp { .. } => Capability::Progression,
            Self::Launch { .. } => Capability::Areas,
            Self::Dash { .. } | Self::KnockBack { .. } => Capability::Navigation,
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_math::Num;
    use campfire_values::{DamageKind, TrackId, UnitType};

    use crate::amount::Amount;
    use crate::capability_does_spec::CapabilityDoesSpec;
    use crate::effect_data::EffectTo;
    use crate::launch_id::LaunchId;

    impl CapabilityDoesSpec {
        /// One effect of each kind: a new kind fails to compile here until it joins.
        pub fn one_of_each() -> Vec<Self> {
            let amount = Amount::Value(Num::ONE);
            let all = vec![
                Self::Damage {
                    amount,
                    kind: DamageKind::new(0),
                },
                Self::Heal { amount },
                Self::Xp {
                    track: TrackId::new(0),
                    amount,
                },
                Self::Launch {
                    area: UnitType::new(0),
                    launch: LaunchId(0),
                },
                Self::Dash {
                    to: EffectTo::Reached,
                    speed: amount,
                },
                Self::KnockBack {
                    from: EffectTo::Source,
                    distance: amount,
                    ms: amount,
                },
            ];
            for (at, does) in all.iter().enumerate() {
                let place = match does {
                    Self::Damage { .. } => 0,
                    Self::Heal { .. } => 1,
                    Self::Xp { .. } => 2,
                    Self::Launch { .. } => 3,
                    Self::Dash { .. } => 4,
                    Self::KnockBack { .. } => 5,
                };
                assert_eq!(place, at, "one of each kind, in order");
            }
            all
        }
    }
}
