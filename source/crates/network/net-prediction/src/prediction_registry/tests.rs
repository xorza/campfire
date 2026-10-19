use super::*;
use crate::fixtures::prediction_stack::PredictionStack;
use crate::prediction_history::PredictionHistory;
use crate::prediction_registration_ext::PredictionRegistrationExt;
use bevy_replicon::prelude::EntityDiffExt;
use bevy_replicon::prelude::{RepliconTick, RuleFns};
use bytes::Bytes;
use campfire_codec::Binary;
use campfire_net_connection::NetworkTarget;
use campfire_net_core::NetTick;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::ConfirmedTickMap;
use campfire_net_replication::HistoryState;
use campfire_net_replication::Predicted;
use campfire_net_replication::internals::CompRepliconDiff;
use campfire_net_replication::internals::client_copy;
use campfire_net_replication::internals::newest_confirmed_value;
use campfire_net_replication::{PredictionTarget, Replicate};
use campfire_net_stepper::StepperSetup;
use campfire_net_stepper::{StepOrder, Stepper};
use campfire_net_sync::TimelineSync;
use campfire_net_timeline::LocalTimeline;

use bevy_replicon::shared::replication::diff::Diffable as RepliconDiffable;

use core::fmt::Debug;

use campfire_net_replication::ComponentRegistry;

impl PredictionRegistry {
    /// Returns true if the component is predicted
    pub(crate) fn is_predicted<C: Component>(&self) -> bool {
        self.fns(ComponentTypeId::of::<C>())
            .is_some_and(|fns| fns.prediction.is_some())
    }

    /// Whether the component rolls back locally, with no prediction fns.
    pub(crate) fn rolls_back_locally<C: Component>(&self) -> bool {
        self.fns(ComponentTypeId::of::<C>())
            .is_some_and(|fns| fns.prediction.is_none())
    }
}
use crate::prediction_marker_plugin::PredictionMarkerPlugin;
use crate::prediction_plugins::PredictionPlugins;
use crate::prediction_registry_plugin::PredictionRegistryPlugin;
use crate::prediction_setup::PredictionSetup;
use bevy_ecs::system::RunSystemOnce;
use bevy_replicon::prelude::{AuthMethod, RepliconPlugins, RepliconSharedPlugin};
use bevy_replicon::shared::replication::diff::diff_index::DiffIndex;
use bevy_replicon::shared::replication::registry::FnsId;
use bevy_replicon::shared::replication::registry::ReplicationRegistry;
use bevy_replicon::shared::replication::registry::test_fns::TestFnsEntityExt;
use bevy_state::app::StatesPlugin;
use campfire_net_interpolation::{
    InterpolationFns, InterpolationMarkerPlugin, InterpolationPlugins,
    InterpolationRegistrationExt, InterpolationRegistry, InterpolationRegistryPlugin,
};
use campfire_net_replication::AppComponentExt;
use campfire_net_sync::{InputTimelineSetup, LocalTimelineSync};
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, PartialEq, Debug, Serialize, Deserialize)]
struct BuilderComponent(u32);

#[derive(Component, Clone, PartialEq, Debug)]
struct LocalRollbackComponent(u32);

#[derive(Component, Clone, Debug)]
struct LocalRollbackOnlyComponent;

#[derive(Resource, Clone, Debug, PartialEq)]
struct LocalRollbackOnlyResource(u32);

fn prediction_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        StatesPlugin,
        RepliconSharedPlugin {
            auth_method: AuthMethod::None,
        },
        PredictionMarkerPlugin,
        InterpolationMarkerPlugin,
        InterpolationRegistryPlugin,
        InterpolationPlugins,
    ));
    app.init_resource::<PredictionRegistry>();
    app.init_resource::<PredictionSetup>();
    app.init_resource::<InputTimelineSetup>();
    app
}

/// A server app: the shared protocol's plugins and the registry plugin, without
/// `PredictionPlugins`.
fn server_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        StatesPlugin,
        RepliconSharedPlugin {
            auth_method: AuthMethod::None,
        },
        PredictionMarkerPlugin,
        InterpolationMarkerPlugin,
        InterpolationRegistryPlugin,
        InterpolationPlugins,
        PredictionRegistryPlugin,
    ));
    app
}

