use crate::component_codec::ComponentCodec;
use crate::confirmed_history::ConfirmedHistory;
use bevy_app::App;
use bevy_ecs::entity::Entity;
use bevy_replicon::prelude::RepliconTick;
use bevy_replicon::shared::replication::diff::diff_index::DiffIndex;
use bytes::Bytes;
use campfire_net_core::NetTick;
use serde::Serialize;

use crate::confirmed_tick_map::ConfirmedTickMap;
use crate::fixtures::protocol::CompRepliconDiff;

/// Replicon's wire form of a diffed component's update, for tests that write it by hand.
#[derive(Debug, Serialize)]
enum ComponentDelta<'a> {
    Snapshot {
        index: DiffIndex,
        component: &'a CompRepliconDiff,
    },
    Diffs {
        index: DiffIndex,
        diffs: &'a [u32],
    },
}

/// A whole `component` at diff `index`.
pub fn diff_snapshot(index: u16, component: &CompRepliconDiff) -> Bytes {
    encode(&ComponentDelta::Snapshot {
        index: DiffIndex::new(index),
        component,
    })
}

/// The `diffs` that end at diff `index`.
pub fn diff_message(index: u16, diffs: &[u32]) -> Bytes {
    encode(&ComponentDelta::Diffs {
        index: DiffIndex::new(index),
        diffs,
    })
}

fn encode(delta: &ComponentDelta<'_>) -> Bytes {
    let mut message = Vec::new();
    ComponentCodec::write(delta, &mut message);
    message.into()
}

/// Maps replicon tick `tick` to the network tick of the same number.
pub fn record_confirmed_tick(app: &mut App, tick: u32) -> RepliconTick {
    let replicon_tick = RepliconTick::new(tick);
    app.world_mut()
        .resource_mut::<ConfirmedTickMap>()
        .record(replicon_tick, NetTick(tick));
    replicon_tick
}

/// The newest value `entity`'s confirmed history holds.
pub fn newest_confirmed_value(app: &App, entity: Entity) -> Option<u32> {
    app.world()
        .entity(entity)
        .get::<ConfirmedHistory<CompRepliconDiff>>()
        .and_then(ConfirmedHistory::newest_present)
        .map(|newest| newest.value.0)
}
