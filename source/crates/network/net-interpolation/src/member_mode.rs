use crate::confirmed_history_fns::ConfirmedHistoryFns;
use crate::history_receive::{register_diff_marker_fns, register_marker_fns};
use bevy_app::App;
use campfire_net_replication::HistoryComponent;

use bevy_replicon::shared::replication::diff::Diffable;
use campfire_net_replication::ReplicationKind;

/// How a rule member's confirmed history is written.
#[derive(Debug, Clone, Copy)]
pub(crate) enum MemberMode {
    /// A component no network replicates: it holds no confirmed history.
    Local,
    /// A replicated component, whole or as diffs.
    Replicated(MemberHistoryFns),
}

/// What a replicated member writes its confirmed history with.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MemberHistoryFns {
    pub(crate) confirmed: ConfirmedHistoryFns,
    pub(crate) register_markers: fn(&mut App),
}

impl MemberMode {
    pub(crate) fn full<C: HistoryComponent>() -> Self {
        Self::Replicated(MemberHistoryFns {
            confirmed: ConfirmedHistoryFns::full::<C>(),
            register_markers: register_marker_fns::<C>,
        })
    }

    pub(crate) fn diff<C: HistoryComponent + Diffable>() -> Self {
        Self::Replicated(MemberHistoryFns {
            confirmed: ConfirmedHistoryFns::diff::<C>(),
            register_markers: register_diff_marker_fns::<C>,
        })
    }

    pub(crate) const fn replication(self) -> Option<ReplicationKind> {
        match self {
            Self::Local => None,
            Self::Replicated(fns) => Some(fns.confirmed.replication),
        }
    }
}
