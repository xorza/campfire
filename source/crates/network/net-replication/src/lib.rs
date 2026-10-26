//! Replication of entities and their components over each link, on `bevy_replicon`: the targets
//! that choose which clients receive, predict or interpolate an entity, its hierarchy, its
//! visibility and rooms, its control, and pre-spawns.

#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

mod app_component_ext;
mod authoritative_tick;
#[cfg(feature = "internals")]
pub mod bench;
mod client_replication_plugin;
mod client_replication_plugins;
mod client_replicon_plugin;
mod component_codec;
mod component_registration;
mod component_registrator;
mod component_registry;
mod component_type_id;
mod confirmed_tick;
mod confirmed_tick_error;
mod confirmed_tick_map;
mod control_plugin;
mod controlled;
mod controlled_by;
mod controlled_lifetime;
mod controlled_send;
mod deferred_entity_commands;
mod delayed_despawn;
mod despawn;
mod detached_bit;
mod diffable;
mod disable_replicate_hierarchy;
#[cfg(feature = "internals")]
mod fixtures;
mod hello_channel;
mod hello_plugin;
mod hierarchy;
mod hierarchy_send_plugin;
mod history_component;
mod history_diff_receiver;
mod inheritance_plugin;
mod interpolated_despawn;
mod interpolation_yield;
mod manual_visibility;
mod network_visibility_plugin;
mod overridden;
mod persistent;
mod pre_spawned;
mod pre_spawned_plugin;
mod pre_spawned_receiver;
mod pre_spawned_systems;
mod remote_parent;
mod replicate_like;
mod replication_kind;
mod replication_mode;
mod replication_plugins;
mod replication_receiver;
mod replication_sender;
mod replication_systems;
mod replication_target;
mod replication_timer;
mod replicon_channel_map;
mod replicon_channel_registration_plugin;
mod replicon_mutation_acks_channel;
mod replicon_mutations_channel;
mod replicon_updates_channel;
mod room_allocator;
mod room_id;
mod room_plugin;
mod rooms;
mod sender_hello;
mod sent_marker;
mod server_replication_plugins;
mod server_replicon_plugin;
mod server_send_plugin;
mod shared_component_registration_plugin;
mod visibility_bits;
mod visibility_ext;

mod confirmed_history;
mod history_state;
mod interpolated;
mod predicted;
mod targets_overlap;

pub use crate::app_component_ext::AppComponentExt;
pub use crate::client_replication_plugins::ClientReplicationPlugins;
pub use crate::component_registration::ComponentRegistration;
pub use crate::component_registrator::ComponentRegistrator;
pub use crate::component_registry::ComponentRegistry;
pub use crate::component_type_id::ComponentTypeId;
pub use crate::confirmed_history::ConfirmedHistory;
pub use crate::confirmed_history::Iter;
pub use crate::confirmed_history::ReceivedValue;
pub use crate::confirmed_tick::ConfirmedTick;
pub use crate::confirmed_tick_map::ConfirmedTickFaults;
pub use crate::confirmed_tick_map::ConfirmedTickMap;
pub use crate::controlled::Controlled;
pub use crate::controlled_by::ControlledBy;
pub use crate::controlled_by::ControlledByRemote;
pub use crate::controlled_lifetime::ControlledLifetime;
pub use crate::controlled_send::ControlledSend;
pub use crate::deferred_entity_commands::DeferredEntityCommands;
pub use crate::delayed_despawn::DelayedDespawn;
pub use crate::diffable::Diffable;
pub use crate::disable_replicate_hierarchy::DisableReplicateHierarchy;
pub use crate::history_component::HistoryComponent;
pub use crate::history_diff_receiver::HistoryDiffError;
pub use crate::history_diff_receiver::HistoryDiffReceiver;
pub use crate::history_state::HistoryState;
pub use crate::interpolated::Interpolated;
pub use crate::interpolated_despawn::InterpolatedDespawn;
pub use crate::manual_visibility::ManualVisibility;
pub use crate::overridden::Overridden;
pub use crate::persistent::Persistent;
pub use crate::pre_spawned::PreSpawned;
pub use crate::pre_spawned_receiver::PreSpawnedReceiver;
pub use crate::pre_spawned_systems::PreSpawnedSystems;
pub use crate::predicted::Predicted;
pub use crate::replicate_like::ReplicateLike;
pub use crate::replicate_like::ReplicateLikeChildren;
pub use crate::replication_kind::DiffReplicated;
pub use crate::replication_kind::FullReplicated;
pub use crate::replication_kind::NotReplicated;
pub use crate::replication_kind::ReplicationKind;
pub use crate::replication_mode::ReplicationMode;
pub use crate::replication_plugins::ReplicationPlugins;
pub use crate::replication_receiver::ReplicationReceiver;
pub use crate::replication_sender::ReplicationSender;
pub use crate::replication_systems::ReplicationSystems;
pub use crate::replication_target::InterpolatedSend;
pub use crate::replication_target::InterpolationTarget;
pub use crate::replication_target::PredictedSend;
pub use crate::replication_target::PredictionTarget;
pub use crate::replication_target::Replicate;
pub use crate::replication_target::ReplicationTarget;
pub use crate::replication_target::TargetKind;
pub use crate::replication_timer::ReplicationTimer;
pub use crate::room_allocator::RoomAllocator;
pub use crate::room_id::RoomId;
pub use crate::room_plugin::RoomPlugin;
pub use crate::rooms::Rooms;
pub use crate::sender_hello::SenderHello;
pub use crate::server_replication_plugins::ServerReplicationPlugins;
pub use crate::visibility_ext::VisibilityExt;

/// What the tests and benches of the layers above share: this layer's stack, test protocol,
/// fixtures and measurements.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::fixtures::diff_messages::{
        diff_message, diff_snapshot, newest_confirmed_value, record_confirmed_tick,
    };
    pub use crate::fixtures::protocol::{
        CompA, CompMap, CompReplicateOnce, CompRepliconDiff, CompS, CompSimple,
        ReplicationProtocolPlugin,
    };
    pub use crate::fixtures::replication_counts::summed_replication_counts;
    pub use crate::fixtures::replication_stack::ReplicationStack;
    pub use crate::fixtures::replicon_apps::{replicon_app, shared_replicon_app};
    pub use crate::fixtures::steps::{ClientCopies, PROPAGATION_FRAMES};
}
