//! Replication send-side: target components, visibility rules, and send scheduling.
//!
//! The main type here is [`Replicate`] (an alias for [`ReplicationTarget<()>`]),
//! which selects the peers that receive an entity. Adding it also inserts
//! Replicon's required [`Replicated`] marker.
//! On the server you also typically add [`PredictionTarget`] and
//! [`InterpolationTarget`] to control which clients run prediction or
//! interpolation for that entity.
//!
//! Each link entity (the entity representing a connection to a remote peer) needs a
//! [`ReplicationSender`](crate::replication_sender::ReplicationSender) component to enable outgoing
//! replication through that link.

use crate::interpolated::Interpolated;
use crate::predicted::Predicted;
use crate::replication_mode::ReplicationMode;
use crate::sent_marker::SentMarker;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use bevy_replicon::prelude::Replicated;
use bevy_replicon::prelude::ScopeLifetime;
use bevy_replicon::prelude::SingleComponent;
use bevy_replicon::prelude::VisibilityFilter;
use bevy_replicon::server::visibility::client_visibility::ClientVisibility;
use bevy_replicon::server::visibility::registry::FilterRegistry;
use campfire_net_connection::HostClient;
use campfire_net_connection::NetworkTarget;
use campfire_net_connection::RemoteId;
use serde::Deserialize;
use serde::Serialize;

/// Selects which peers an entity is replicated to.
///
/// Inserting this component also inserts Replicon's required
/// [`Replicated`] marker. Removing it changes the entity's
/// visibility and can despawn its remote copies; remove `Replicated` instead to pause replication
/// without despawning them. Component removals made during the pause are not replayed when
/// replication resumes.
///
/// The target is evaluated as a Replicon [`VisibilityFilter`]:
/// spawning (or replacing) it evaluates visibility against all connected links, and it composes
/// (logical AND) with rooms and manual visibility. The component is immutable, so retarget with
/// `insert`, not mutation.
///
/// # Example
///
/// ```rust
/// use bevy_ecs::prelude::Entity;
/// use bevy_replicon::prelude::VisibilityFilter;
/// use campfire_net_connection::NetworkTarget;
/// use campfire_net_core::PeerId;
/// use campfire_net_connection::RemoteId;
/// use campfire_net_replication::Replicate;
///
/// let alice = PeerId::Entity(1);
/// let bob = PeerId::Entity(2);
///
/// // The predicate Replicon evaluates for each link. Only the link's `RemoteId` matters, and
/// // links without one receive nothing.
/// let link = Entity::PLACEHOLDER;
/// let target = Replicate::to_clients(NetworkTarget::single(alice));
/// assert!(target.is_visible(link, Some(&RemoteId(alice))));
/// assert!(!target.is_visible(link, Some(&RemoteId(bob))));
/// assert!(!target.is_visible(link, None));
/// ```
pub type Replicate = ReplicationTarget<()>;

/// Replication target configuration, evaluated as a Replicon [`VisibilityFilter`].
///
/// The component is immutable: replace it (via `insert`) to retarget, which
/// re-evaluates visibility against all links while preserving manual visibility
/// overrides. It composes (logical AND) with rooms and manual visibility.
#[derive(Component, Clone, Default, Debug, PartialEq, Eq, Reflect)]
#[component(immutable)]
pub struct ReplicationTarget<T: TargetKind> {
    mode: ReplicationMode,
    #[reflect(ignore)]
    marker: core::marker::PhantomData<T>,
}

mod private {
    use crate::replication_target::InterpolatedSend;
    use crate::replication_target::PredictedSend;

    #[expect(unnameable_types, reason = "sealed: only this crate implements it")]
    pub trait Sealed {}
    impl Sealed for () {}
    impl Sealed for PredictedSend {}
    impl Sealed for InterpolatedSend {}
}

#[doc(hidden)]
pub trait TargetKind: private::Sealed + Send + Sync + 'static {
    /// The marker a remote receiver of this target gets, which a host client gets on the
    /// server's entity itself, as it shares the server's world.
    type HostMarker: Bundle + Default;
    /// What leaves the entity with its target: Replicon's [`Replicated`] for [`Replicate`].
    type RemovedWith: Bundle;
    /// Whether a link the entity's prediction target names is no target of this kind: a client
    /// gets `Predicted` alone where both targets name it.
    const YIELDS_TO_PREDICTION: bool = false;
}

