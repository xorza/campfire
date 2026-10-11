use campfire_sim::Capability;

use crate::actions::amount::Amount;
use crate::actions::effect_data::EffectTo;
use crate::actions::launch_id::LaunchId;
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

#[cfg(test)]
pub(crate) mod internals {
    use campfire_math::Num;

    use crate::actions::amount::Amount;
    use crate::actions::capability_does::CapabilityDoes;
    use crate::actions::effect_data::EffectTo;
    use crate::actions::launch_id::LaunchId;
    use crate::stats::pool_id::PoolId;
    use crate::units::track_id::TrackId;
    use crate::units::unit_type::UnitType;
    use crate::values::damage_kind::DamageKind;

    impl CapabilityDoes {
        /// One effect of each kind: a new kind fails to compile here until it joins.
        pub(crate) fn one_of_each() -> Vec<CapabilityDoes> {
            let amount = Amount::Value(Num::ONE);
            let all = vec![
                CapabilityDoes::Damage {
                    amount,
                    kind: DamageKind::new(0),
                },
                CapabilityDoes::Heal { amount },
                CapabilityDoes::Restore {
                    pool: PoolId::new(0).unwrap(),
                    amount,
                },
                CapabilityDoes::Xp {
                    track: TrackId::new(0),
                    amount,
                },
                CapabilityDoes::Launch {
                    area: UnitType::new(0),
                    launch: LaunchId(0),
                },
                CapabilityDoes::Dash {
                    to: EffectTo::Reached,
                    speed: amount,
                },
                CapabilityDoes::KnockBack {
                    from: EffectTo::Source,
                    distance: amount,
                    ms: amount,
                },
            ];
            for (at, does) in all.iter().enumerate() {
                let place = match does {
                    CapabilityDoes::Damage { .. } => 0,
                    CapabilityDoes::Heal { .. } => 1,
                    CapabilityDoes::Restore { .. } => 2,
                    CapabilityDoes::Xp { .. } => 3,
                    CapabilityDoes::Launch { .. } => 4,
                    CapabilityDoes::Dash { .. } => 5,
                    CapabilityDoes::KnockBack { .. } => 6,
                };
                assert_eq!(place, at, "one of each kind, in order");
            }
            all
        }
    }
}