/// A server seals the registry as a client does, and runs none of the installs, which only a
/// predicting app needs: an entity with a predicted component and `Predicted` gets no history.
#[test]
fn a_server_seals_the_registry_and_installs_nothing() {
    let mut app = server_app();
    app.register_component::<BuilderComponent>()
        .replicate()
        .predict();
    app.finish();

    assert!(app.world().resource::<PredictionRegistry>().finalized);
    let entity = app.world_mut().spawn((BuilderComponent(1), Predicted)).id();
    app.world_mut().flush();
    assert!(
        !app.world()
            .entity(entity)
            .contains::<PredictionHistory<BuilderComponent>>()
    );
}

#[test]
#[should_panic(
    expected = "a component registers for prediction before the app finishes its plugins"
)]
fn a_late_registration_on_a_server_panics() {
    let mut app = server_app();
    app.finish();
    app.register_component::<LocalRollbackComponent>()
        .local_rollback();
}

/// Runs the deferred registrations and seals the registry, as the app's plugins do when it
/// finishes.
fn finish_registration(app: &mut App) {
    app.finish();
    install(app);
    seal(app);
}

fn parity_mismatch(confirmed: &BuilderComponent, predicted: &BuilderComponent) -> bool {
    confirmed.0 % 2 != predicted.0 % 2
}

#[test]
fn the_marker_plugin_enables_no_prediction() {
    let mut app = App::new();
    app.add_plugins((
        StatesPlugin,
        RepliconSharedPlugin {
            auth_method: AuthMethod::None,
        },
        PredictionMarkerPlugin,
    ));

    assert!(!app.world().contains_resource::<PredictionRegistry>());
}

#[test]
fn a_predict_builder_enables_prediction_before_its_mismatch_fn() {
    let mut app = prediction_app();

    app.register_component::<BuilderComponent>()
        .replicate()
        .predict()
        .mismatch_with(parity_mismatch);

    let registry = app.world().resource::<PredictionRegistry>();
    assert!(registry.is_predicted::<BuilderComponent>());
    assert!(!registry.mismatch::<BuilderComponent>()(
        &BuilderComponent(1),
        &BuilderComponent(3)
    ));
    assert!(registry.mismatch::<BuilderComponent>()(
        &BuilderComponent(1),
        &BuilderComponent(2)
    ));
}

#[test]
fn a_predicted_builder_can_add_its_own_interpolation() {
    let mut app = prediction_app();

    app.register_component::<BuilderComponent>()
        .replicate()
        .predict()
        .interpolate(InterpolationFns::history_only())
        .mismatch_with(parity_mismatch);
    finish_registration(&mut app);

    let prediction_registry = app.world().resource::<PredictionRegistry>();
    assert!(prediction_registry.is_predicted::<BuilderComponent>());

    let interpolation_registry = app.world().resource::<InterpolationRegistry>();
    assert!(interpolation_registry.is_interpolated::<BuilderComponent>());
    assert!(prediction_registry.mismatch::<BuilderComponent>()(
        &BuilderComponent(1),
        &BuilderComponent(2)
    ));
}

#[test]
fn an_interpolated_builder_can_add_prediction() {
    let mut app = prediction_app();

    app.register_component::<BuilderComponent>()
        .replicate()
        .interpolate(InterpolationFns::history_only())
        .predict()
        .mismatch_with(parity_mismatch);
    finish_registration(&mut app);

    let prediction_registry = app.world().resource::<PredictionRegistry>();
    assert!(prediction_registry.is_predicted::<BuilderComponent>());

    let interpolation_registry = app.world().resource::<InterpolationRegistry>();
    assert!(interpolation_registry.is_interpolated::<BuilderComponent>());
    assert!(prediction_registry.mismatch::<BuilderComponent>()(
        &BuilderComponent(1),
        &BuilderComponent(2)
    ));
}

#[test]
fn a_local_rollback_builder_registers_a_rollback_with_no_network() {
    let mut app = App::new();
    app.init_resource::<PredictionRegistry>();

    app.register_component::<LocalRollbackComponent>()
        .local_rollback();
    install(&mut app);
    seal(&mut app);

    let registry = app.world().resource::<PredictionRegistry>();
    assert!(!registry.is_predicted::<LocalRollbackComponent>());
    assert!(registry.rolls_back_locally::<LocalRollbackComponent>());
    assert!(app.world().get_resource::<ComponentRegistry>().is_none());
}