/// Entity-level replication target: visible (spawned) exactly on the links in the target.
///
/// Replicon evaluates it for each entity when a link gets its `ClientVisibility`, and for each
/// link when the entity gets the target.
impl VisibilityFilter for ReplicationTarget<()> {
    type ClientComponent = RemoteId;
    type Scope = Entity;
    const LIFETIME: ScopeLifetime = ScopeLifetime::WhileVisible;

    fn is_visible(&self, link: Entity, remote: Option<&RemoteId>) -> bool {
        self.is_visible_for(link, remote)
    }
}

impl TargetKind for () {
    type HostMarker = ();
    type RemovedWith = Replicated;
}

pub(crate) fn set_host_marker<T: TargetKind>(entity: &mut EntityCommands<'_>, targeted: bool) {
    if targeted {
        entity.try_insert(T::HostMarker::default());
    } else {
        entity.try_remove::<T::HostMarker>();
    }
}

impl<T: TargetKind> ReplicationTarget<T> {
    pub const fn new(mode: ReplicationMode) -> Self {
        Self {
            mode,
            marker: core::marker::PhantomData,
        }
    }

    /// Replicates to the links whose peer `target` names.
    pub const fn to_clients(target: NetworkTarget) -> Self {
        Self::new(ReplicationMode::Target(target))
    }

    /// Replicates to these link entities.
    pub fn to_links(links: impl IntoIterator<Item = Entity>) -> Self {
        Self::new(ReplicationMode::Links(links.into_iter().collect()))
    }

    /// The peers or links the target names.
    pub(crate) const fn mode(&self) -> &ReplicationMode {
        &self.mode
    }

    /// Whether a host client `host` takes this target's marker: the target names it, and for a
    /// kind that yields to prediction, the entity's `prediction` target does not.
    pub(crate) fn targets_host(
        &self,
        host: Entity,
        remote: Option<&RemoteId>,
        prediction: Option<&PredictionTarget>,
    ) -> bool {
        self.is_visible_for(host, remote)
            && !(T::YIELDS_TO_PREDICTION
                && prediction.is_some_and(|prediction| prediction.is_visible_for(host, remote)))
    }

    /// Pure visibility predicate behind the [`VisibilityFilter`] impls.
    ///
    /// Also reused by the host marker sync, so that both agree.
    pub(crate) fn is_visible_for(&self, link: Entity, remote: Option<&RemoteId>) -> bool {
        self.mode.is_visible_for(link, remote)
    }

