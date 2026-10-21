//! One workload each, on one remote client or on one part, audited a frame at a time.

mod idle;
mod messages;
mod prediction;
mod replication;
mod writer;

use bevy::prelude::Entity;
use campfire_measure::AllocAudit;
use campfire_net_replication::internals::ClientCopies;
use campfire_net_stack::internals::FullStack;
use campfire_net_stepper::StepOrder;
use campfire_net_stepper::Stepper;

/// Frames before the measured run. The last buffer to grow is bevy_replicon's mutation message,
/// when its ticks reach 128 and their postcard varints take a second byte; each frame sends one,
/// and the frames before the audit only add to the count. The next growth, at 16384, is far past
/// the measured run.
const WARMUP: usize = 128;

/// Frames measured after the warmup, each held to the budget.
const FRAMES: usize = 256;

/// The audit of the stack: a fixed warmup past its buffers' last growth, then `FRAMES` frames
/// that each allocate nothing unless its fixture's budget allows it.
const AUDIT: AllocAudit = AllocAudit::new().warmup(WARMUP).frames(FRAMES);

/// What each frame of a connected link allocates: bevy_replicon's server mutation message, and
/// the client's acknowledgement as a `Vec` and then as `Bytes`.
const LINK: u64 = 3;

/// Steps the server first until `server_entity` reaches the client, and returns the client's
/// entity.
fn replicate_to_client(stepper: &mut Stepper<FullStack>, server_entity: Entity) -> Entity {
    for _ in 0..8 {
        stepper.step(1, stepper.frame_interval(), StepOrder::ServerFirst);
        let mapped = stepper.client_copy(0, server_entity);
        if let Some(client_entity) = mapped {
            return client_entity;
        }
    }
    panic!("the server entity did not reach the client in 8 frames");
}