#[derive(Component, Clone, Debug, Deserialize, PartialEq, Serialize)]
struct TestDiffComponent(u32);

impl RepliconDiffable for TestDiffComponent {
    type Diff = u32;

    fn apply_diff(&mut self, diff: &Self::Diff) -> Result<()> {
        self.0 = *diff;
        Ok(())
    }
}

#[derive(Debug, Serialize)]
enum TestComponentDelta<'a> {
    Snapshot {
        index: DiffIndex,
        component: &'a TestDiffComponent,
    },
    Diffs {
        index: DiffIndex,
        diffs: &'a [u32],
    },
}

fn diff_snapshot(index: u16, component: &TestDiffComponent) -> Bytes {
    let mut message = Vec::new();
    let wire = TestComponentDelta::Snapshot {
        index: DiffIndex::new(index),
        component,
    };
    Binary::encode_into(&wire, &mut message);
    message.into()
}

fn diff_message(index: u16, diffs: &[u32]) -> Bytes {
    let mut message = Vec::new();
    let wire = TestComponentDelta::Diffs {
        index: DiffIndex::new(index),
        diffs,
    };
    Binary::encode_into(&wire, &mut message);
    message.into()
}

fn setup_prediction_diff_app() -> (App, FnsId) {
    let mut app = App::new();
    app.add_plugins((
        StatesPlugin,
        RepliconPlugins,
        PredictionMarkerPlugin,
        InterpolationRegistryPlugin,
        PredictionRegistryPlugin,
        PredictionPlugins,
    ));
    app.insert_resource(LocalTimeline::default());
    app.insert_resource(ConfirmedTickMap::default());
    app.insert_resource(PredictionSetup::default());
    app.world_mut().flush();
    app.register_component::<TestDiffComponent>()
        .replicate_diff()
        .predict();
    app.finish();

    let fns_id =
        app.world_mut()
            .resource_scope(|world, mut registry: Mut<'_, ReplicationRegistry>| {
                let (_, fns_id) =
                    registry.register_rule_fns(world, RuleFns::<TestDiffComponent>::new_diff());
                fns_id
            });
    (app, fns_id)
}

fn record_confirmed_tick(app: &mut App, tick: u32) -> RepliconTick {
    let replicon_tick = RepliconTick::new(tick);
    app.world_mut()
        .resource_mut::<ConfirmedTickMap>()
        .record(replicon_tick, NetTick(tick));
    replicon_tick
}

#[test]
fn a_local_rollback_takes_a_component_that_is_not_sync() {
    let mut app = prediction_app();

    app.register_component::<LocalRollbackOnlyComponent>()
        .local_rollback();

    assert!(
        app.world()
            .component_id::<PredictionHistory<LocalRollbackOnlyComponent>>()
            .is_some()
    );
    assert!(
        !app.world()
            .resource::<PredictionRegistry>()
            .is_predicted::<LocalRollbackOnlyComponent>()
    );
}

#[test]
fn a_local_rollback_resource_gives_an_existing_resource_its_history() {
    let mut app = prediction_app();
    app.insert_resource(LocalTimeline::default());
    let mut sync = LocalTimelineSync::default();
    sync.set_synced(true);
    app.insert_resource(sync);
    app.insert_resource(LocalRollbackOnlyResource(42));
    app.world_mut()
        .spawn((PredictionSetup::default(), InputTimelineSetup::default()));

    app.register_component::<LocalRollbackOnlyResource>()
        .local_rollback();
    install(&mut app);
    seal(&mut app);
    app.world_mut()
        .run_system_once(PredictionHistory::<LocalRollbackOnlyResource>::update)
        .unwrap();

    assert!(
        app.world()
            .component_id::<PredictionHistory<LocalRollbackOnlyResource>>()
            .is_some()
    );
    assert!(
        !app.world()
            .resource::<PredictionRegistry>()
            .is_predicted::<LocalRollbackOnlyResource>()
    );

    let resource_entity = app
        .world()
        .resource_entities()
        .get(
            app.world()
                .component_id::<LocalRollbackOnlyResource>()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        app.world()
            .get::<PredictionHistory<LocalRollbackOnlyResource>>(resource_entity)
            .unwrap()
            .get_state_at_or_before(NetTick(0)),
        Some(&HistoryState::Updated(LocalRollbackOnlyResource(42)))
    );
}

