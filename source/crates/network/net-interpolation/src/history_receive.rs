//! Writing received values and removals into confirmed histories.
use crate::history_component::HistoryComponent;
use bevy_app::App;

use bevy_ecs::prelude::*;
use bevy_replicon::prelude::AppMarkerExt;
use bevy_replicon::prelude::RuleFns;
use bevy_replicon::shared::replication::deferred_entity::DeferredEntity;
use bevy_replicon::shared::replication::diff::Diffable as RepliconDiffable;
use bevy_replicon::shared::replication::registry::ctx::{RemoveCtx, WriteCtx};
use bevy_replicon::shared::replication::registry::receive_fns::WriteFn;
use bytes::Bytes;
use campfire_net_replication::ComponentKind;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::Interpolated;
use campfire_net_replication::InterpolatedSend;
use campfire_net_replication::history_write;
use tracing::debug;

/// The components whose receive fns are set under the interpolation markers.
#[derive(Resource, Debug, Default)]
pub(crate) struct InterpolatedMarkerFns(pub(crate) Vec<ComponentKind>);

pub(crate) fn register_marker_fns<C: HistoryComponent>(app: &mut App) {
    register_interpolated_marker_fns::<C>(app, write_history::<C>);
}

pub(crate) fn register_diff_marker_fns<C: HistoryComponent + RepliconDiffable>(app: &mut App) {
    register_interpolated_marker_fns::<C>(app, write_history_diff::<C>);
}

/// Sets `C`'s receive fns under the interpolation markers, once.
fn register_interpolated_marker_fns<C: HistoryComponent>(app: &mut App, write: WriteFn<C>) {
    let kind = ComponentKind::of::<C>();
    let mut registered = app
        .world_mut()
        .get_resource_or_init::<InterpolatedMarkerFns>();
    if registered.0.contains(&kind) {
        return;
    }
    registered.0.push(kind);
    app.set_marker_fns::<Interpolated, C>(write, remove_history::<C>);
    app.set_marker_fns::<InterpolatedSend, C>(write, remove_history::<C>);
}

/// Instead of writing into a component directly, it writes data into [`ConfirmedHistory<C>`].
fn write_history<C: HistoryComponent>(
    ctx: &mut WriteCtx<'_>,
    rule_fns: &RuleFns<C>,
    entity: &mut DeferredEntity<'_>,
    message: &mut Bytes,
) -> Result<()> {
    let component: C = rule_fns.deserialize(ctx, message)?;
    let Some(tick) = history_write::message_tick(entity, ctx.message_tick) else {
        debug!(
            message_tick = ?ctx.message_tick,
            "dropping an interpolated value that no checkpoint maps"
        );
        return Ok(());
    };
    if let Some(mut history) = entity.get_mut::<ConfirmedHistory<C>>() {
        history.insert_present(tick, component);
    } else {
        let mut history = ConfirmedHistory::<C>::default();
        history.insert_present(tick, component);
        entity.insert(history);
    }
    Ok(())
}

fn write_history_diff<C: HistoryComponent + RepliconDiffable>(
    ctx: &mut WriteCtx<'_>,
    _rule_fns: &RuleFns<C>,
    entity: &mut DeferredEntity<'_>,
    message: &mut Bytes,
) -> Result<()> {
    history_write::receive_diff::<C>(ctx, entity, message, |_, _| {})
}

/// Records a component removal in `ConfirmedHistory<C>`.
///
/// The live component is removed later by interpolation systems once the interpolation timeline
/// reaches the server tick that produced this removal.
pub(crate) fn remove_history<C: HistoryComponent>(
    ctx: &mut RemoveCtx,
    entity: &mut DeferredEntity<'_>,
) {
    let Some(tick) = history_write::message_tick(entity, ctx.message_tick) else {
        debug!(
            message_tick = ?ctx.message_tick,
            "dropping an interpolated removal that no checkpoint maps"
        );
        return;
    };
    if let Some(mut history) = entity.get_mut::<ConfirmedHistory<C>>() {
        history.insert_removed(tick);
    } else {
        let mut history = ConfirmedHistory::<C>::default();
        history.insert_removed(tick);
        entity.insert(history);
    }
}
