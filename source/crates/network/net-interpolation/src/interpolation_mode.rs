use crate::interpolation_registry::InterpolationRegistry;
use crate::member_mode::MemberMode;
use bevy_replicon::shared::replication::diff::Diffable as RepliconDiffable;
use campfire_net_replication::HistoryComponent;
use campfire_net_replication::{DiffReplicated, FullReplicated, NotReplicated};

/// How a component of a replication kind takes part in interpolation: from whole values, from
/// diffs, or, unreplicated, with no confirmed history.
pub trait InterpolationMode<C> {
    /// Records how the component's confirmed history is written, for each rule it is a member
    /// of.
    fn record(registry: &mut InterpolationRegistry);
}

impl<C: HistoryComponent> InterpolationMode<C> for NotReplicated {
    fn record(registry: &mut InterpolationRegistry) {
        registry.record_member::<C>(MemberMode::Local);
    }
}

impl<C: HistoryComponent> InterpolationMode<C> for FullReplicated {
    fn record(registry: &mut InterpolationRegistry) {
        registry.record_member::<C>(MemberMode::full::<C>());
    }
}

impl<C: HistoryComponent + RepliconDiffable> InterpolationMode<C> for DiffReplicated {
    fn record(registry: &mut InterpolationRegistry) {
        registry.record_member::<C>(MemberMode::diff::<C>());
    }
}