#[test]
fn a_local_rollback_resource_gives_a_later_resource_its_history() {
    let mut app = prediction_app();
    app.insert_resource(LocalTimeline::default());
    let mut sync = LocalTimelineSync::default();
    sync.set_synced(true);
    app.insert_resource(sync);
    app.world_mut()
        .spawn((PredictionSetup::default(), InputTimelineSetup::default()));

    app.register_component::<LocalRollbackOnlyResource>()
        .local_rollback();
    install(&mut app);
    seal(&mut app);
    app.insert_resource(LocalRollbackOnlyResource(42));
    app.world_mut().flush();
    app.world_mut()
        .run_system_once(PredictionHistory::<LocalRollbackOnlyResource>::update)
        .unwrap();

    let resource_entity = app
        .world()
        .resource_entities()
        .get(
            app.world()
                .component_id::<LocalRollbackOnlyResource>()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        app.world()
            .get::<PredictionHistory<LocalRollbackOnlyResource>>(resource_entity)
            .unwrap()
            .get_state_at_or_before(NetTick(0)),
        Some(&HistoryState::Updated(LocalRollbackOnlyResource(42)))
    );
}

#[test]
fn a_diff_prediction_buffers_a_newer_diff_until_its_older_base_arrives() {
    let (mut app, fns_id) = setup_prediction_diff_app();
    let tick0 = record_confirmed_tick(&mut app, 0);
    let tick3 = record_confirmed_tick(&mut app, 3);
    let tick5 = record_confirmed_tick(&mut app, 5);

    let entity = app.world_mut().spawn(Predicted).id();

    app.world_mut().entity_mut(entity).apply_write(
        diff_snapshot(0, &TestDiffComponent(0)),
        fns_id,
        tick0,
    );

    app.world_mut()
        .entity_mut(entity)
        .apply_write(diff_message(5, &[4, 5]), fns_id, tick5);
    {
        let entity_ref = app.world().entity(entity);
        let history = entity_ref
            .get::<ConfirmedHistory<TestDiffComponent>>()
            .unwrap();
        assert!(history.get_state_at(NetTick(5)).is_none());
    }

    app.world_mut()
        .entity_mut(entity)
        .apply_write(diff_message(3, &[1, 2, 3]), fns_id, tick3);

    let entity_ref = app.world().entity(entity);
    let history = entity_ref
        .get::<ConfirmedHistory<TestDiffComponent>>()
        .unwrap();
    assert_eq!(
        history
            .get_state_at(NetTick(3))
            .and_then(HistoryState::value),
        Some(&TestDiffComponent(3))
    );
    assert_eq!(
        history
            .get_state_at(NetTick(5))
            .and_then(HistoryState::value),
        Some(&TestDiffComponent(5))
    );
}

/// The end-to-end prediction path for Replicon diff components:
/// server-side `apply_diff` produces diff replication, and the client stores
/// the materialized value in `ConfirmedHistory`.
#[test]
fn a_diff_prediction_records_its_confirmed_history_from_replicon_diffs() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
            CompRepliconDiff(0),
        ))
        .id();
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(server_entity)
        .apply_diff::<CompRepliconDiff>(0)
        .unwrap();

    stepper.step(1, stepper.frame_interval(), StepOrder::ServerFirst);
    let client_entity = client_copy(&stepper, server_entity).unwrap();
    assert!(
        stepper
            .client_app(0)
            .world()
            .entity(client_entity)
            .contains::<Predicted>()
    );
    assert_eq!(
        newest_confirmed_value(stepper.client_app_mut(0), client_entity),
        Some(0)
    );

    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(server_entity)
        .apply_diff::<CompRepliconDiff>(1)
        .unwrap();
    stepper.step(1, stepper.frame_interval(), StepOrder::ServerFirst);

    assert_eq!(
        newest_confirmed_value(stepper.client_app_mut(0), client_entity),
        Some(1)
    );
}
