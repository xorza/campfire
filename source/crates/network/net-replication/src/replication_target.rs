//! The send side of replication: the targets that choose which links receive, predict or
//! interpolate an entity; a target brings Replicon's `Replicated`.

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
    /// What leaves the entity with its target: Replicon's [`Replicated`] for [`Replicate`].
    type RemovedWith: Bundle;
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
    type RemovedWith = Replicated;
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

    /// Pure visibility predicate behind the [`VisibilityFilter`] impls.
    pub(crate) fn is_visible_for(&self, link: Entity, remote: Option<&RemoteId>) -> bool {
        self.mode.is_visible_for(link, remote)
    }

    /// A target that is removed hides the entity from every link, and takes what leaves with it.
    ///
    /// `Remove` does not fire on replace (only `Discard` does), so a retarget runs just the
    /// filter's re-evaluation, preserving manual visibility overrides. A despawn (`new_archetype`
    /// of `None`) is skipped: marking an entity hidden while it is being despawned makes Replicon
    /// suppress the actual despawn message.
    ///
    /// The hide runs deferred (after Replicon's own `ReplicationTarget::on_remove`, which releases
    /// the filter bit) so removing `Replicate` deterministically despawns the remote entity instead
    /// of leaving it retained.
    pub(crate) fn on_remove(trigger: On<'_, '_, Remove<Self>>, mut commands: Commands<'_, '_>)
    where
        Self: VisibilityFilter,
    {
        if trigger.trigger().new_archetype.is_none() {
            return;
        }
        commands.run_system_cached_with(Self::hide_removed, trigger.entity);
    }

    /// Hides `entity`, whose target went, from every link, and takes what leaves with it, through
    /// one cached system rather than a query built for each removal.
    fn hide_removed(
        In(entity): In<Entity>,
        registry: Res<'_, FilterRegistry>,
        mut senders: Query<'_, '_, &mut ClientVisibility>,
        mut commands: Commands<'_, '_>,
    ) where
        Self: VisibilityFilter,
    {
        if let Some(bit) = registry.get_bit::<Self>() {
            for mut visibility in &mut senders {
                visibility.set(entity, bit, false);
            }
        }
        commands.entity(entity).try_remove::<T::RemovedWith>();
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
    type RemovedWith = ();
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
    fn an_unchanged_timeline_sends_nothing() {
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
    fn several_fixed_ticks_make_one_replication_tick_after_the_fixed_loop() {
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
