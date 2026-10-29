//! Writing received values and removals into confirmed histories.
use bevy_app::App;

use bevy_ecs::prelude::*;
use bevy_replicon::prelude::AppMarkerExt;
use bevy_replicon::prelude::RuleFns;
use bevy_replicon::shared::replication::deferred_entity::DeferredEntity;
use bevy_replicon::shared::replication::diff::Diffable;
use bevy_replicon::shared::replication::registry::ctx::WriteCtx;
use bevy_replicon::shared::replication::registry::receive_fns::WriteFn;
use bytes::Bytes;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::HistoryComponent;
use campfire_net_replication::HistoryDiffReceiver;
use campfire_net_replication::Interpolated;
use campfire_net_replication::InterpolatedSend;

pub(crate) fn register_marker_fns<C: HistoryComponent>(app: &mut App) {
    register_interpolated_marker_fns::<C>(app, write_history::<C>);
}

pub(crate) fn register_diff_marker_fns<C: HistoryComponent + Diffable>(app: &mut App) {
    register_interpolated_marker_fns::<C>(app, write_history_diff::<C>);
}

/// Sets `C`'s receive fns under the interpolation markers.
fn register_interpolated_marker_fns<C: HistoryComponent>(app: &mut App, write: WriteFn<C>) {
    app.set_marker_fns::<Interpolated, C>(write, ConfirmedHistory::<C>::receive_removal);
    app.set_marker_fns::<InterpolatedSend, C>(write, ConfirmedHistory::<C>::receive_removal);
}

/// Writes a whole value into `C`'s confirmed history, not into the live component.
fn write_history<C: HistoryComponent>(
    ctx: &mut WriteCtx<'_>,
    rule_fns: &RuleFns<C>,
    entity: &mut DeferredEntity<'_>,
    message: &mut Bytes,
) -> Result<()> {
    ConfirmedHistory::receive_whole(ctx, rule_fns, entity, message, |_, _| {})
}

fn write_history_diff<C: HistoryComponent + Diffable>(
    ctx: &mut WriteCtx<'_>,
    _rule_fns: &RuleFns<C>,
    entity: &mut DeferredEntity<'_>,
    message: &mut Bytes,
) -> Result<()> {
    HistoryDiffReceiver::<C>::receive(ctx, entity, message, |_, _| {})
}
