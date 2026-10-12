use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use lightyear::prelude::client::RawClient;
use lightyear::prelude::{Client, Link, LinkMtu, ReplicationReceiver};

use campfire_quic::LINK_MIN_MTU;

/// What the sim client module gives a world, for every crate.
pub trait SimClientWorld {
    /// Spawns the client's entity, which its link and its connect name. Its link takes
    /// `LINK_MIN_MTU` now, as its transport fixes its fragment size at the spawn, before any IO
    /// links it (design 15, D5).
    fn spawn_client(&mut self) -> Entity;
}

impl SimClientWorld for World {
    fn spawn_client(&mut self) -> Entity {
        self.spawn((
            Client,
            RawClient,
            ReplicationReceiver,
            Link::default().with_mtu(LinkMtu::new(LINK_MIN_MTU)),
        ))
        .id()
    }
}