    /// A target that is removed hides the entity from every link, and takes its host marker and
    /// what leaves with it.
    ///
    /// `Remove` does not fire on replace (only `Discard` does), so a retarget runs just the filter's
    /// re-evaluation, preserving manual visibility overrides. A despawn (`new_archetype` of `None`)
    /// is skipped: marking an entity hidden while it is being despawned makes Replicon suppress the
    /// actual despawn message.
    ///
    /// The hide runs deferred (after Replicon's own `ReplicationTarget::on_remove`, which releases the filter bit) so
    /// removing `Replicate` deterministically despawns the remote entity instead of leaving it
    /// retained.
    pub(crate) fn on_remove(trigger: On<'_, '_, Remove<Self>>, mut commands: Commands<'_, '_>)
    where
        Self: VisibilityFilter,
    {
        if trigger.trigger().new_archetype.is_none() {
            return;
        }
        let entity = trigger.entity;
        commands.queue(move |world: &mut World| {
            if let Some(bit) = world.resource::<FilterRegistry>().get_bit::<Self>() {
                let mut senders = world.query::<&mut ClientVisibility>();
                for mut visibility in senders.iter_mut(world) {
                    visibility.set(entity, bit, false);
                }
            }
            if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                entity_mut.remove::<(T::HostMarker, T::RemovedWith)>();
            }
        });
    }

    /// A target that is inserted or replaced gives the entity its host marker exactly while a host
    /// client is in the target.
    pub(crate) fn sync_host_marker(
        trigger: On<'_, '_, Insert<Self>>,
        targets: Query<'_, '_, (&Self, Option<&PredictionTarget>)>,
        hosts: Query<'_, '_, (Entity, Option<&RemoteId>), With<HostClient>>,
        mut commands: Commands<'_, '_>,
    ) where
        T::HostMarker: Component,
    {
        let Ok((target, prediction)) = targets.get(trigger.entity) else {
            return;
        };
        let targeted = hosts
            .iter()
            .any(|(host, remote)| target.targets_host(host, remote, prediction));
        set_host_marker::<T>(&mut commands.entity(trigger.entity), targeted);
    }

    /// A client that becomes a host client gets the host marker of each target it is in.
    pub(crate) fn sync_host_markers_on_host_added(
        trigger: On<'_, '_, Add<HostClient>>,
        hosts: Query<'_, '_, Option<&RemoteId>, With<HostClient>>,
        targets: Query<'_, '_, (Entity, &Self, Has<T::HostMarker>, Option<&PredictionTarget>)>,
        mut commands: Commands<'_, '_>,
    ) where
        T::HostMarker: Component,
    {
        let host = trigger.entity;
        let Ok(remote) = hosts.get(host) else {
            return;
        };
        for (entity, target, marked, prediction) in &targets {
            if !marked && target.targets_host(host, remote, prediction) {
                set_host_marker::<T>(&mut commands.entity(entity), true);
            }
        }
    }

    /// A client that stops being a host client takes the host markers that only it held.
    pub(crate) fn sync_host_markers_on_host_removed(
        trigger: On<'_, '_, Remove<HostClient>>,
        hosts: Query<'_, '_, (Entity, Option<&RemoteId>), With<HostClient>>,
        targets: Query<'_, '_, (Entity, &Self, Option<&PredictionTarget>), With<T::HostMarker>>,
        mut commands: Commands<'_, '_>,
    ) where
        T::HostMarker: Component,
    {
        let leaving = trigger.entity;
        for (entity, target, prediction) in &targets {
            let targeted = hosts.iter().any(|(host, remote)| {
                host != leaving && target.targets_host(host, remote, prediction)
            });
            if !targeted {
                set_host_marker::<T>(&mut commands.entity(entity), false);
            }
        }
    }
}

/// Controls which clients run server-authoritative interpolation for this entity.
///
/// Typically set to all clients *except* the owning client, so remote
/// players see a smooth interpolated version of the entity.
///
/// See [`PredictionTarget`] for the complementary prediction setting.
pub type InterpolationTarget = ReplicationTarget<InterpolatedSend>;

/// Component-level replication target: `InterpolatedSend` is visible exactly on the links in the
/// target.
impl VisibilityFilter for InterpolationTarget {
    type ClientComponent = RemoteId;
    type Scope = SingleComponent<InterpolatedSend>;
    const LIFETIME: ScopeLifetime = ScopeLifetime::WhileVisible;

    fn is_visible(&self, link: Entity, remote: Option<&RemoteId>) -> bool {
        self.is_visible_for(link, remote)
    }
}

/// Controls which clients run client-side prediction for this entity.
///
/// This is the send-side way to enable prediction for an entity. The
/// receive-side can also opt an entity into prediction by inserting
/// `Predicted` directly on the received entity.
///
/// Typically set to the owning client so they get a `Predicted` entity,
/// while other clients receive an `Interpolated` entity instead.
///
/// ```rust,ignore
/// commands.spawn((
///     Replicate::to_clients(NetworkTarget::All),
///     PredictionTarget::to_clients(NetworkTarget::single(client_id)),
///     InterpolationTarget::to_clients(NetworkTarget::all_except([client_id])),
/// ));
/// ```
pub type PredictionTarget = ReplicationTarget<PredictedSend>;

/// Component-level replication target: `PredictedSend` is visible exactly on the links in the
/// target.
impl VisibilityFilter for PredictionTarget {
    type ClientComponent = RemoteId;
    type Scope = SingleComponent<PredictedSend>;
    const LIFETIME: ScopeLifetime = ScopeLifetime::WhileVisible;

    fn is_visible(&self, link: Entity, remote: Option<&RemoteId>) -> bool {
        self.is_visible_for(link, remote)
    }
}

/// Sender-side marker that materializes as [`Predicted`] on the receiver.
///
/// Keeping this distinct from [`Predicted`] prevents an authoritative
/// sender from being mistaken for a client-side predicted simulation.
/// [`PredictionTarget`] adds this component automatically.
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub struct PredictedSend;

