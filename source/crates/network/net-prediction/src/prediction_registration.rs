//! Registering and installing a component's prediction and local rollback.
use bevy_app::App;
use bevy_ecs::component::{ComponentId, Mutable};
use bevy_ecs::prelude::*;
use bevy_ecs::world::FilteredEntityRef;
use bevy_replicon::prelude::AppMarkerExt;
use bevy_replicon::shared::replication::diff::Diffable;
use bevy_utils::prelude::DebugName;
use campfire_net_core::NetTick;
use campfire_net_frame_interpolation::FrameRegistry;
use campfire_net_frame_interpolation::{FrameSamples, FrameSource};
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::HistoryComponent;
use campfire_net_replication::Predicted;
use campfire_net_replication::{PreSpawned, PredictedSend};
use tracing::trace;

use crate::history_receive;
use crate::mismatch_fn::MismatchFn;
use crate::prediction_history::PredictionHistory;
use crate::prediction_plugin;
use crate::prediction_plugin::{add_non_networked_rollback_systems, add_prediction_systems};
use crate::prediction_registry;

pub(crate) fn register_rollback_fns<C: Component<Mutability = Mutable> + Clone>(
    app: &mut App,
    prediction_history_id: ComponentId,
) {
    prediction_registry::open(app).register_rollback::<C>(prediction_history_id);
    // A rollback replays the prediction history, which frame interpolation then reads, so a
    // replay leaves no frame history to repair.
    app.world_mut()
        .get_resource_or_init::<FrameRegistry>()
        .set_frame_source::<C>(FrameSource::new::<C>(
            prediction_history_id,
            prediction_frame_samples::<C>,
        ));
}

/// The frame samples of a rolled-back `C`: its predicted values at `tick` and the tick before,
/// read from its [`PredictionHistory<C>`].
pub(crate) fn prediction_frame_samples<'w, C: Component>(
    source: &FilteredEntityRef<'w, '_>,
    tick: NetTick,
) -> FrameSamples<'w, C> {
    let Some(history) = source.get::<PredictionHistory<C>>() else {
        return FrameSamples::NONE;
    };
    FrameSamples {
        previous: history.get(tick - 1),
        current: history.get(tick),
    }
}

pub(crate) fn register_prediction_fns<C: HistoryComponent>(app: &mut App) {
    let prediction_history_id = app.world_mut().register_component::<PredictionHistory<C>>();
    register_rollback_fns::<C>(app, prediction_history_id);
    prediction_registry::open(app).register::<C>();
}

/// Installs the receive fns and systems of a component predicted whole.
pub(crate) fn install_prediction<C: HistoryComponent>(app: &mut App) {
    app.set_marker_fns::<Predicted, C>(
        history_receive::write_history::<C>,
        ConfirmedHistory::<C>::receive_removal,
    );
    app.set_marker_fns::<PredictedSend, C>(
        history_receive::write_initial_live_and_history::<C>,
        ConfirmedHistory::<C>::receive_removal,
    );
    // A pre-spawned entity can receive replicated component data before the
    // server match has inserted `Predicted`. Keep that authoritative data in
    // history so it cannot overwrite the live locally-predicted component.
    app.set_marker_fns::<PreSpawned, C>(
        history_receive::write_history::<C>,
        ConfirmedHistory::<C>::receive_removal,
    );
    trace!(
        "Adding prediction for component {:?}",
        DebugName::type_name::<C>()
    );
    add_prediction_systems::<C>(app);
}

/// Installs the receive fns and systems of a component predicted from diffs.
pub(crate) fn install_prediction_diff<C: HistoryComponent + Diffable>(app: &mut App) {
    app.set_marker_fns::<Predicted, C>(
        history_receive::write_history_diff::<C>,
        ConfirmedHistory::<C>::receive_removal,
    );
    app.set_marker_fns::<PredictedSend, C>(
        history_receive::write_initial_live_and_history_diff::<C>,
        ConfirmedHistory::<C>::receive_removal,
    );
    app.set_marker_fns::<PreSpawned, C>(
        history_receive::write_history_diff::<C>,
        ConfirmedHistory::<C>::receive_removal,
    );
    trace!(
        "Adding diff prediction for component {:?}",
        DebugName::type_name::<C>()
    );
    add_prediction_systems::<C>(app);
    prediction_plugin::add_prediction_diff_systems::<C>(app);
}

pub(crate) fn set_mismatch<C: HistoryComponent>(app: &mut App, mismatch: MismatchFn<C>) {
    prediction_registry::open(app).set_mismatch::<C>(mismatch);
}

/// Registers rollback for a component that no network replicates: it rolls back with the
/// predicted entities, but has no confirmed value to check.
pub(crate) fn add_local_rollback<C: Component<Mutability = Mutable> + Clone>(app: &mut App) {
    let prediction_history_id = app.world_mut().register_component::<PredictionHistory<C>>();
    register_rollback_fns::<C>(app, prediction_history_id);
    prediction_registry::open(app).defer(install_local_rollback::<C>);
}

fn install_local_rollback<C: Component<Mutability = Mutable> + Clone>(app: &mut App) {
    add_non_networked_rollback_systems::<C>(app);

    // Resources live on dedicated entities in Bevy. If the resource was inserted before
    // rollback was registered, its `Add<C>` observer has already run, so backfill the history
    // that marks the resource entity as a rollback participant.
    let resource_entity = app
        .world()
        .component_id::<C>()
        .and_then(|component_id| app.world().resource_entities().get(component_id));
    if let Some(resource_entity) = resource_entity {
        app.world_mut()
            .entity_mut(resource_entity)
            .insert_if_new(PredictionHistory::<C>::default());
    }
}
