use bevy_app::{App, PostUpdate};
use bevy_ecs::component::Component;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_replicon::prelude::{AppRuleExt, ReplicationMode};
use bevy_replicon::shared::replication::diff::Diffable;
use bevy_replicon::shared::replication::registry::receive_fns::MutWrite;
use serde::{Serialize, de::DeserializeOwned};

use crate::component_codec::ComponentCodec;
use crate::component_registry::ComponentRegistry;
use crate::component_type_id::ComponentTypeId;
use crate::history_diff_receiver::HistoryDiffReceiver;
use crate::replication_kind::{DiffReplicated, FullReplicated, NotReplicated, ReplicationKind};
use crate::replication_systems::ReplicationSystems;

/// A component's registration, typed by how it replicates: [`NotReplicated`], [`FullReplicated`] or
/// [`DiffReplicated`], which prediction and interpolation read their mode from
/// (11-net-stack/api.md, One builder per protocol item, and the replication mode lives in its
/// type).
#[derive(Debug)]
pub struct ComponentRegistration<'a, C, M = NotReplicated> {
    pub app: &'a mut App,
    marker: core::marker::PhantomData<fn() -> (C, M)>,
}

impl<C, M> ComponentRegistration<'_, C, M> {
    pub const fn new(app: &mut App) -> ComponentRegistration<'_, C, M> {
        ComponentRegistration {
            app,
            marker: core::marker::PhantomData,
        }
    }
}

impl<'a, C> ComponentRegistration<'a, C> {
    /// Replicates this component on each change, encoded through `codec`.
    pub fn replicate(self) -> ComponentRegistration<'a, C, FullReplicated>
    where
        C: Component<Mutability: MutWrite<C>> + Serialize + DeserializeOwned,
    {
        self.app.replicate_with(ComponentCodec::rule_fns::<C>());
        self.record(ReplicationKind::Full)
    }

    /// Replicates this component's first value and its removal, encoded through `codec`.
    pub fn replicate_once(self) -> ComponentRegistration<'a, C, FullReplicated>
    where
        C: Component<Mutability: MutWrite<C>> + Serialize + DeserializeOwned,
    {
        self.app
            .replicate_with((ComponentCodec::rule_fns::<C>(), ReplicationMode::Once));
        self.record(ReplicationKind::Full)
    }

    /// Register this component using Replicon's diff-based replication.
    ///
    /// Mutations must be recorded with
    /// [`EntityDiffExt::apply_diff`](bevy_replicon::shared::replication::diff::EntityDiffExt::apply_diff)
    /// so Replicon can create diff messages. This registers Replicon's `replicate_diff` rule, but
    /// does not also register the normal full-component replication rule.
    pub fn replicate_diff(self) -> ComponentRegistration<'a, C, DiffReplicated>
    where
        C: Diffable,
    {
        self.app.replicate_diff::<C>();
        self.app.add_systems(
            PostUpdate,
            HistoryDiffReceiver::<C>::prune.after(ReplicationSystems::CutHistory),
        );
        self.record(ReplicationKind::Diff)
    }

    /// Types the registration of a component that an earlier [`replicate`](Self::replicate) or
    /// [`replicate_once`](Self::replicate_once) made replicate, as a layer's protocol that extends
    /// another's does.
    ///
    /// # Panics
    ///
    /// When no earlier registration made the component replicate whole.
    pub fn already_replicated(self) -> ComponentRegistration<'a, C, FullReplicated>
    where
        C: Component,
    {
        self.expect_recorded(ReplicationKind::Full)
    }

    /// Types the registration of a component that an earlier
    /// [`replicate_diff`](Self::replicate_diff) made replicate as diffs.
    ///
    /// # Panics
    ///
    /// When no earlier registration made the component replicate as diffs.
    pub fn already_replicated_diff(self) -> ComponentRegistration<'a, C, DiffReplicated>
    where
        C: Component,
    {
        self.expect_recorded(ReplicationKind::Diff)
    }

    fn expect_recorded<M>(self, replication: ReplicationKind) -> ComponentRegistration<'a, C, M>
    where
        C: Component,
    {
        let recorded = self
            .app
            .world()
            .get_resource::<ComponentRegistry>()
            .and_then(|registry| registry.replication(&ComponentTypeId::of::<C>()));
        assert_eq!(
            recorded,
            Some(replication),
            "{} has no earlier registration that replicates it this way",
            core::any::type_name::<C>()
        );
        ComponentRegistration::new(self.app)
    }

    /// Adds the component to the protocol, and records how it replicates for the layers that read
    /// it at run time.
    fn record<M>(self, replication: ReplicationKind) -> ComponentRegistration<'a, C, M>
    where
        C: Component,
    {
        let mut registry = self
            .app
            .world_mut()
            .get_resource_or_init::<ComponentRegistry>();
        if !registry.is_registered::<C>() {
            registry.register_component::<C>();
        }
        registry.set_replication::<C>(replication);
        ComponentRegistration::new(self.app)
    }
}

#[cfg(test)]
mod tests;