impl TargetKind for PredictedSend {
    type HostMarker = Predicted;
    type RemovedWith = ();
}

impl SentMarker for PredictedSend {
    type Received = Predicted;
}

/// Sender-side marker that materializes as [`Interpolated`] on the receiver.
///
/// Keeping this distinct from [`Interpolated`] prevents an authoritative
/// sender from being mistaken for a client-side interpolated presentation.
/// [`InterpolationTarget`] adds this component automatically.
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub struct InterpolatedSend;

impl TargetKind for InterpolatedSend {
    type HostMarker = Interpolated;
    type RemovedWith = ();
    const YIELDS_TO_PREDICTION: bool = true;
}

impl SentMarker for InterpolatedSend {
    type Received = Interpolated;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replication_timer::ReplicationTimer;
    use crate::server_send_plugin::ServerSendPlugin;
    use bevy_app::prelude::*;
    use bevy_replicon::server::server_tick::ServerTick;
    use bevy_time::Time;
    use campfire_net_timeline::LocalTimeline;
    use core::time::Duration;

    #[derive(Debug, Resource, Default)]
    struct SendCount(usize);

    fn count_sends(mut count: ResMut<'_, SendCount>) {
        count.0 += 1;
    }

    fn add_replication_tick_test_systems(app: &mut App) {
        app.add_systems(
            RunFixedMainLoop,
            ServerSendPlugin::update_replication_tick
                .in_set(RunFixedMainLoopSystems::AfterFixedMainLoop),
        );
        app.add_systems(
            PostUpdate,
            count_sends.run_if(resource_changed::<ServerTick>),
        );
    }

    fn clear_send_state(app: &mut App) {
        app.world_mut().run_schedule(PostUpdate);
        app.world_mut().resource_mut::<SendCount>().0 = 0;
        app.world_mut().clear_trackers();
    }

    #[test]
    fn unchanged_timeline_does_not_trigger_send() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.insert_resource(LocalTimeline::default());
        app.insert_resource(ReplicationTimer::default());
        app.init_resource::<ServerTick>();
        app.init_resource::<SendCount>();
        add_replication_tick_test_systems(&mut app);
        clear_send_state(&mut app);

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(1));
        app.world_mut().run_schedule(RunFixedMainLoop);
        app.world_mut().run_schedule(PostUpdate);

        assert_eq!(app.world().resource::<ServerTick>().get(), 0);
        assert_eq!(app.world().resource::<SendCount>().0, 0);
    }

    #[test]
    fn multiple_fixed_ticks_trigger_single_replication_tick_after_fixed_loop() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.insert_resource(LocalTimeline::default());
        app.insert_resource(ReplicationTimer::default());
        app.init_resource::<ServerTick>();
        app.init_resource::<SendCount>();
        add_replication_tick_test_systems(&mut app);
        clear_send_state(&mut app);

        app.world_mut().resource_mut::<LocalTimeline>().shift(2);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(1));
        app.world_mut().run_schedule(RunFixedMainLoop);
        app.world_mut().run_schedule(PostUpdate);

        assert_eq!(app.world().resource::<ServerTick>().get(), 1);
        assert_eq!(app.world().resource::<SendCount>().0, 1);
    }

    #[test]
    fn elapsed_interval_on_no_fixed_frame_sends_on_next_fixed_frame() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.insert_resource(LocalTimeline::default());
        app.insert_resource(ReplicationTimer::new(Duration::from_millis(10)));
        app.init_resource::<ServerTick>();
        app.init_resource::<SendCount>();
        add_replication_tick_test_systems(&mut app);
        clear_send_state(&mut app);

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(10));
        app.world_mut().run_schedule(RunFixedMainLoop);
        app.world_mut().run_schedule(PostUpdate);

        assert_eq!(app.world().resource::<ServerTick>().get(), 0);
        assert_eq!(app.world().resource::<SendCount>().0, 0);

        app.world_mut().clear_trackers();
        app.world_mut().resource_mut::<LocalTimeline>().shift(1);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(1));
        app.world_mut().run_schedule(RunFixedMainLoop);
        app.world_mut().run_schedule(PostUpdate);

        assert_eq!(app.world().resource::<ServerTick>().get(), 1);
        assert_eq!(app.world().resource::<SendCount>().0, 1);
    }
}
