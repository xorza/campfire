use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use lightyear::prelude::client::RawClient;
use lightyear::prelude::{Client, ReplicationReceiver};

/// What the sim client module gives a world, for every crate.
pub trait SimClientWorld {
    /// Spawns the client's entity, which its link and its connect name.
    fn spawn_client(&mut self) -> Entity;
}

impl SimClientWorld for World {
    fn spawn_client(&mut self) -> Entity {
        self.spawn((Client, RawClient, ReplicationReceiver)).id()
    }
}
