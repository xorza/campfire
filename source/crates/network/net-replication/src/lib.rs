//! Entity replication layer for the stack, built on top of [`bevy_replicon`].
//!
//! This crate handles replicating ECS entities and their components across the
//! network. It wraps `bevy_replicon`'s low-level replication machinery and adds
//! stack-specific features: prediction/interpolation targets, network
//! visibility, hierarchy inheritance, and pre-spawning.
//!
//! # Getting started
//!
//! Add [`Replicate`] to an entity to start replicating it. On the server, you
//! typically specify which clients should receive the entity:
//!
//! ```rust,ignore
//! commands.spawn((
//!     Replicate::to_clients(NetworkTarget::All),
//!     PredictionTarget::to_clients(NetworkTarget::single(client_id)),
//!     InterpolationTarget::to_clients(NetworkTarget::all_except([client_id])),
//!     MyComponent(42),
//! ));
//! ```
//!
//! # Key concepts
//!
//! ## Replication targets
//!
//! [`Replicate`] (alias for [`ReplicationTarget<()>`]) controls which peers
//! receive an entity. [`PredictionTarget`] and [`InterpolationTarget`] further
//! control which clients run prediction or interpolation for that entity.
//! `PredictionTarget` is the send-side way to enable prediction for an entity;
//! the receiver can also insert [`Predicted`] directly on the received entity.
//! Each target uses a [`ReplicationMode`] to specify the set of recipients.
//!
//! A [`ReplicationSender`] component must be present on the link entity
//! (the entity that represents the connection to a remote peer) to enable
//! outgoing replication through that link.
//!
//! ## Hierarchy inheritance
//!
//! When an entity with [`Replicate`] has children (via `ChildOf`), those
//! children automatically receive a [`ReplicateLike`] component pointing back
//! to the root. You can also add [`ReplicateLike`] on any entity manually.
//! Use [`DisableReplicateHierarchy`] on a child to opt out.
//!
//! A member follows the root's replication aspects: [`Replicate`], the
//! prediction and interpolation targets, rooms and manual visibility. A member
//! that joins with an aspect that differs from the root's, or that is given one
//! later, keeps it with [`Overridden`]. Remove
//! the marker to follow the root again.
//!
//! ## Visibility
//!
//! [`VisibilityExt::gain_visibility`] and [`VisibilityExt::lose_visibility`]
//! let you dynamically show or hide an entity for a specific client. The default
//! [`VisibilityExt::lose_visibility`] behavior despawns the remote entity. Use
//! [`VisibilityExt::lose_visibility_retained`] to retain an entity the client has already seen, or
//! [`VisibilityExt::lose_visibility_always_present`] to guarantee an initial remote entity even
//! when it starts hidden. Both retaining policies pause updates while hidden.
//! Members follow visibility changes through [`ReplicateLikeChildren`] so that
//! hiding a parent also hides its replicated descendants.
//!
//! For interest management based on spatial regions, see [`RoomPlugin`].
//!
//! ## Control
//!
//! [`ControlledBy`] marks which link entity "owns" a replicated entity.
//!
//! ## Pre-spawning
//!
//! [`PreSpawned`] allows both client and server to spawn the same entity independently, then match
//! them via a deterministic hash. This enables zero-latency predicted spawns (e.g. bullets,
//! projectiles). Sender-side [`PreSpawned::for_link`](crate::pre_spawned::PreSpawned::for_link)
//! scopes the matching signature to one remote client without changing the entity's replication
//! visibility. Rollback and expiry bookkeeping lives in the world-global [`PreSpawnedReceiver`]
//! resource.
//!
//! [`Replicate`]: crate::replication_target::Replicate [`ReplicationTarget<()>`]:
//! crate::replication_target::ReplicationTarget [`PredictionTarget`]:
//! crate::replication_target::PredictionTarget [`InterpolationTarget`]:
//! crate::replication_target::InterpolationTarget [`Predicted`]: crate::predicted::Predicted
//! [`ReplicationMode`]: crate::replication_mode::ReplicationMode [`ReplicationSender`]:
//! crate::replication_sender::ReplicationSender [`ReplicateLike`]:
//! crate::replicate_like::ReplicateLike [`DisableReplicateHierarchy`]:
//! crate::disable_replicate_hierarchy::DisableReplicateHierarchy [`ReplicateLikeChildren`]:
//! crate::replicate_like::ReplicateLikeChildren [`VisibilityExt::gain_visibility`]:
//! crate::visibility_ext::VisibilityExt::gain_visibility [`VisibilityExt::lose_visibility`]:
//! crate::visibility_ext::VisibilityExt::lose_visibility
//! [`VisibilityExt::lose_visibility_retained`]:
//! crate::visibility_ext::VisibilityExt::lose_visibility_retained
//! [`VisibilityExt::lose_visibility_always_present`]:
//! crate::visibility_ext::VisibilityExt::lose_visibility_always_present [`RoomPlugin`]:
//! crate::room_plugin::RoomPlugin [`ControlledBy`]: crate::controlled_by::ControlledBy
//! [`PreSpawned`]: crate::pre_spawned::PreSpawned

