//! One workload each, on one remote client or on one part, audited a frame at a time.

mod idle;
mod messages;
mod prediction;
mod replication;
mod writer;

use bevy::prelude::Entity;
use bevy_replicon::shared::server_entity_map::ServerEntityMap;
use campfire_net_stack::internals::full_stack::FullStack;
use campfire_net_stepper::Order;
use campfire_net_stepper::Stepper;

/// What each frame of a connected link allocates: bevy_replicon's server mutation message, and
/// the client's acknowledgement as a `Vec` and then as `Bytes`.
const LINK: u64 = 3;

/// Steps the server first until `server_entity` reaches the client, and returns the client's
/// entity.
fn replicate_to_client(stepper: &mut Stepper<FullStack>, server_entity: Entity) -> Entity {
    for _ in 0..8 {
        stepper.step(1, stepper.frame_interval(), Order::ServerFirst);
        let mapped = stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied();
        if let Some(client_entity) = mapped {
            return client_entity;
        }
    }
    panic!("the server entity did not reach the client in 8 frames");
}
