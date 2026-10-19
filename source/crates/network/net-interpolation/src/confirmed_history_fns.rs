use crate::history_component::HistoryComponent;
use crate::interpolate::{update_confirmed_history, update_confirmed_history_diff};
use crate::interpolation_stage::InterpolationStage;
use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_replicon::shared::replication::diff::Diffable as RepliconDiffable;
use campfire_net_replication::Replication;

/// What a member that owns its confirmed history installs, once for its component: the system
/// that maintains it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ConfirmedHistoryFns {
    /// How the component replicates, which one component does one way.
    pub(crate) replication: Replication,
    pub(crate) install: fn(&mut App),
}

impl ConfirmedHistoryFns {
    /// From whole replicated values.
    pub(crate) fn full<C: HistoryComponent>() -> Self {
        Self {
            replication: Replication::Full,
            install: |app| {
                app.add_systems(
                    Update,
                    update_confirmed_history::<C>.in_set(InterpolationStage::History),
                );
            },
        }
    }

    /// From replicated diffs.
    pub(crate) fn diff<C: HistoryComponent + RepliconDiffable>() -> Self {
        Self {
            replication: Replication::Diff,
            install: |app| {
                app.add_systems(
                    Update,
                    update_confirmed_history_diff::<C>.in_set(InterpolationStage::History),
                );
            },
        }
    }
}