#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod app_component_ext;
#[cfg(feature = "internals")]
pub mod bench;
mod checkpoint;
mod checkpoint_error;
mod client_replication_plugin;
mod client_replication_plugins;
mod client_replicon_plugin;
mod completed_checkpoint;
mod component_codec;
mod component_kind;
mod component_registration;
mod component_registrator;
mod component_registry;
mod control_plugin;
mod controlled;
mod controlled_by;
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
mod history_diff_receiver;
mod history_write;
mod inheritance_plugin;
mod interpolated_despawn;
mod interpolation_yield;
mod lifetime;
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
mod replication_checkpoint_map;
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

pub use crate::app_component_ext::AppComponentExt;
pub use crate::client_replication_plugins::ClientReplicationPlugins;
pub use crate::completed_checkpoint::CompletedCheckpoint;
pub use crate::component_kind::ComponentKind;
pub use crate::component_registration::ComponentRegistration;
pub use crate::component_registrator::ComponentRegistrator;
pub use crate::component_registry::ComponentRegistry;
pub use crate::confirmed_history::ConfirmedHistory;
pub use crate::confirmed_history::Iter;
pub use crate::controlled::Controlled;
pub use crate::controlled_by::ControlledBy;
pub use crate::controlled_by::ControlledByRemote;
pub use crate::controlled_send::ControlledSend;
pub use crate::deferred_entity_commands::DeferredEntityCommands;
pub use crate::delayed_despawn::DelayedDespawn;
pub use crate::diffable::Diffable;
pub use crate::disable_replicate_hierarchy::DisableReplicateHierarchy;
pub use crate::hello_channel::HelloChannel;
pub use crate::history_diff_receiver::HistoryDiffError;
pub use crate::history_diff_receiver::HistoryDiffReceiver;
pub use crate::history_state::HistoryState;
pub use crate::history_write::{ReceivedValue, receive_history_diff, replicated_message_tick};
pub use crate::interpolated::Interpolated;
pub use crate::interpolated_despawn::InterpolatedDespawn;
pub use crate::lifetime::Lifetime;
pub use crate::manual_visibility::ManualVisibility;
pub use crate::overridden::Overridden;
pub use crate::persistent::Persistent;
pub use crate::pre_spawned::PreSpawned;
pub use crate::pre_spawned_receiver::PreSpawnedReceiver;
pub use crate::pre_spawned_systems::PreSpawnedSystems;
pub use crate::predicted::Predicted;
pub use crate::replicate_like::ReplicateLike;
pub use crate::replicate_like::ReplicateLikeChildren;
pub use crate::replication_checkpoint_map::CheckpointFaults;
pub use crate::replication_checkpoint_map::ReplicationCheckpointMap;
pub use crate::replication_kind::DiffReplicated;
pub use crate::replication_kind::FullReplicated;
pub use crate::replication_kind::NotReplicated;
pub use crate::replication_kind::Replication;
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
        diff_message, diff_snapshot, newest_confirmed_value, record_checkpoint,
    };
    pub use crate::fixtures::protocol::{
        CompA, CompMap, CompReplicateOnce, CompRepliconDiff, CompS, CompSimple,
        ReplicationProtocolPlugin,
    };
    pub use crate::fixtures::replication_counts::summed_replication_counts;
    pub use crate::fixtures::replication_stack::ReplicationStack;
    pub use crate::fixtures::steps::{
        PROPAGATION_FRAMES, client_copy, on_client, on_server, spawn_on_server,
    };
}
